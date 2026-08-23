use axum::{
    Json,
    extract::Path,
    http::StatusCode,
    routing::{get, post},
};
use serde::Deserialize;

use crate::{
    api::{
        ApiError, assemble_render_context, render_context_for_device, render_preview_png,
        require_auth,
    },
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

async fn update_device_firmware_updates(
    auth: AuthSession,
    Path(device_id): Path<i64>,
    Json(body): Json<UpdateCompatBody>,
) -> Result<StatusCode, ApiError> {
    require_auth(&auth)?;
    crate::db::update_device_firmware_updates_enabled(device_id, body.enabled).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn update_device_grayscale(
    auth: AuthSession,
    Path(device_id): Path<i64>,
    Json(body): Json<UpdateCompatBody>,
) -> Result<StatusCode, ApiError> {
    require_auth(&auth)?;
    crate::db::update_device_supports_2bit_grayscale(device_id, body.enabled).await?;
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
    require_auth(&auth)?;
    let ctx = render_context_for_device(id).await?;
    Ok(Json(render_preview_png(&ctx).await?))
}

async fn get_screen_preview_for_template(
    auth: AuthSession,
    Path((device_id, template_id)): Path<(i64, i64)>,
) -> Result<Json<String>, ApiError> {
    require_auth(&auth)?;
    let device = crate::db::get_device(device_id).await?;
    let template = crate::db::get_template_by_id(template_id).await?;
    let ctx = assemble_render_context(device, template).await?;
    Ok(Json(render_preview_png(&ctx).await?))
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
        .route(
            "/devices/{id}/firmware-updates",
            post(update_device_firmware_updates),
        )
        .route("/devices/{id}/grayscale", post(update_device_grayscale))
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
    async fn firmware_updates_toggle_round_trip() {
        let (router, cookie) = crate::api::test_support::login_session(super::router()).await;

        let suffix = format!("{}_{}", std::process::id(), line!());
        let device = crate::db::create_device(
            &format!("fw-toggle-api-token-{suffix}"),
            Some(&format!("aa:bb:cc:dd:ff:{suffix}")),
            Some("trmnl-og"),
            &format!("fw-toggle-api-device-{suffix}"),
            Some("1.0.0"),
            Some(800),
            Some(480),
            Some(3.9),
            Some("-60"),
        )
        .await
        .expect("create device fixture");
        assert!(!device.firmware_updates_enabled);

        let response = router
            .oneshot(
                Request::post(format!("/devices/{}/firmware-updates", device.id))
                    .header("cookie", &cookie)
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::json!({"enabled": true}).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        let updated = crate::db::get_device(device.id).await.expect("get device");
        assert!(updated.firmware_updates_enabled);
    }

    #[tokio::test]
    async fn grayscale_toggle_round_trip() {
        let (router, cookie) = crate::api::test_support::login_session(super::router()).await;

        let suffix = format!("{}_{}", std::process::id(), line!());
        let device = crate::db::create_device(
            &format!("grayscale-toggle-api-token-{suffix}"),
            Some(&format!("aa:bb:cc:dd:99:{suffix}")),
            Some("trmnl-og"),
            &format!("grayscale-toggle-api-device-{suffix}"),
            Some("1.0.0"),
            Some(800),
            Some(480),
            Some(3.9),
            Some("-60"),
        )
        .await
        .expect("create device fixture");
        assert!(!device.supports_2bit_grayscale);

        let response = router
            .oneshot(
                Request::post(format!("/devices/{}/grayscale", device.id))
                    .header("cookie", &cookie)
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::json!({"enabled": true}).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        let updated = crate::db::get_device(device.id).await.expect("get device");
        assert!(updated.supports_2bit_grayscale);
    }

    /// Builds a device with its own dedicated template, so preview renders
    /// don't depend on the shared lowest-id "default" template row that every
    /// test in the binary sees — see docs/testing.md#database-tests.
    async fn device_with_own_template(tag: &str, grayscale: bool) -> crate::models::Device {
        let suffix = format!("{}_{}_{}", std::process::id(), tag, line!());
        let device = crate::db::create_device(
            &format!("preview-token-{suffix}"),
            Some(&format!("aa:bb:cc:ee:{suffix}")),
            Some("trmnl-og"),
            &format!("preview-device-{suffix}"),
            Some("1.0.0"),
            Some(800),
            Some(480),
            Some(3.9),
            Some("-60"),
        )
        .await
        .expect("create device fixture");

        let template = crate::db::create_template(
            &format!("preview-template-{suffix}"),
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10" fill="black"/></svg>"#,
        )
        .await
        .expect("create template fixture");
        crate::db::update_device_template(device.id, template.id)
            .await
            .expect("assign template");

        if grayscale {
            crate::db::update_device_supports_2bit_grayscale(device.id, true)
                .await
                .expect("enable grayscale");
        }
        crate::db::get_device(device.id).await.expect("reload device")
    }

    /// Decodes a base64 preview payload and reports the raw PNG bit depth.
    /// Read via `png::Decoder` rather than `image`, which transparently
    /// expands sub-8-bit depths and so can't report the real one.
    fn preview_bit_depth(b64: &str) -> png::BitDepth {
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .expect("preview payload should be valid base64");
        let reader = png::Decoder::new(std::io::Cursor::new(&bytes[..]))
            .read_info()
            .expect("preview payload should be a PNG");
        reader.info().bit_depth
    }

    async fn get_preview_b64(
        router: axum::Router,
        cookie: &str,
        path: String,
    ) -> String {
        let response = router
            .oneshot(Request::get(path).header("cookie", cookie).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&body).expect("preview returns a bare base64 string")
    }

    #[tokio::test]
    async fn preview_for_a_grayscale_device_is_a_real_2bit_png() {
        let (router, cookie) = crate::api::test_support::login_session(super::router()).await;
        let device = device_with_own_template("gs", true).await;

        let b64 = get_preview_b64(router, &cookie, format!("/devices/{}/preview", device.id)).await;
        assert_eq!(
            preview_bit_depth(&b64),
            png::BitDepth::Two,
            "a grayscale-enabled device's preview must show what it will actually display"
        );
    }

    #[tokio::test]
    async fn preview_for_a_legacy_device_stays_bilevel() {
        let (router, cookie) = crate::api::test_support::login_session(super::router()).await;
        let device = device_with_own_template("legacy", false).await;

        let b64 = get_preview_b64(router, &cookie, format!("/devices/{}/preview", device.id)).await;
        assert_eq!(
            preview_bit_depth(&b64),
            png::BitDepth::Eight,
            "a 1-bit device's preview is an 8-bit PNG of the bilevel render, not a 2-bit one"
        );
    }

    #[tokio::test]
    async fn preview_for_template_follows_the_devices_grayscale_mode() {
        let (router, cookie) = crate::api::test_support::login_session(super::router()).await;
        let device = device_with_own_template("gs-tpl", true).await;

        let b64 = get_preview_b64(
            router,
            &cookie,
            format!("/devices/{}/preview/{}", device.id, device.template_id),
        )
        .await;
        assert_eq!(preview_bit_depth(&b64), png::BitDepth::Two);
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
