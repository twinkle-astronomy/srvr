pub mod ad_hoc_fetch;
pub mod auth;
pub mod claude;
pub mod devices;
pub mod firmware;
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

#[derive(Debug)]
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
        match &e {
            sqlx::Error::RowNotFound => Self(StatusCode::NOT_FOUND, "Not found".to_string()),
            sqlx::Error::Database(db)
                if db.kind() == sqlx::error::ErrorKind::UniqueViolation =>
            {
                Self(StatusCode::CONFLICT, "Already exists".to_string())
            }
            // Don't leak driver/database internals to the client; log them.
            _ => {
                tracing::error!("database error: {e}");
                Self(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".to_string(),
                )
            }
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

/// Full render context for a device (device + its assigned template + data
/// sources). Shared by the dashboard handlers and the device screen endpoint
/// (`/render/screen.bmp` in `device/api.rs`).
pub(crate) async fn render_context_for_device(id: i64) -> Result<RenderContext, ApiError> {
    let device = crate::db::get_device(id).await?;
    let template = crate::db::get_template_for_device(id).await?;
    assemble_render_context(device, template).await
}

pub fn router() -> axum::Router {
    axum::Router::new().nest(
        "/dashboard",
        axum::Router::new()
            .merge(ad_hoc_fetch::router())
            .merge(auth::router())
            .merge(claude::router())
            .merge(devices::router())
            .merge(firmware::router())
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

    /// Success-path fixture: merge the JSON auth endpoints into `routes`, wrap
    /// with sessions, create a fresh (process-unique) user in the shared test
    /// DB, and log in through the real login handler. Returns the wrapped
    /// router and the session cookie to send on authenticated requests — both
    /// are needed together, since the cookie only exists in this router's
    /// in-memory session store.
    pub(crate) async fn login_session(routes: axum::Router) -> (axum::Router, String) {
        use axum::{body::Body, http::Request};
        use tower::ServiceExt;

        crate::db::test_support::init_test_db().await;

        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let username = format!(
            "api_test_user_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        );
        let hash = crate::auth::hash_password("test-pw").expect("hash password");
        crate::db::create_user(&username, &hash).await.expect("create test user");

        let router = auth_router(routes.merge(crate::api::auth::router())).await;
        let response = router
            .clone()
            .oneshot(
                Request::post("/auth/login")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({"username": username, "password": "test-pw"})
                            .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(response.status().is_success(), "login failed: {}", response.status());
        let cookie = response
            .headers()
            .get("set-cookie")
            .expect("login should set a session cookie")
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_string();
        (router, cookie)
    }
}
