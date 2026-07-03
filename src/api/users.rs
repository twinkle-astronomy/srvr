use axum::{Json, extract::Path, http::StatusCode, routing::{delete, get}};

use crate::{
    api::{ApiError, require_auth},
    auth::AuthSession,
    models::AuthenticatedUser,
};

async fn get_all_users(auth: AuthSession) -> Result<Json<Vec<AuthenticatedUser>>, ApiError> {
    require_auth(&auth)?;
    let users = crate::db::get_users().await?;
    Ok(Json(
        users
            .into_iter()
            .map(|u| AuthenticatedUser {
                id: u.id,
                username: u.username,
            })
            .collect(),
    ))
}

async fn delete_user(
    auth: AuthSession,
    Path(user_id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    let current = require_auth(&auth)?;
    if current.id == user_id {
        return Err(ApiError(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Cannot delete yourself".to_string(),
        ));
    }
    let count = crate::db::user_count().await?;
    if count <= 1 {
        return Err(ApiError(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Cannot delete the last user".to_string(),
        ));
    }
    crate::db::delete_user(user_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub fn router() -> axum::Router {
    axum::Router::new()
        .route("/users", get(get_all_users))
        .route("/users/{id}", delete(delete_user))
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;

    #[tokio::test]
    async fn unauthenticated_returns_401() {
        let router = crate::api::test_support::auth_router(super::router()).await;
        let response = router
            .oneshot(Request::get("/users").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn authenticated_user_can_list_users() {
        let (router, cookie) = crate::api::test_support::login_session(super::router()).await;
        let response = router
            .oneshot(
                Request::get("/users")
                    .header("cookie", &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let users: Vec<crate::models::AuthenticatedUser> = serde_json::from_slice(&body).unwrap();
        assert!(
            users.iter().any(|u| u.username.starts_with("api_test_user_")),
            "list should contain the logged-in test user"
        );
    }

    #[tokio::test]
    async fn deleting_yourself_is_rejected() {
        let (router, cookie) = crate::api::test_support::login_session(super::router()).await;
        // Identify the session's own user via the auth endpoint.
        let response = router
            .clone()
            .oneshot(
                Request::get("/auth")
                    .header("cookie", &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let me: Option<crate::models::AuthenticatedUser> = serde_json::from_slice(&body).unwrap();
        let me = me.expect("session should resolve to a user");

        let response = router
            .oneshot(
                Request::delete(format!("/users/{}", me.id))
                    .header("cookie", &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }
}
