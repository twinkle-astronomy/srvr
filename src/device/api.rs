use std::{f32, num::ParseFloatError};

use axum::{
    Router, extract::{Json, Query}, http::{HeaderMap, StatusCode}, response::IntoResponse, routing::{get, post}
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use tracing::info;

use crate::renderer;

pub fn router<T: Clone + Send + Sync + 'static>() -> Router<T> {
    Router::new()
        .route("/api/display", get(display_handler))
        .route("/api/display/current", get(display_current_handler))
        .route("/api/log", post(log_handler))
        .route("/api/setup", get(setup_handler))
        .route("/render/screen.bmp", get(render_screen_handler))
}



#[derive(Serialize)]
struct DisplayResponse {
    // status: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    image_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    filename: Option<String>,
    refresh_rate: u32,
    // reset_firmware: bool,
    update_firmware: bool,
    // #[serde(skip_serializing_if = "Option::is_none")]
    // firmware_url: Option<String>,
    // special_function: String,
    // #[serde(skip_serializing_if = "Option::is_none")]
    // action: Option<String>,
    maximum_compatibility: bool,
}

#[derive(Serialize)]
struct DisplayCurrentResponse {
    status: u16,
    refresh_rate: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    image_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    filename: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rendered_at: Option<DateTime<Utc>>,
}

#[derive(Deserialize)]
struct LogRequest {
    logs: Vec<serde_json::Value>,
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

fn percent_charged(battery_voltage: &str) -> Result<f32,  ParseFloatError> {
    let battery_voltage: f32 = battery_voltage.parse()?;
    let pct_charged = (battery_voltage - 3.) / 0.012;

    Ok(match pct_charged {
        88.0..=f32::INFINITY => 100.0,
        85.0..88.0 => 95.0,
        83.0..85.0 => 90.0,
        10.0..83.0 => pct_charged,
        _ => 0.0,
    })
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

    // Extract required Access-Token header
    let access_token = match headers.get("Access-Token") {
        Some(token) => token.to_str().unwrap_or(""),
        None => {
            info!("Response: 401 Unauthorized - Missing Access-Token");
            return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({
                "error": "Missing Access-Token header"
            }))).into_response();
        }
    };

    // Extract optional headers for device telemetry
    let battery_voltage = headers.get("Battery-Voltage").and_then(|h| h.to_str().ok());
    // let percent_charged = headers.get("Percent-Charged").and_then(|h| h.to_str().ok());
    let fw_version = headers.get("FW-Version").and_then(|h| h.to_str().ok());
    let rssi = headers.get("RSSI").and_then(|h| h.to_str().ok());
    let device_height = headers.get("Height").and_then(|h| h.to_str().ok());
    let device_width = headers.get("Width").and_then(|h| h.to_str().ok());
    let special_function = headers.get("Special-Function").and_then(|h| h.to_str().ok());
    let base64 = headers.get("BASE64").and_then(|h| h.to_str().ok());

    info!("Device telemetry - Access-Token: {}, Battery: {:?}, Charge: {:?}, FW: {:?}, RSSI: {:?}, Size: {:?}x{:?}, Special: {:?}, Base64: {:?}",
        access_token, battery_voltage, battery_voltage.map(|x| percent_charged(x)), fw_version, rssi, device_width, device_height, special_function, base64);

    // Get the host from the request headers to build the correct image URL
    let host = headers.get("host")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("localhost:8080");

    // Add timestamp for cache busting and device dimensions
    let timestamp = Utc::now().timestamp();
    let width_param = device_width.unwrap_or("800");
    let height_param = device_height.unwrap_or("480");
    let fw_param = fw_version.unwrap_or("unknown");
    let image_url = format!("http://{}/render/screen.bmp?width={}&height={}&fw={}&t={}",
        host, width_param, height_param, fw_param, timestamp);

    // TODO: Implement actual device lookup and image generation
    let response = DisplayResponse {
        // status: 200,
        image_url: Some(image_url),
        filename: Some(format!("screen_{}.bmp", timestamp)),
        refresh_rate: 5*60,
        // reset_firmware: false,
        update_firmware: false,
        // firmware_url: None,
        // special_function: "identify".to_string(),
        // action: Some("identify".to_string()),
        maximum_compatibility: false,
    };

    // Log response
    info!("Response: {}", serde_json::to_string_pretty(&response).unwrap_or_else(|_| "Failed to serialize response".to_string()));

    (StatusCode::OK, Json(response)).into_response()
}

// GET /api/display/current - Fetch the current screen
async fn display_current_handler(headers: HeaderMap) -> impl IntoResponse {
    // Extract required Access-Token header
    let _access_token = match headers.get("Access-Token") {
        Some(token) => token.to_str().unwrap_or(""),
        None => return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({
            "error": "Missing Access-Token header"
        }))).into_response(),
    };

    // Get device dimensions and firmware version if available
    let device_width = headers.get("Width").and_then(|h| h.to_str().ok()).unwrap_or("800");
    let device_height = headers.get("Height").and_then(|h| h.to_str().ok()).unwrap_or("480");
    let fw_version = headers.get("FW-Version").and_then(|h| h.to_str().ok()).unwrap_or("unknown");

    // Get the host from the request headers to build the correct image URL
    let host = headers.get("host")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("localhost:8080");

    // Add timestamp for cache busting and device dimensions
    let timestamp = Utc::now().timestamp();
    let image_url = format!("http://{}/render/screen.bmp?width={}&height={}&fw={}&t={}",
        host, device_width, device_height, fw_version, timestamp);

    // TODO: Implement actual current screen lookup
    let response = DisplayCurrentResponse {
        status: 200,
        refresh_rate: 300,
        image_url: Some(image_url),
        filename: Some(format!("screen_{}.bmp", timestamp)),
        rendered_at: Some(Utc::now()),
    };

    (StatusCode::OK, Json(response)).into_response()
}

// POST /api/log - Log with logs[] array
async fn log_handler(headers: HeaderMap, Json(payload): Json<LogRequest>) -> impl IntoResponse {
    // Extract required Access-Token header
    let _access_token = match headers.get("Access-Token") {
        Some(token) => token.to_str().unwrap_or(""),
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };

    // TODO: Implement actual logging
    info!("Received logs: {:?}", payload.logs);

    StatusCode::NO_CONTENT.into_response()
}

// GET /api/setup - Set up device
async fn setup_handler(headers: HeaderMap) -> impl IntoResponse {
    // Extract required headers
    let device_id = match headers.get("ID") {
        Some(id) => id.to_str().unwrap_or(""),
        None => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({
            "error": "Missing ID header (Device MAC Address)"
        }))).into_response(),
    };

    let device_model = match headers.get("Model") {
        Some(model) => model.to_str().unwrap_or(""),
        None => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({
            "error": "Missing Model header"
        }))).into_response(),
    };

    // TODO: Implement actual device setup and registration
    info!("Setup request - ID: {}, Model: {}", device_id, device_model);

    // Get device dimensions and firmware version if available
    let device_width = headers.get("Width").and_then(|h| h.to_str().ok()).unwrap_or("800");
    let device_height = headers.get("Height").and_then(|h| h.to_str().ok()).unwrap_or("480");
    let fw_version = headers.get("FW-Version").and_then(|h| h.to_str().ok()).unwrap_or("unknown");

    // Get the host from the request headers to build the correct image URL
    let host = headers.get("host")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("localhost:8080");

    // Add timestamp for cache busting and device dimensions
    let timestamp = Utc::now().timestamp();
    let image_url = format!("http://{}/render/screen.bmp?width={}&height={}&fw={}&t={}",
        host, device_width, device_height, fw_version, timestamp);

    // Mock response - in production, this would check if device exists
    let response = SetupResponse {
        status: 200,
        api_key: Some("mock-api-key-12345".to_string()),
        friendly_id: Some(format!("device-{}", &device_id[..6])),
        image_url: Some(image_url),
        message: "Device setup successful".to_string(),
    };

    (StatusCode::OK, Json(response)).into_response()
}

#[derive(Deserialize)]
struct RenderQuery {
    #[serde(default = "default_width")]
    width: u32,
    #[serde(default = "default_height")]
    height: u32,
    #[serde(default = "default_fw")]
    fw: String,
}

fn default_width() -> u32 { 800 }
fn default_height() -> u32 { 480 }
fn default_fw() -> String { "unknown".to_string() }

// GET /render/screen.bmp - Render screen image
async fn render_screen_handler(Query(params): Query<RenderQuery>) -> impl IntoResponse {
    let prometheus_url = std::env::var("PROMETHEUS_URL").unwrap_or_else(|_| "http://prometheus:9090".to_string());
    let client = prometheus_http_query::Client::try_from(prometheus_url.as_str()).unwrap();
    let query = r#"sht30_reading{location="Front Porch", sensor="temperature"} * 9/5 + 32"#;

    let scrape_duration_display = match client.query(query).get().await {
        Ok(response) => {
            response.data().as_vector()
                .and_then(|v| v.first().map(|sample| sample.sample().value()))
        }
        Err(e) => {
            info!("Failed to query Prometheus: {}", e);
            None
        }
    };

    match renderer::render_screen(params.width, params.height, scrape_duration_display, params.fw).await {
        Ok(image) => {
             (
                StatusCode::OK,
                [("Content-Type", "image/bmp")],
                image,
            ).into_response()
        },
        Err(e) => {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("{}", e)
            ).into_response()
        }
    }
}
