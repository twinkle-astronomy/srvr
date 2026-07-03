use axum::{
    Json,
    extract::Path,
    http::StatusCode,
    routing::{get, post},
};
use serde::Deserialize;

use crate::{
    api::{ApiError, assemble_render_context, render_context_for_device, require_auth},
    auth::AuthSession,
    models::{Device, DeviceLog, RenderContext},
};

#[derive(Deserialize)]
struct UpdateTemplateBody {
    template_id: i64,
}

#[derive(Deserialize)]
struct UpdateCompatBody {
    enabled: bool,
}

async fn get_devices(auth: AuthSession) -> Result<Json<Vec<Device>>, ApiError> {
    require_auth(&auth)?;
    Ok(Json(crate::db::get_devices().await?))
}

async fn get_device_by_id(
    auth: AuthSession,
    Path(id): Path<i64>,
) -> Result<Json<Device>, ApiError> {
    require_auth(&auth)?;
    Ok(Json(crate::db::get_device(id).await?))
}

async fn get_device_logs(
    auth: AuthSession,
    Path(id): Path<i64>,
) -> Result<Json<Vec<DeviceLog>>, ApiError> {
    require_auth(&auth)?;
    Ok(Json(crate::db::get_device_logs(id, 100).await?))
}

async fn delete_device(auth: AuthSession, Path(id): Path<i64>) -> Result<StatusCode, ApiError> {
    require_auth(&auth)?;
    crate::db::delete_device(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn update_device_template(
    auth: AuthSession,
    Path(device_id): Path<i64>,
    Json(body): Json<UpdateTemplateBody>,
) -> Result<StatusCode, ApiError> {
    require_auth(&auth)?;
    crate::db::update_device_template(device_id, body.template_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn update_device_compat(
    auth: AuthSession,
    Path(device_id): Path<i64>,
    Json(body): Json<UpdateCompatBody>,
) -> Result<StatusCode, ApiError> {
    require_auth(&auth)?;
    crate::db::update_device_maximum_compatibility(device_id, body.enabled).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_render_context(
    auth: AuthSession,
    Path(id): Path<i64>,
) -> Result<Json<RenderContext>, ApiError> {
    require_auth(&auth)?;
    Ok(Json(render_context_for_device(id).await?))
}

async fn get_render_context_for_template(
    auth: AuthSession,
    Path((device_id, template_id)): Path<(i64, i64)>,
) -> Result<Json<RenderContext>, ApiError> {
    require_auth(&auth)?;
    let device = crate::db::get_device(device_id).await?;
    let template = crate::db::get_template_by_id(template_id).await?;
    Ok(Json(assemble_render_context(device, template).await?))
}

async fn get_screen_preview(
    auth: AuthSession,
    Path(id): Path<i64>,
) -> Result<Json<String>, ApiError> {
    use base64::Engine;
    require_auth(&auth)?;
    let ctx = render_context_for_device(id).await?;
    let bmp = crate::device::renderer::render_screen(&ctx)
        .await
        .map_err(|e| ApiError::internal(format!("{e:?}")))?;
    Ok(Json(base64::engine::general_purpose::STANDARD.encode(&bmp)))
}

async fn get_screen_preview_for_template(
    auth: AuthSession,
    Path((device_id, template_id)): Path<(i64, i64)>,
) -> Result<Json<String>, ApiError> {
    use base64::Engine;
    require_auth(&auth)?;
    let device = crate::db::get_device(device_id).await?;
    let template = crate::db::get_template_by_id(template_id).await?;
    let ctx = assemble_render_context(device, template).await?;
    let bmp = crate::device::renderer::render_screen(&ctx)
        .await
        .map_err(|e| ApiError::internal(format!("{e:?}")))?;
    Ok(Json(base64::engine::general_purpose::STANDARD.encode(&bmp)))
}

pub fn router() -> axum::Router {
    axum::Router::new()
        .route("/devices", get(get_devices))
        .route(
            "/devices/{id}",
            get(get_device_by_id).delete(delete_device),
        )
        .route("/devices/{id}/logs", get(get_device_logs))
        .route("/devices/{id}/template", post(update_device_template))
        .route("/devices/{id}/compat", post(update_device_compat))
        .route("/devices/{id}/render-context", get(get_render_context))
        .route(
            "/devices/{id}/render-context/{template_id}",
            get(get_render_context_for_template),
        )
        .route("/devices/{id}/preview", get(get_screen_preview))
        .route(
            "/devices/{id}/preview/{template_id}",
            get(get_screen_preview_for_template),
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
    async fn unauthenticated_returns_401() {
        let router = crate::api::test_support::auth_router(super::router()).await;
        let response = router
            .oneshot(Request::get("/devices").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn authenticated_devices_list_returns_json() {
        let (router, cookie) = crate::api::test_support::login_session(super::router()).await;
        let response = router
            .oneshot(
                Request::get("/devices")
                    .header("cookie", &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        // Shared test DB: assert the shape parses, not the count.
        let _devices: Vec<crate::models::Device> = serde_json::from_slice(&body).unwrap();
    }

    #[tokio::test]
    async fn unknown_device_returns_404() {
        let (router, cookie) = crate::api::test_support::login_session(super::router()).await;
        let response = router
            .oneshot(
                Request::get("/devices/999999999")
                    .header("cookie", &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
