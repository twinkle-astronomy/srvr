use axum::{
    Json,
    extract::Path,
    http::StatusCode,
    routing::{get, post},
};
use serde::Deserialize;

use crate::{
    api::{ApiError, assemble_render_context, render_preview_png, require_auth},
    auth::AuthSession,
    frontend::server_fns::TemplateVar,
    models::{RenderContext, Template},
};

#[derive(Deserialize)]
struct CreateTemplateBody {
    name: String,
    content: String,
}

#[derive(Deserialize)]
struct UpdateTemplateBody {
    name: String,
    content: String,
}

async fn get_templates(auth: AuthSession) -> Result<Json<Vec<Template>>, ApiError> {
    require_auth(&auth)?;
    Ok(Json(crate::db::get_templates().await?))
}

async fn get_default_template(auth: AuthSession) -> Result<Json<Template>, ApiError> {
    require_auth(&auth)?;
    Ok(Json(crate::db::get_default_template().await?))
}

async fn get_template_by_id(
    auth: AuthSession,
    Path(id): Path<i64>,
) -> Result<Json<Template>, ApiError> {
    require_auth(&auth)?;
    Ok(Json(crate::db::get_template_by_id(id).await?))
}

async fn create_template(
    auth: AuthSession,
    Json(body): Json<CreateTemplateBody>,
) -> Result<Json<Template>, ApiError> {
    require_auth(&auth)?;
    Ok(Json(
        crate::db::create_template(&body.name, &body.content).await?,
    ))
}

async fn copy_template(
    auth: AuthSession,
    Path(id): Path<i64>,
) -> Result<Json<Template>, ApiError> {
    require_auth(&auth)?;
    Ok(Json(crate::db::copy_template(id).await?))
}

async fn save_template(
    auth: AuthSession,
    Path(id): Path<i64>,
    Json(body): Json<UpdateTemplateBody>,
) -> Result<StatusCode, ApiError> {
    require_auth(&auth)?;
    crate::db::update_template(id, &body.name, &body.content).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_template(
    auth: AuthSession,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    require_auth(&auth)?;
    crate::db::delete_template(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_virtual_render_context(
    auth: AuthSession,
    Path(id): Path<i64>,
) -> Result<Json<RenderContext>, ApiError> {
    require_auth(&auth)?;
    let device = crate::models::Device::virtual_device();
    let template = crate::db::get_template_by_id(id).await?;
    Ok(Json(assemble_render_context(device, template).await?))
}

/// Base64 PNG preview of an arbitrary render context, in whichever mode
/// `ctx.device` is configured for. Always PNG (see `render_preview_png`),
/// which is also why there's no longer a separate `/preview/png`: this one
/// response serves both the `<img>` in the editor and Claude's vision input
/// in the AI generator, so the two can't drift and the SVG is only
/// rasterized once per render.
async fn get_template_preview(
    auth: AuthSession,
    Json(ctx): Json<RenderContext>,
) -> Result<Json<String>, ApiError> {
    require_auth(&auth)?;
    Ok(Json(render_preview_png(&ctx).await?))
}

async fn get_template_context(
    auth: AuthSession,
    Json(ctx): Json<RenderContext>,
) -> Result<Json<Vec<TemplateVar>>, ApiError> {
    use crate::frontend::server_fns::utils::obj_to_template_var;
    require_auth(&auth)?;
    let obj = crate::device::renderer::render_vars(&ctx)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    let mut vars = vec![];
    obj_to_template_var(&String::new(), &mut vars, &obj);
    Ok(Json(vars))
}

pub fn router() -> axum::Router {
    axum::Router::new()
        .route("/templates", get(get_templates).post(create_template))
        .route("/templates/default", get(get_default_template))
        .route(
            "/templates/{id}",
            get(get_template_by_id)
                .put(save_template)
                .delete(delete_template),
        )
        .route("/templates/{id}/copy", post(copy_template))
        .route(
            "/templates/{id}/virtual-render-context",
            get(get_virtual_render_context),
        )
        .route("/preview", post(get_template_preview))
        .route("/context", post(get_template_context))
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
            .oneshot(Request::get("/templates").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn create_then_fetch_template_round_trips() {
        let (router, cookie) = crate::api::test_support::login_session(super::router()).await;
        let name = format!("api_tpl_roundtrip_{}", std::process::id());
        let response = router
            .clone()
            .oneshot(
                Request::post("/templates")
                    .header("cookie", &cookie)
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "name": name,
                            "content": "<svg xmlns=\"http://www.w3.org/2000/svg\"/>"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(response.status().is_success(), "create: {}", response.status());
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let created: crate::models::Template = serde_json::from_slice(&body).unwrap();
        assert_eq!(created.name, name);

        let response = router
            .oneshot(
                Request::get(format!("/templates/{}", created.id))
                    .header("cookie", &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let fetched: crate::models::Template = serde_json::from_slice(&body).unwrap();
        assert_eq!(fetched.name, name);
    }

    /// A `RenderContext` POST body with a mid-gray fill — the fill matters:
    /// at luminance 100 the 1-bit path thresholds it to black while the 2-bit
    /// path keeps it at level 85, so the two modes are distinguishable by
    /// pixel value, not just by header.
    fn preview_ctx_json(grayscale: bool) -> serde_json::Value {
        serde_json::json!({
            "device": {
                "id": 0, "access_token": "", "mac_address": "00:00:00:00:00:00",
                "model": "Virtual", "friendly_id": "virtual-device", "fw_version": null,
                "width": 10, "height": 10, "battery_voltage": null, "rssi": null,
                "template_id": 0, "maximum_compatibility": false, "firmware_updates_enabled": false,
                "supports_2bit_grayscale": grayscale,
                "last_seen_at": "", "created_at": ""
            },
            "template": {
                "id": 0, "name": "preview-png-test",
                "content": "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"10\" height=\"10\"><rect width=\"10\" height=\"10\" fill=\"rgb(100,100,100)\"/></svg>",
                "created_at": "2026-01-01T00:00:00", "updated_at": "2026-01-01T00:00:00"
            },
            "prometheus_queries": [],
            "range_queries": [],
            "http_sources": []
        })
    }

    async fn post_preview(router: axum::Router, cookie: &str, ctx: serde_json::Value) -> Vec<u8> {
        let response = router
            .oneshot(
                Request::post("/preview")
                    .header("cookie", cookie)
                    .header("content-type", "application/json")
                    .body(Body::from(ctx.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let b64: String = serde_json::from_slice(&body).unwrap();
        use base64::Engine;
        base64::engine::general_purpose::STANDARD
            .decode(&b64)
            .expect("preview payload should be valid base64")
    }

    #[tokio::test]
    async fn preview_for_a_grayscale_context_is_a_real_2bit_png() {
        let (router, cookie) = crate::api::test_support::login_session(super::router()).await;
        let bytes = post_preview(router, &cookie, preview_ctx_json(true)).await;

        // Raw depth via png::Decoder — image's decoder expands sub-8-bit
        // depths transparently and can't report the real one.
        let reader = png::Decoder::new(std::io::Cursor::new(&bytes[..]))
            .read_info()
            .expect("preview should be a PNG");
        assert_eq!(reader.info().bit_depth, png::BitDepth::Two);

        let mid = image::load_from_memory(&bytes).expect("decode png").to_luma8();
        assert_eq!(
            mid.get_pixel(5, 5).0[0],
            85,
            "the 2-bit preview must keep mid-gray as a gray level, not threshold it"
        );
    }

    #[tokio::test]
    async fn preview_for_a_bilevel_context_thresholds_to_black_and_white() {
        let (router, cookie) = crate::api::test_support::login_session(super::router()).await;
        let bytes = post_preview(router, &cookie, preview_ctx_json(false)).await;

        let reader = png::Decoder::new(std::io::Cursor::new(&bytes[..]))
            .read_info()
            .expect("preview should be a PNG");
        assert_eq!(reader.info().bit_depth, png::BitDepth::Eight);

        let decoded = image::load_from_memory(&bytes).expect("decode png");
        assert_eq!(decoded.width(), 10);
        assert_eq!(decoded.height(), 10);
        assert_eq!(
            decoded.to_luma8().get_pixel(5, 5).0[0],
            0,
            "the 1-bit preview must still threshold mid-gray to black, unchanged from before"
        );
    }

    #[tokio::test]
    async fn unknown_template_returns_404() {
        let (router, cookie) = crate::api::test_support::login_session(super::router()).await;
        let response = router
            .oneshot(
                Request::get("/templates/999999999")
                    .header("cookie", &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
