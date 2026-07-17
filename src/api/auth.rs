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

#[derive(Deserialize)]
struct ClaudeApiKeyBody {
    key: String,
}

/// Reports only whether the calling user has a key configured. The key
/// itself never leaves the server — the browser calls Claude through the
/// `/claude/messages` proxy (src/api/claude.rs), which attaches it
/// server-side.
async fn has_claude_api_key_json(auth: AuthSession) -> Result<Json<bool>, ApiError> {
    let user = require_auth(&auth)?;
    let key = crate::db::get_claude_api_key(user.id).await?;
    Ok(Json(key.is_some()))
}

async fn save_claude_api_key_json(
    auth: AuthSession,
    Json(body): Json<ClaudeApiKeyBody>,
) -> Result<StatusCode, ApiError> {
    let user = require_auth(&auth)?;
    if body.key.is_empty() {
        return Err(ApiError::bad_request("Key required"));
    }
    crate::db::set_claude_api_key(user.id, &body.key).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_claude_api_key_json(auth: AuthSession) -> Result<StatusCode, ApiError> {
    let user = require_auth(&auth)?;
    crate::db::clear_claude_api_key(user.id).await?;
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
        .route(
            "/claude-api-key",
            get(has_claude_api_key_json)
                .put(save_claude_api_key_json)
                .delete(delete_claude_api_key_json),
        )
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

    #[tokio::test]
    async fn duplicate_username_returns_409_with_friendly_message() {
        // login_session merges this module's router in, so pass an empty one.
        let (router, cookie) =
            crate::api::test_support::login_session(axum::Router::new()).await;
        let name = format!("dup_user_{}", std::process::id());
        let body = serde_json::json!({"username": name, "password": "pw"}).to_string();
        let request = |b: String| {
            Request::post("/auth/create-user")
                .header("cookie", &cookie)
                .header("content-type", "application/json")
                .body(Body::from(b))
                .unwrap()
        };

        let first = router.clone().oneshot(request(body.clone())).await.unwrap();
        assert_eq!(first.status(), StatusCode::CREATED);

        let second = router.oneshot(request(body)).await.unwrap();
        assert_eq!(second.status(), StatusCode::CONFLICT);
        let bytes = axum::body::to_bytes(second.into_body(), usize::MAX).await.unwrap();
        let err: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        // User-facing message, not database driver internals.
        assert_eq!(err["error"], "Already exists");
    }

    #[tokio::test]
    async fn claude_api_key_requires_auth() {
        let router = crate::api::test_support::auth_router(super::router()).await;
        let response = router
            .oneshot(Request::get("/claude-api-key").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn claude_api_key_round_trip_never_returns_the_key() {
        // login_session merges this module's router in, so pass an empty one.
        let (router, cookie) = crate::api::test_support::login_session(axum::Router::new()).await;

        // GET reports only whether a key is configured — the key itself must
        // never leave the server (the browser talks to Claude through the
        // /claude/messages proxy, which attaches it server-side).
        let read_has_key = |router: axum::Router, cookie: String| async move {
            let response = router
                .oneshot(
                    Request::get("/claude-api-key")
                        .header("cookie", &cookie)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let body = String::from_utf8(bytes.to_vec()).unwrap();
            assert!(
                !body.contains("sk-ant"),
                "response must not contain the key, got: {body}"
            );
            serde_json::from_str::<bool>(&body).expect("body should be a bare JSON boolean")
        };

        // No key set yet.
        assert!(!read_has_key(router.clone(), cookie.clone()).await);

        // Save a key.
        let response = router
            .clone()
            .oneshot(
                Request::put("/claude-api-key")
                    .header("cookie", &cookie)
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({"key": "sk-ant-test-key"}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        // Configured now — but still no key material in the response.
        assert!(read_has_key(router.clone(), cookie.clone()).await);

        // Delete it.
        let response = router
            .clone()
            .oneshot(
                Request::delete("/claude-api-key")
                    .header("cookie", &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        // Gone.
        assert!(!read_has_key(router, cookie).await);
    }

    #[tokio::test]
    async fn empty_claude_api_key_is_rejected() {
        // login_session merges this module's router in, so pass an empty one.
        let (router, cookie) = crate::api::test_support::login_session(axum::Router::new()).await;
        let response = router
            .oneshot(
                Request::put("/claude-api-key")
                    .header("cookie", &cookie)
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::json!({"key": ""}).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
