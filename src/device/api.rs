use axum::{
    Router,
    extract::{Json, Query},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use tracing::{error, info};

use crate::{
    db::{get_device, get_device_id_by_access_token, get_template, insert_device_logs},
    device::{create_device_from_headers, get_and_update_device_from_headers, renderer},
    models::DeviceLogEntry,
};

pub fn router<T: Clone + Send + Sync + 'static>() -> Router<T> {
    Router::new()
        .route("/api/display", get(display_handler))
        .route("/api/log", post(log_handler))
        .route("/api/setup", get(setup_handler))
        .route("/render/screen.bmp", get(render_screen_handler))
}

#[derive(Serialize)]
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
            return (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": 500,
                    "error": format!("{:?}", e)
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

    // Get the host from the request headers to build the correct image URL
    let host = headers
        .get("host")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("localhost:8080");

    // Add timestamp for cache busting and device dimensions
    let timestamp = Utc::now().timestamp();
    let image_url = format!(
        "http://{}/render/screen.bmp?device_id={}&t={}",
        host, device.id, timestamp
    );

    let response = DisplayResponse {
        image_url: Some(image_url),
        filename: Some(format!("screen_{}.bmp", timestamp)),
        refresh_rate: 1 * 60,
        update_firmware: false,
        maximum_compatibility: false,
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

    if let Err(e) = insert_device_logs(device_id, &payload.logs).await {
        error!("Error inserting device logs: {:?}", e);
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
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

    // Get the host from the request headers to build the correct image URL
    let host = headers
        .get("host")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("localhost:8080");

    // Add timestamp for cache busting and device dimensions
    let timestamp = Utc::now().timestamp();
    let image_url = format!(
        "http://{}/render/screen.bmp?device_id={}&t={}",
        host, device.id, timestamp
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
    let device = match get_device(params.device_id).await {
        Ok(Some(d)) => d,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                format!("Unable to find device with id: {}", params.device_id),
            )
                .into_response();
        }
        Err(e) => {
            error!("Error: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, format!("{:?}", e)).into_response();
        }
    };

    let template = match get_template().await {
        Ok(t) => t,
        Err(e) => {
            error!("Error: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, format!("{:?}", e)).into_response();
        }
    };

    match renderer::render_screen(&device, &template).await {
        Ok(image) => (StatusCode::OK, [("Content-Type", "image/bmp")], image).into_response(),
        Err(e) => {
            error!("Error: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, format!("{:?}", e)).into_response()
        }
    }
}
