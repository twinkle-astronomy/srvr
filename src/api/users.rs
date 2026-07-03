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
}
