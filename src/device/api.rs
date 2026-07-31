use std::borrow::Cow;
use std::convert::Infallible;
use std::sync::OnceLock;

use axum::{
    Router,
    extract::{Json, Path, Query, Request},
    http::{HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
    routing::{get, post},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{Local, Timelike};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tokio_stream::{StreamExt, wrappers::BroadcastStream};

use tracing::{error, info};

use crate::time::Clock;
use crate::{
    db::{get_device_id_by_access_token, insert_device_logs},
    device::{create_device_from_headers, get_and_update_device_from_headers, renderer},
    api::render_context_for_device,
    hmac::{generate_signature_bytes, validate_signature},
    models::{DeviceLog, DeviceLogEntry, FirmwareRelease},
    time::RealClock,
};

/// Whether a device should be told to update, and which release to serve if
/// so. Returns `None` when updates are opted out, there's no active release
/// for the device's model, or the device already reports that release's
/// version — `Some(release)` only for a genuine version mismatch on an
/// opted-in device.
fn decide_firmware_update<'a>(
    firmware_updates_enabled: bool,
    device_fw_version: Option<&str>,
    active_release: Option<&'a FirmwareRelease>,
) -> Option<&'a FirmwareRelease> {
    if !firmware_updates_enabled {
        return None;
    }
    let release = active_release?;
    if Some(release.version.as_str()) == device_fw_version {
        return None;
    }
    Some(release)
}

#[derive(Clone, Debug)]
struct LogBroadcastMessage {
    device_id: i64,
    logs: Vec<DeviceLog>,
}

#[derive(Clone, Debug, Serialize)]
struct DeviceBroadcastMessage {
    device: crate::models::Device,
}

static LOG_CHANNEL: OnceLock<broadcast::Sender<LogBroadcastMessage>> = OnceLock::new();
static DEVICE_CHANNEL: OnceLock<broadcast::Sender<DeviceBroadcastMessage>> = OnceLock::new();
static TLS_ENABLED: OnceLock<bool> = OnceLock::new();

fn log_sender() -> &'static broadcast::Sender<LogBroadcastMessage> {
    LOG_CHANNEL.get_or_init(|| {
        let (tx, _rx) = broadcast::channel(256);
        tx
    })
}

fn device_sender() -> &'static broadcast::Sender<DeviceBroadcastMessage> {
    DEVICE_CHANNEL.get_or_init(|| {
        let (tx, _rx) = broadcast::channel(64);
        tx
    })
}

fn get_effective_host(headers: &HeaderMap) -> Cow<'_, str> {
    if let Ok(host) = std::env::var("SERVER_HOST") {
        return Cow::Owned(host);
    }
    Cow::Borrowed(
        headers
            .get("x-forwarded-host")
            .or_else(|| headers.get("host"))
            .and_then(|h| h.to_str().ok())
            .unwrap_or("localhost:8080"),
    )
}

async fn connection_close(req: Request, next: Next) -> Response {
    let mut res = next.run(req).await;
    res.headers_mut().insert(
        axum::http::header::CONNECTION,
        axum::http::HeaderValue::from_static("close"),
    );
    res
}

pub fn router<T: Clone + Send + Sync + 'static>(tls_enabled: bool) -> Router<T> {
    use std::time::Duration;
    use tower_http::timeout::TimeoutLayer;

    TLS_ENABLED.get_or_init(|| tls_enabled);
    let device_routes = Router::new()
        .route("/api/display", get(display_handler))
        .route("/api/log", post(log_handler))
        .route("/api/setup", get(setup_handler))
        .route("/api/setup/", get(setup_handler))
        .route("/render/screen.bmp", get(render_screen_handler))
        .route("/firmware/download", get(firmware_download_handler))
        .layer(TimeoutLayer::with_status_code(axum::http::StatusCode::REQUEST_TIMEOUT, Duration::from_secs(30)))
        .layer(middleware::from_fn(connection_close));

    Router::new()
        .merge(device_routes)
        .route("/api/devices/{id}/logs/stream", get(log_stream_handler))
        .route("/api/devices/stream", get(device_stream_handler))
}

#[derive(Debug, Serialize)]
struct DisplayResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    image_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    filename: Option<String>,
    refresh_rate: u32,
    update_firmware: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    firmware_url: Option<String>,
    maximum_compatibility: bool,
}

#[derive(Deserialize)]
struct LogRequest {
    logs: Vec<DeviceLogEntry>,
}

#[derive(Serialize)]
struct SetupResponse {
    status: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    friendly_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    image_url: Option<String>,
    message: String,
}

fn generate_access_token() -> String {
    crate::hmac::random_hex_token()
}

// GET /api/display - Fetch the next screen
async fn display_handler(headers: HeaderMap) -> impl IntoResponse {
    // Log all request headers
    info!("=== GET /api/display - Request Headers ===");
    for (key, value) in headers.iter() {
        if let Ok(val_str) = value.to_str() {
            info!("  {}: {}", key, val_str);
        } else {
            info!("  {}: <non-UTF8 value>", key);
        }
    }

    let device = match get_and_update_device_from_headers(&headers).await {
        Ok(d) => d,
        Err(crate::device::Error::MissingAccessToken) => {
            error!("Missing access token");
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "error": "Missing Access-Token header"
                })),
            )
                .into_response();
        }
        Err(crate::device::Error::SqlxError(e)) => {
            error!("Error: {:?}", e);
            match e {
                sqlx::Error::RowNotFound => {
                    return (
                        StatusCode::UNAUTHORIZED,
                        Json(serde_json::json!({
                            "status": 403,
                            "error": "Unauthorized"
                        })),
                    )
                        .into_response();
                }
                e => {
                    error!("Error: {:?}", e);
                    return (
                        StatusCode::OK,
                        Json(serde_json::json!({
                            "status": 500,
                            "error": format!("{:?}", e)
                        })),
                    )
                        .into_response();
                }
            }
        }
        Err(e) => {
            error!("Error: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("{:?}", e)
                })),
            )
                .into_response();
        }
    };

    let host = get_effective_host(&headers);

    // Add timestamp for cache busting and device dimensions
    let real_clock = RealClock;
    let timestamp = real_clock.now_secs();
    let scheme = if *TLS_ENABLED.get().unwrap_or(&false) {
        "https"
    } else {
        "http"
    };

    // Generate HMAC signature for the image URL
    let secret = crate::hmac::signing_secret();
    let signed_bytes = generate_signature_bytes(secret, device.id, real_clock.clone());
    let sig_encoded = URL_SAFE_NO_PAD.encode(&signed_bytes);

    let image_url = format!(
        "{}://{}/render/screen.bmp?device_id={}&t={}&sig={}",
        scheme, host, device.id, timestamp, sig_encoded
    );

    // Opted-out devices (the default) skip the release lookup entirely —
    // this handler runs on every poll of every device.
    let active_release = if device.firmware_updates_enabled {
        match crate::db::get_active_firmware_release(&device.model).await {
            Ok(r) => r,
            Err(e) => {
                error!("Error looking up active firmware release: {:?}", e);
                None
            }
        }
    } else {
        None
    };
    let firmware_release = decide_firmware_update(
        device.firmware_updates_enabled,
        device.fw_version.as_deref(),
        active_release.as_ref(),
    );
    let firmware_url = firmware_release.map(|_| {
        format!(
            "{}://{}/firmware/download?device_id={}&t={}&sig={}",
            scheme, host, device.id, timestamp, sig_encoded
        )
    });

    let response = DisplayResponse {
        image_url: Some(image_url),
        filename: Some(format!("screen_{}.bmp", timestamp)),
        refresh_rate: (60 - Local::now().second()) as u32,
        update_firmware: firmware_release.is_some(),
        firmware_url,
        maximum_compatibility: device.maximum_compatibility,
    };
    info!("Response: {:?}", response);
    (StatusCode::OK, Json(response)).into_response()
}

// POST /api/log - Log with logs[] array
async fn log_handler(headers: HeaderMap, Json(payload): Json<LogRequest>) -> impl IntoResponse {
    // Extract required Access-Token header
    let access_token = match headers.get("Access-Token") {
        Some(token) => token.to_str().unwrap_or(""),
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };

    info!("Received {} log(s) from device", payload.logs.len());
    for entry in &payload.logs {
        info!("Log entry: {:?}", entry);
    }

    let device_id = match get_device_id_by_access_token(access_token).await {
        Ok(Some(id)) => id,
        Ok(None) => return StatusCode::UNAUTHORIZED.into_response(),
        Err(e) => {
            error!("Error looking up device: {:?}", e);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let log_count = payload.logs.len() as i64;

    if let Err(e) = insert_device_logs(device_id, &payload.logs).await {
        error!("Error inserting device logs: {:?}", e);
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    // Best-effort broadcast for SSE subscribers
    if log_sender().receiver_count() > 0 {
        match crate::db::get_device_logs(device_id, log_count).await {
            Ok(logs) => {
                let _ = log_sender().send(LogBroadcastMessage { device_id, logs });
            }
            Err(e) => {
                tracing::warn!("Failed to query back logs for broadcast: {:?}", e);
            }
        }
    }

    StatusCode::NO_CONTENT.into_response()
}

// GET /api/setup - Set up device
async fn setup_handler(headers: HeaderMap) -> impl IntoResponse {
    let access_token = generate_access_token();

    info!("=== GET /api/setup - Request Headers ===");
    for (key, value) in headers.iter() {
        if let Ok(val_str) = value.to_str() {
            info!("  {}: {}", key, val_str);
        } else {
            info!("  {}: <non-UTF8 value>", key);
        }
    }

    let device = match create_device_from_headers(&access_token, &headers).await {
        Ok(d) => d,
        Err(crate::device::Error::MissingAccessToken) => {
            error!("Missing access token");
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "error": "Missing Access-Token header"
                })),
            )
                .into_response();
        }
        Err(e) => {
            error!("Error: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("{:?}", e)
                })),
            )
                .into_response();
        }
    };

    info!(
        "Setup request - MAC: {:?}, Model: {:?}, FriendlyID: {}, FW: {:?}, Battery: {:?}, RSSI: {:?}, Width: {:?}, Height: {:?}",
        device.mac_address,
        device.model,
        device.friendly_id,
        device.fw_version,
        device.battery_voltage,
        device.rssi,
        device.width,
        device.height
    );

    // Broadcast new device for SSE subscribers
    if device_sender().receiver_count() > 0 {
        let _ = device_sender().send(DeviceBroadcastMessage {
            device: device.clone(),
        });
    }

    let host = get_effective_host(&headers);

    // Add timestamp for cache busting and device dimensions
    let real_clock = RealClock;
    let timestamp = real_clock.now_secs();
    let scheme = if *TLS_ENABLED.get().unwrap_or(&false) {
        "https"
    } else {
        "http"
    };

    // Generate HMAC signature for the image URL
    let secret = crate::hmac::signing_secret();
    let signed_bytes = generate_signature_bytes(secret, device.id, real_clock.clone());
    let sig_encoded = URL_SAFE_NO_PAD.encode(&signed_bytes);

    let image_url = format!(
        "{}://{}/render/screen.bmp?device_id={}&t={}&sig={}",
        scheme, host, device.id, timestamp, sig_encoded
    );

    let response = SetupResponse {
        status: 200,
        api_key: Some(device.access_token),
        friendly_id: Some(device.friendly_id),
        image_url: Some(image_url),
        message: "Device setup successful".to_string(),
    };

    (StatusCode::OK, Json(response)).into_response()
}

#[derive(Deserialize)]
struct RenderQuery {
    device_id: i64,
    #[serde(default)]
    t: Option<i64>,
    #[serde(default)]
    sig: Option<String>,
}

/// Shared HMAC gate for the signed device URLs (`/render/screen.bmp`,
/// `/firmware/download`): presence, decoding, and validity of the `t`/`sig`
/// query params against the device+timestamp scoped signature. `Err` is the
/// 401 response to return as-is.
fn check_signed_request(params: &RenderQuery) -> Result<(), Response> {
    let unauthorized = |msg: &str| {
        Err((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": msg })),
        )
            .into_response())
    };

    let Some(timestamp) = params.t else {
        return unauthorized("Missing timestamp parameter");
    };
    let signed_bytes = match &params.sig {
        Some(sig) => match base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(sig) {
            Ok(bytes) => bytes,
            Err(_) => return unauthorized("Invalid signature encoding"),
        },
        None => return unauthorized("Missing signature parameter"),
    };

    let secret = crate::hmac::signing_secret();
    if !validate_signature(secret, params.device_id, &signed_bytes, timestamp, RealClock) {
        return unauthorized("Invalid or expired signature");
    }
    Ok(())
}

// GET /render/screen.bmp - Render screen image with HMAC validation
async fn render_screen_handler(Query(params): Query<RenderQuery>) -> impl IntoResponse {
    if let Err(response) = check_signed_request(&params) {
        return response;
    }

    let render_context = match render_context_for_device(params.device_id).await {
        Ok(d) => d,
        Err(e) => {
            error!("Error: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, format!("{:?}", e)).into_response();
        }
    };

    match renderer::render_screen(&render_context).await {
        Ok(image) => (StatusCode::OK, [("Content-Type", "image/bmp")], image).into_response(),
        Err(e) => {
            error!("Error: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, format!("{:?}", e)).into_response()
        }
    }
}

// GET /firmware/download - Download the active firmware binary for the
// requesting device's model. Shares the signed-URL gate with
// /render/screen.bmp (same device+timestamp scoped signature).
async fn firmware_download_handler(Query(params): Query<RenderQuery>) -> impl IntoResponse {
    if let Err(response) = check_signed_request(&params) {
        return response;
    }

    let device = match crate::db::get_device(params.device_id).await {
        Ok(d) => d,
        Err(e) => {
            error!("Error loading device for firmware download: {:?}", e);
            return (StatusCode::NOT_FOUND, "Device not found").into_response();
        }
    };

    // No active release matching device.model at download time — an admin
    // may have deactivated it between the /api/display poll and this
    // request. Treat as transient: the device will retry on its next poll.
    match crate::db::get_active_firmware_binary(&device.model).await {
        Ok(Some((filename, binary))) => (
            StatusCode::OK,
            [
                ("Content-Type", "application/octet-stream".to_string()),
                (
                    "Content-Disposition",
                    format!("attachment; filename=\"{filename}\""),
                ),
            ],
            binary,
        )
            .into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "No active firmware release").into_response(),
        Err(e) => {
            error!("Error loading firmware binary: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response()
        }
    }
}

// GET /api/devices/:id/logs/stream - SSE stream of new logs for a device
async fn log_stream_handler(
    Path(device_id): Path<i64>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let rx = log_sender().subscribe();
    let stream = BroadcastStream::new(rx).filter_map(move |msg| match msg {
        Ok(msg) if msg.device_id == device_id => {
            let json = serde_json::to_string(&msg.logs).unwrap_or_default();
            Some(Ok(Event::default().data(json).event("logs")))
        }
        _ => None,
    });

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("ping"),
    )
}

// GET /api/devices/stream - SSE stream of newly added devices
async fn device_stream_handler() -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>>
{
    let rx = device_sender().subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|msg| match msg {
        Ok(msg) => {
            let json = serde_json::to_string(&msg.device).unwrap_or_default();
            Some(Ok(Event::default().data(json).event("device_added")))
        }
        _ => None,
    });

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("ping"),
    )
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use std::sync::{Arc, Mutex};
    use tower::ServiceExt;

    use crate::time::MockClock;

    /// Captures everything a `tracing` subscriber writes, so tests can
    /// assert on the content of `info!`/`error!` log lines.
    #[derive(Clone, Default)]
    struct CapturingWriter(Arc<Mutex<Vec<u8>>>);

    impl CapturingWriter {
        fn contents(&self) -> String {
            String::from_utf8(self.0.lock().expect("lock captured log buffer").clone())
                .expect("log output should be valid utf8")
        }
    }

    impl std::io::Write for CapturingWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().expect("lock captured log buffer").extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for CapturingWriter {
        type Writer = CapturingWriter;
        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    fn sample_release(version: &str) -> FirmwareRelease {
        FirmwareRelease {
            id: 1,
            model: "trmnl-og".to_string(),
            version: version.to_string(),
            filename: "fw.bin".to_string(),
            size_bytes: 10,
            active: true,
            created_at: String::new(),
        }
    }

    #[test]
    fn decide_firmware_update_version_mismatch_returns_release() {
        let release = sample_release("2.0.0");
        let decision = decide_firmware_update(true, Some("1.0.0"), Some(&release));
        assert_eq!(decision, Some(&release));
    }

    #[test]
    fn decide_firmware_update_version_match_returns_none() {
        let release = sample_release("1.0.0");
        let decision = decide_firmware_update(true, Some("1.0.0"), Some(&release));
        assert_eq!(decision, None, "device already has the active version");
    }

    #[test]
    fn decide_firmware_update_disabled_returns_none_even_with_newer_release() {
        let release = sample_release("2.0.0");
        let decision = decide_firmware_update(false, Some("1.0.0"), Some(&release));
        assert_eq!(
            decision, None,
            "opted-out devices must never be told to update"
        );
    }

    #[test]
    fn decide_firmware_update_no_active_release_returns_none() {
        let decision = decide_firmware_update(true, Some("1.0.0"), None);
        assert_eq!(decision, None);
    }

    #[tokio::test]
    async fn firmware_download_without_active_release_returns_404() {
        crate::hmac::init_signing_secret("device-api-test-secret".to_string());
        crate::db::test_support::init_test_db().await;

        let suffix = format!("{}_{}", std::process::id(), line!());
        let device = crate::db::create_device(
            &format!("fw-dl-token-{suffix}"),
            Some(&format!("aa:bb:cc:dd:aa:{suffix}")),
            Some("trmnl-og"),
            &format!("fw-dl-device-{suffix}"),
            Some("1.0.0"),
            Some(800),
            Some(480),
            Some(3.9),
            Some("-60"),
        )
        .await
        .expect("create device fixture");

        // Sign with the same fixed timestamp we put in the URL — validation
        // inside the handler always checks against the real clock, so this
        // must be a real "now" value, not an arbitrary mock one.
        let real_now = RealClock.now_secs();
        let mock = MockClock { time: real_now };
        let secret = crate::hmac::signing_secret();
        let sig_bytes = generate_signature_bytes(secret, device.id, mock.clone());
        let sig = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&sig_bytes);

        let router = super::router::<()>(false);
        let uri = format!(
            "/firmware/download?device_id={}&t={}&sig={}",
            device.id, real_now, sig
        );
        let response = router
            .oneshot(Request::get(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::NOT_FOUND,
            "no active release for this model should 404, not error"
        );
    }

    #[tokio::test]
    async fn display_handler_reports_update_on_version_mismatch_when_enabled() {
        crate::hmac::init_signing_secret("device-api-test-secret".to_string());
        crate::db::test_support::init_test_db().await;

        let suffix = format!("{}_{}", std::process::id(), line!());
        let model = format!("trmnl-display-{suffix}");
        let mac = format!("aa:bb:cc:dd:cc:{suffix}");
        let device = crate::db::create_device(
            &format!("fw-display-token-{suffix}"),
            Some(&mac),
            Some(&model),
            &format!("fw-display-device-{suffix}"),
            Some("1.0.0"),
            Some(800),
            Some(480),
            Some(3.9),
            Some("-60"),
        )
        .await
        .expect("create device fixture");

        crate::db::update_device_firmware_updates_enabled(device.id, true)
            .await
            .expect("enable firmware updates");

        let release = crate::db::create_firmware_release(&model, "2.0.0", "fw.bin", 4, b"data")
            .await
            .expect("create release");
        crate::db::activate_firmware_release(release.id)
            .await
            .expect("activate release");

        let router = super::router::<()>(false);
        let response = router
            .oneshot(
                Request::get("/api/display")
                    .header("Access-Token", &device.access_token)
                    .header("ID", &mac)
                    .header("model", &model)
                    .header("FW-Version", "1.0.0")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["update_firmware"], serde_json::json!(true));
        assert!(
            json["firmware_url"]
                .as_str()
                .is_some_and(|u| u.contains("/firmware/download")),
            "expected a firmware_url pointing at the download route, got: {json:?}"
        );
    }

    #[tokio::test]
    async fn display_handler_does_not_report_update_when_opted_out() {
        crate::hmac::init_signing_secret("device-api-test-secret".to_string());
        crate::db::test_support::init_test_db().await;

        let suffix = format!("{}_{}", std::process::id(), line!());
        let model = format!("trmnl-display-optout-{suffix}");
        let mac = format!("aa:bb:cc:dd:dd:{suffix}");
        let device = crate::db::create_device(
            &format!("fw-display-optout-token-{suffix}"),
            Some(&mac),
            Some(&model),
            &format!("fw-display-optout-device-{suffix}"),
            Some("1.0.0"),
            Some(800),
            Some(480),
            Some(3.9),
            Some("-60"),
        )
        .await
        .expect("create device fixture");
        // firmware_updates_enabled defaults to false — left untouched.

        let release = crate::db::create_firmware_release(&model, "2.0.0", "fw.bin", 4, b"data")
            .await
            .expect("create release");
        crate::db::activate_firmware_release(release.id)
            .await
            .expect("activate release");

        let router = super::router::<()>(false);
        let response = router
            .oneshot(
                Request::get("/api/display")
                    .header("Access-Token", &device.access_token)
                    .header("ID", &mac)
                    .header("model", &model)
                    .header("FW-Version", "1.0.0")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["update_firmware"], serde_json::json!(false));
        assert!(json.get("firmware_url").is_none() || json["firmware_url"].is_null());
    }

    #[tokio::test]
    async fn log_handler_logs_submitted_entry_content() {
        crate::db::test_support::init_test_db().await;

        let suffix = format!("{}_{}", std::process::id(), line!());
        let device = crate::db::create_device(
            &format!("log-content-token-{suffix}"),
            Some(&format!("aa:bb:cc:dd:ee:{suffix}")),
            Some("trmnl-og"),
            &format!("log-content-device-{suffix}"),
            Some("1.0.0"),
            Some(800),
            Some(480),
            Some(3.9),
            Some("-60"),
        )
        .await
        .expect("create device fixture");

        let writer = CapturingWriter::default();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(writer.clone())
            .with_ansi(false)
            .finish();
        let guard = tracing::subscriber::set_default(subscriber);

        let fingerprint = format!("log-message-fingerprint-{suffix}");
        let body = serde_json::json!({
            "logs": [{ "message": fingerprint, "wake_reason": "button_press" }]
        });

        let router = super::router::<()>(false);
        let response = router
            .oneshot(
                Request::post("/api/log")
                    .header("Access-Token", &device.access_token)
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        drop(guard);
        let output = writer.contents();
        assert!(
            output.contains(&fingerprint),
            "expected log output to contain the submitted log entry's message, got: {output}"
        );
    }

    #[tokio::test]
    async fn setup_handler_logs_parsed_device_fields() {
        crate::hmac::init_signing_secret("device-api-test-secret".to_string());
        crate::db::test_support::init_test_db().await;

        let suffix = format!("{}_{}", std::process::id(), line!());
        let mac = format!("aa:bb:cc:dd:ff:{suffix}");

        let writer = CapturingWriter::default();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(writer.clone())
            .with_ansi(false)
            .finish();
        let guard = tracing::subscriber::set_default(subscriber);

        // An unparseable Battery-Voltage is silently swallowed to `None` by
        // `create_device_from_headers` (`.and_then(|x| x.parse().ok())`).
        // The raw header dump above already echoes the garbage value back,
        // so it can't distinguish "logs the request" from "logs what was
        // actually parsed" — asserting on the parsed `None` can only pass
        // once the parsed-fields line exists.
        let router = super::router::<()>(false);
        let response = router
            .oneshot(
                Request::get("/api/setup")
                    .header("ID", &mac)
                    .header("model", "trmnl-og")
                    .header("Battery-Voltage", "not-a-number")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        drop(guard);
        let output = writer.contents();
        assert!(
            output.contains("Battery: None"),
            "expected setup log output to show the parsed (failed) Battery-Voltage as None, got: {output}"
        );
    }

    #[tokio::test]
    async fn setup_handler_matches_path_with_trailing_slash() {
        crate::hmac::init_signing_secret("device-api-test-secret".to_string());
        crate::db::test_support::init_test_db().await;

        let suffix = format!("{}_{}", std::process::id(), line!());
        let mac = format!("aa:bb:cc:dd:00:{suffix}");

        let router = super::router::<()>(false);
        let response = router
            .oneshot(
                Request::get("/api/setup/")
                    .header("ID", &mac)
                    .header("model", "trmnl-og")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "GET /api/setup/ (trailing slash) should route to the same handler as /api/setup"
        );
    }

    #[tokio::test]
    async fn setup_handler_defaults_missing_model_and_dimensions() {
        crate::hmac::init_signing_secret("device-api-test-secret".to_string());
        crate::db::test_support::init_test_db().await;

        let suffix = format!("{}_{}", std::process::id(), line!());
        let mac = format!("aa:bb:cc:dd:33:{suffix}");

        // Some real-world firmware sends only ID + FW-Version on setup — no
        // model/Width/Height/Battery-Voltage/RSSI at all.
        let router = super::router::<()>(false);
        let response = router
            .oneshot(
                Request::get("/api/setup")
                    .header("ID", &mac)
                    .header("FW-Version", "1.5.12")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "a device that omits model/width/height headers should still complete setup"
        );

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let access_token = json["api_key"]
            .as_str()
            .expect("successful setup returns an api_key")
            .to_string();

        let device_id = crate::db::get_device_id_by_access_token(&access_token)
            .await
            .expect("lookup device")
            .expect("device exists");
        let device = crate::db::get_device(device_id).await.expect("fetch device");

        assert_eq!(device.model, "unknown");
        assert_eq!(device.width, 800);
        assert_eq!(device.height, 480);
    }

    #[tokio::test]
    async fn display_handler_survives_a_poll_missing_model_and_dimensions() {
        crate::hmac::init_signing_secret("device-api-test-secret".to_string());
        crate::db::test_support::init_test_db().await;

        let suffix = format!("{}_{}", std::process::id(), line!());
        let mac = format!("aa:bb:cc:dd:44:{suffix}");
        let device = crate::db::create_device(
            &format!("poll-preserve-token-{suffix}"),
            Some(&mac),
            Some("trmnl-og"),
            &format!("poll-preserve-device-{suffix}"),
            Some("1.0.0"),
            Some(800),
            Some(480),
            Some(3.9),
            Some("-60"),
        )
        .await
        .expect("create device fixture");

        // A later poll from the same device omits model/Width/Height this
        // time (as the real device in this bug report does on every
        // request) — it must not clobber the already-known values.
        let router = super::router::<()>(false);
        let response = router
            .oneshot(
                Request::get("/api/display")
                    .header("Access-Token", &device.access_token)
                    .header("ID", &mac)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(
            json.get("image_url").and_then(|v| v.as_str()).is_some(),
            "poll missing model/dimensions headers should still succeed, got: {json:?}"
        );

        let refreshed = crate::db::get_device(device.id).await.expect("fetch device");
        assert_eq!(
            refreshed.model, "trmnl-og",
            "a poll missing the model header must not clobber a previously known model"
        );
        assert_eq!(refreshed.width, 800);
        assert_eq!(refreshed.height, 480);
    }
}
