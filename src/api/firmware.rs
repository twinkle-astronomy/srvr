use axum::{
    Json,
    extract::{DefaultBodyLimit, Multipart, Path},
    http::StatusCode,
    routing::{get, post},
};

use crate::{
    api::{ApiError, require_auth},
    auth::AuthSession,
    models::FirmwareRelease,
};

/// Server-side guardrail, not a verified hardware ceiling — real ESP32 OTA
/// partitions are typically 1-4MB. See docs/projects/plans/firmware-ota-updates.md.
const MAX_FIRMWARE_UPLOAD_BYTES: usize = 16 * 1024 * 1024;

/// The stored filename is later interpolated into a `Content-Disposition:
/// attachment; filename="…"` header by the device download endpoint, so it
/// must never contain quotes or control characters — keep a conservative
/// allowlist and fall back to a constant when nothing survives.
fn sanitize_filename(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
        .collect();
    if cleaned.is_empty() {
        "firmware.bin".to_string()
    } else {
        cleaned
    }
}

async fn list_firmware_releases(
    auth: AuthSession,
) -> Result<Json<Vec<FirmwareRelease>>, ApiError> {
    require_auth(&auth)?;
    Ok(Json(crate::db::get_firmware_releases().await?))
}

async fn upload_firmware_release(
    auth: AuthSession,
    mut multipart: Multipart,
) -> Result<Json<FirmwareRelease>, ApiError> {
    require_auth(&auth)?;

    let mut model: Option<String> = None;
    let mut version: Option<String> = None;
    let mut file: Option<(String, Vec<u8>)> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::bad_request(e.to_string()))?
    {
        match field.name() {
            Some("model") => {
                model = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| ApiError::bad_request(e.to_string()))?,
                );
            }
            Some("version") => {
                version = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| ApiError::bad_request(e.to_string()))?,
                );
            }
            Some("file") => {
                let filename = sanitize_filename(field.file_name().unwrap_or("firmware.bin"));
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|e| ApiError::bad_request(e.to_string()))?;
                file = Some((filename, bytes.to_vec()));
            }
            _ => {}
        }
    }

    let model = model
        .filter(|m| !m.is_empty())
        .ok_or_else(|| ApiError::bad_request("model is required"))?;
    let version = version
        .filter(|v| !v.is_empty())
        .ok_or_else(|| ApiError::bad_request("version is required"))?;
    let (filename, bytes) = file.ok_or_else(|| ApiError::bad_request("file is required"))?;
    if bytes.is_empty() {
        return Err(ApiError::bad_request("file must not be empty"));
    }

    let release = crate::db::create_firmware_release(
        &model,
        &version,
        &filename,
        bytes.len() as i64,
        &bytes,
    )
    .await?;
    Ok(Json(release))
}

async fn activate_firmware_release(
    auth: AuthSession,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    require_auth(&auth)?;
    crate::db::activate_firmware_release(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_firmware_release(
    auth: AuthSession,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    require_auth(&auth)?;
    // 404 for an unknown id; then delete atomically on `active = 0` so a
    // concurrent activation between this check and the DELETE can't take
    // out the active release.
    let release = crate::db::get_firmware_release(id).await?;
    if release.active || !crate::db::delete_firmware_release_if_inactive(id).await? {
        return Err(ApiError::conflict(
            "cannot delete the active release for a model — activate a different version first",
        ));
    }
    Ok(StatusCode::NO_CONTENT)
}

pub fn router() -> axum::Router {
    axum::Router::new()
        .route(
            "/firmware",
            get(list_firmware_releases)
                .post(upload_firmware_release)
                .layer(DefaultBodyLimit::max(MAX_FIRMWARE_UPLOAD_BYTES)),
        )
        .route("/firmware/{id}/activate", post(activate_firmware_release))
        .route(
            "/firmware/{id}",
            axum::routing::delete(delete_firmware_release),
        )
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;

    use crate::models::FirmwareRelease;

    fn unique_model(tag: &str) -> String {
        format!("trmnl-{tag}-{}-{}", std::process::id(), line!())
    }

    /// Hand-builds a `multipart/form-data` body — the project has no
    /// multipart-capable HTTP client dev-dependency (reqwest's dev-dep here
    /// omits the `multipart` feature), and the raw format is small enough
    /// that this is simpler than adding one.
    fn multipart_body(model: &str, version: &str, filename: &str, content: &[u8]) -> (String, Vec<u8>) {
        let boundary = "----firmwaretestboundary";
        let mut body = Vec::new();
        body.extend_from_slice(
            format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\n{model}\r\n"
            )
            .as_bytes(),
        );
        body.extend_from_slice(
            format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"version\"\r\n\r\n{version}\r\n"
            )
            .as_bytes(),
        );
        body.extend_from_slice(
            format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: application/octet-stream\r\n\r\n"
            )
            .as_bytes(),
        );
        body.extend_from_slice(content);
        body.extend_from_slice(b"\r\n");
        body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
        (format!("multipart/form-data; boundary={boundary}"), body)
    }

    async fn upload(
        router: axum::Router,
        cookie: &str,
        model: &str,
        version: &str,
    ) -> axum::http::Response<Body> {
        let (content_type, body) = multipart_body(model, version, "fw.bin", b"firmware-bytes");
        router
            .oneshot(
                Request::post("/firmware")
                    .header("cookie", cookie)
                    .header("content-type", content_type)
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap()
    }

    #[test]
    fn sanitize_filename_keeps_normal_names_and_strips_header_breakers() {
        assert_eq!(super::sanitize_filename("fw-1.2.3_final.bin"), "fw-1.2.3_final.bin");
        // Quotes, CR/LF, and spaces would corrupt the Content-Disposition
        // header the download endpoint builds — they must not survive.
        assert_eq!(super::sanitize_filename("fw\" x\r\n.bin"), "fwx.bin");
        // Nothing salvageable → a safe constant, never an empty name.
        assert_eq!(super::sanitize_filename("\"\r\n "), "firmware.bin");
    }

    #[tokio::test]
    async fn activating_a_nonexistent_release_returns_404() {
        let (router, cookie) = crate::api::test_support::login_session(super::router()).await;
        let response = router
            .oneshot(
                Request::post("/firmware/999999999/activate")
                    .header("cookie", &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::NOT_FOUND,
            "activating a deleted/unknown release must not report success"
        );
    }

    #[tokio::test]
    async fn unauthenticated_returns_401() {
        let router = crate::api::test_support::auth_router(super::router()).await;
        let response = router
            .oneshot(Request::get("/firmware").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn upload_then_list_round_trip() {
        let (router, cookie) = crate::api::test_support::login_session(super::router()).await;
        let model = unique_model("upload-list");

        let response = upload(router.clone(), &cookie, &model, "1.0.0").await;
        assert_eq!(response.status(), StatusCode::OK, "upload should succeed");
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let created: FirmwareRelease = serde_json::from_slice(&body).unwrap();
        assert_eq!(created.model, model);
        assert_eq!(created.version, "1.0.0");
        assert!(!created.active, "newly uploaded releases start inactive");

        let response = router
            .oneshot(
                Request::get("/firmware")
                    .header("cookie", &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let releases: Vec<FirmwareRelease> = serde_json::from_slice(&body).unwrap();
        assert!(releases.iter().any(|r| r.id == created.id));
    }

    #[tokio::test]
    async fn duplicate_model_version_returns_409() {
        let (router, cookie) = crate::api::test_support::login_session(super::router()).await;
        let model = unique_model("dup");

        let first = upload(router.clone(), &cookie, &model, "1.0.0").await;
        assert_eq!(first.status(), StatusCode::OK);

        let second = upload(router, &cookie, &model, "1.0.0").await;
        assert_eq!(second.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn activate_flips_flag_and_deactivates_sibling() {
        let (router, cookie) = crate::api::test_support::login_session(super::router()).await;
        let model = unique_model("activate");

        let r1 = upload(router.clone(), &cookie, &model, "1.0.0").await;
        let r1: FirmwareRelease =
            serde_json::from_slice(&axum::body::to_bytes(r1.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        let r2 = upload(router.clone(), &cookie, &model, "2.0.0").await;
        let r2: FirmwareRelease =
            serde_json::from_slice(&axum::body::to_bytes(r2.into_body(), usize::MAX).await.unwrap())
                .unwrap();

        let response = router
            .clone()
            .oneshot(
                Request::post(format!("/firmware/{}/activate", r1.id))
                    .header("cookie", &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        let response = router
            .clone()
            .oneshot(
                Request::post(format!("/firmware/{}/activate", r2.id))
                    .header("cookie", &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        let response = router
            .oneshot(
                Request::get("/firmware")
                    .header("cookie", &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let releases: Vec<FirmwareRelease> = serde_json::from_slice(&body).unwrap();
        let r1_after = releases.iter().find(|r| r.id == r1.id).unwrap();
        let r2_after = releases.iter().find(|r| r.id == r2.id).unwrap();
        assert!(!r1_after.active, "activating r2 should deactivate r1");
        assert!(r2_after.active);
    }

    #[tokio::test]
    async fn delete_while_active_returns_409() {
        let (router, cookie) = crate::api::test_support::login_session(super::router()).await;
        let model = unique_model("delete-active");

        let r1 = upload(router.clone(), &cookie, &model, "1.0.0").await;
        let r1: FirmwareRelease =
            serde_json::from_slice(&axum::body::to_bytes(r1.into_body(), usize::MAX).await.unwrap())
                .unwrap();

        router
            .clone()
            .oneshot(
                Request::post(format!("/firmware/{}/activate", r1.id))
                    .header("cookie", &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let response = router
            .oneshot(
                Request::delete(format!("/firmware/{}", r1.id))
                    .header("cookie", &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn delete_inactive_release_succeeds() {
        let (router, cookie) = crate::api::test_support::login_session(super::router()).await;
        let model = unique_model("delete-inactive");

        let r1 = upload(router.clone(), &cookie, &model, "1.0.0").await;
        let r1: FirmwareRelease =
            serde_json::from_slice(&axum::body::to_bytes(r1.into_body(), usize::MAX).await.unwrap())
                .unwrap();

        let response = router
            .oneshot(
                Request::delete(format!("/firmware/{}", r1.id))
                    .header("cookie", &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }
}
