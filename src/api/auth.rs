use axum::{
    Json,
    http::StatusCode,
    routing::{get, post},
};
use serde::Deserialize;

use crate::{
    api::{ApiError, require_auth},
    auth::AuthSession,
    frontend::server_fns::ServerInfo,
    models::AuthenticatedUser,
};

async fn check_auth(auth: AuthSession) -> Result<Json<Option<AuthenticatedUser>>, ApiError> {
    let user = auth.user.map(|u| AuthenticatedUser {
        id: u.id,
        username: u.username,
    });
    Ok(Json(user))
}

async fn check_needs_setup() -> Result<Json<bool>, ApiError> {
    let count = crate::db::user_count().await?;
    Ok(Json(count == 0))
}

async fn get_server_info(auth: AuthSession) -> Result<Json<ServerInfo>, ApiError> {
    require_auth(&auth)?;
    let now = chrono::Utc::now();
    let prometheus_url =
        std::env::var("PROMETHEUS_URL").unwrap_or_else(|_| "http://prometheus:9090".to_string());
    let port = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080u16);
    Ok(Json(ServerInfo {
        time: now.format("%H:%M:%S UTC").to_string(),
        date: now.format("%Y-%m-%d").to_string(),
        prometheus_url,
        port,
    }))
}

#[derive(Deserialize)]
struct LoginBody {
    username: String,
    password: String,
}

#[derive(Deserialize)]
struct ChangePasswordBody {
    current_password: String,
    new_password: String,
}

async fn login_json(
    mut auth: AuthSession,
    Json(body): Json<LoginBody>,
) -> Result<Json<AuthenticatedUser>, ApiError> {
    let creds = crate::auth::Credentials {
        username: body.username,
        password: body.password,
    };
    match auth.authenticate(creds).await {
        Ok(Some(user)) => {
            auth.login(&user).await.map_err(|e| ApiError::internal(e))?;
            Ok(Json(AuthenticatedUser {
                id: user.id,
                username: user.username,
            }))
        }
        Ok(None) => Err(ApiError::unauthorized()),
        Err(e) => Err(ApiError::internal(e)),
    }
}

async fn logout_json(mut auth: AuthSession) -> Result<StatusCode, ApiError> {
    auth.logout().await.ok();
    Ok(StatusCode::NO_CONTENT)
}

async fn setup_json(
    mut auth: AuthSession,
    Json(body): Json<LoginBody>,
) -> Result<Json<AuthenticatedUser>, ApiError> {
    let count = crate::db::user_count().await?;
    if count > 0 {
        return Err(ApiError::conflict("Setup already complete"));
    }
    if body.username.is_empty() || body.password.is_empty() {
        return Err(ApiError::bad_request("Username and password required"));
    }
    let hash = crate::auth::hash_password(&body.password).map_err(|e| ApiError::internal(e))?;
    let user = crate::db::create_user(&body.username, &hash).await?;
    auth.login(&user).await.map_err(|e| ApiError::internal(e))?;
    Ok(Json(AuthenticatedUser {
        id: user.id,
        username: user.username,
    }))
}

async fn create_user_json(
    auth: AuthSession,
    Json(body): Json<LoginBody>,
) -> Result<StatusCode, ApiError> {
    require_auth(&auth)?;
    if body.username.is_empty() || body.password.is_empty() {
        return Err(ApiError::bad_request("Username and password required"));
    }
    let hash = crate::auth::hash_password(&body.password).map_err(|e| ApiError::internal(e))?;
    crate::db::create_user(&body.username, &hash).await?;
    Ok(StatusCode::CREATED)
}

async fn change_password_json(
    auth: AuthSession,
    Json(body): Json<ChangePasswordBody>,
) -> Result<StatusCode, ApiError> {
    let user = require_auth(&auth)?;
    use argon2::{Argon2, PasswordHash, PasswordVerifier};
    let parsed_hash =
        PasswordHash::new(&user.password_hash).map_err(|_| ApiError::internal("Invalid stored hash"))?;
    if Argon2::default()
        .verify_password(body.current_password.as_bytes(), &parsed_hash)
        .is_err()
    {
        return Err(ApiError::unauthorized());
    }
    if body.new_password.is_empty() {
        return Err(ApiError::bad_request("New password required"));
    }
    let new_hash =
        crate::auth::hash_password(&body.new_password).map_err(|e| ApiError::internal(e))?;
    crate::db::update_user_password(user.id, &new_hash).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub fn router() -> axum::Router {
    axum::Router::new()
        .route("/auth", get(check_auth))
        .route("/auth/login", post(login_json))
        .route("/auth/logout", post(logout_json))
        .route("/auth/setup", post(setup_json))
        .route("/auth/create-user", post(create_user_json))
        .route("/auth/change-password", post(change_password_json))
        .route("/needs-setup", get(check_needs_setup))
        .route("/server-info", get(get_server_info))
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;

    #[tokio::test]
    async fn check_auth_returns_null_when_unauthenticated() {
        let router = crate::api::test_support::auth_router(super::router()).await;
        let response = router
            .oneshot(Request::get("/auth").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn server_info_requires_auth() {
        let router = crate::api::test_support::auth_router(super::router()).await;
        let response = router
            .oneshot(Request::get("/server-info").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn logout_without_session_returns_no_content() {
        let router = crate::api::test_support::auth_router(super::router()).await;
        let response = router
            .oneshot(Request::post("/auth/logout").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }
}
