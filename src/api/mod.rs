pub mod auth;
pub mod devices;
pub mod http_sources;
pub mod prometheus;
pub mod range;
pub mod templates;
pub mod users;

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

use crate::auth::AuthSession;
use crate::models::{Device, RenderContext, Template};

pub struct ApiError(StatusCode, String);

impl ApiError {
    fn unauthorized() -> Self {
        Self(StatusCode::UNAUTHORIZED, "Unauthorized".to_string())
    }

    fn internal(msg: impl std::fmt::Display) -> Self {
        Self(StatusCode::INTERNAL_SERVER_ERROR, msg.to_string())
    }

    fn bad_request(msg: impl std::fmt::Display) -> Self {
        Self(StatusCode::BAD_REQUEST, msg.to_string())
    }

    fn conflict(msg: impl std::fmt::Display) -> Self {
        Self(StatusCode::CONFLICT, msg.to_string())
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> Self {
        match e {
            sqlx::Error::RowNotFound => Self(StatusCode::NOT_FOUND, "Not found".to_string()),
            _ => Self(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({"error": self.1}))).into_response()
    }
}

pub(crate) fn require_auth(auth: &AuthSession) -> Result<&crate::models::User, ApiError> {
    auth.user.as_ref().ok_or_else(ApiError::unauthorized)
}

pub(crate) async fn assemble_render_context(
    device: Device,
    template: Template,
) -> Result<RenderContext, ApiError> {
    let prometheus_queries = crate::db::get_prometheus_queries(template.id).await?;
    let range_queries = crate::db::get_range_queries(template.id).await?;
    let http_sources = crate::db::get_http_sources(template.id).await?;
    Ok(RenderContext {
        device,
        template,
        prometheus_queries,
        range_queries,
        http_sources,
    })
}

pub fn router() -> axum::Router {
    axum::Router::new().nest(
        "/dashboard",
        axum::Router::new()
            .merge(auth::router())
            .merge(devices::router())
            .merge(http_sources::router())
            .merge(prometheus::router())
            .merge(range::router())
            .merge(templates::router())
            .merge(users::router()),
    )
}

#[cfg(all(test, feature = "server"))]
pub(crate) mod test_support {
    /// Wraps an API sub-router with the session + auth middleware so unauthenticated
    /// requests return 401 rather than an extractor error.
    ///
    /// Uses an in-memory session store so it doesn't touch the shared SQLite pool
    /// (session migrations on the sqlx pool corrupt the _sqlx_migrations table and
    /// cause other tests that rely on the pool to fail).
    pub(crate) async fn auth_router(routes: axum::Router) -> axum::Router {
        let session_store = tower_sessions::MemoryStore::default();
        let session_layer =
            tower_sessions::SessionManagerLayer::new(session_store).with_secure(false);
        let auth_layer =
            axum_login::AuthManagerLayerBuilder::new(crate::auth::Backend, session_layer).build();
        routes.layer(auth_layer)
    }
}
