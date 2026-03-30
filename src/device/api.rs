use std::borrow::Cow;
use std::convert::Infallible;
use std::sync::OnceLock;

use axum::{
    Router,
    extract::{Json, Path, Query},
    http::{HeaderMap, StatusCode},
    response::{
        IntoResponse,
        sse::{Event, KeepAlive, Sse},
    },
    routing::{get, post},
};
use chrono::{Local, Timelike, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tokio_stream::{StreamExt, wrappers::BroadcastStream};

use tracing::{error, info};

use crate::{
    db::{get_device_id_by_access_token, insert_device_logs},
    device::{create_device_from_headers, get_and_update_device_from_headers, renderer},
    frontend::server_fns::get_render_context,
    models::{DeviceLog, DeviceLogEntry},
};

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

pub fn router<T: Clone + Send + Sync + 'static>(tls_enabled: bool) -> Router<T> {
    TLS_ENABLED.get_or_init(|| tls_enabled);
    Router::new()
        .route("/api/display", get(display_handler))
        .route("/api/log", post(log_handler))
        .route("/api/setup", get(setup_handler))
        .route("/render/screen.bmp", get(render_screen_handler))
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
    use std::io::Read;
    let mut buf = [0u8; 32];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut buf).map(|_| ()))
        .expect("Failed to read /dev/urandom");
    buf.iter().map(|b| format!("{:02x}", b)).collect()
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
    let timestamp = Utc::now().timestamp();
    let scheme = if *TLS_ENABLED.get().unwrap_or(&false) {
        "https"
    } else {
        "http"
    };
    let image_url = format!(
        "{}://{}/render/screen.bmp?device_id={}&t={}",
        scheme, host, device.id, timestamp
    );
    let response = DisplayResponse {
        image_url: Some(image_url),
        filename: Some(format!("screen_{}.bmp", timestamp)),
        refresh_rate: 60 - Local::now().second(),
        update_firmware: false,
        maximum_compatibility: device.maximum_compatibility,
    };

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
        "Setup request - MAC: {}, Model: {}, FriendlyID: {}",
        device.mac_address, device.model, device.friendly_id
    );

    // Broadcast new device for SSE subscribers
    if device_sender().receiver_count() > 0 {
        let _ = device_sender().send(DeviceBroadcastMessage {
            device: device.clone(),
        });
    }

    let host = get_effective_host(&headers);

    // Add timestamp for cache busting and device dimensions
    let timestamp = Utc::now().timestamp();
    let scheme = if *TLS_ENABLED.get().unwrap_or(&false) {
        "https"
    } else {
        "http"
    };
    let image_url = format!(
        "{}://{}/render/screen.bmp?device_id={}&t={}",
        scheme, host, device.id, timestamp
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
}

// GET /render/screen.bmp - Render screen image
async fn render_screen_handler(Query(params): Query<RenderQuery>) -> impl IntoResponse {
    let render_context = match get_render_context(params.device_id).await {
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
