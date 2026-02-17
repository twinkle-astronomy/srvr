use axum::{
    extract::{Json, Query},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod renderer;

// Response structs for Device API

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

#[tokio::main]
async fn main() {
    // Initialize tracing subscriber to output to stdout
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_target(false)
                .with_file(true)
                .with_line_number(true)
                .with_writer(std::io::stdout)
        )
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,tower_http=debug".into())
        )
        .init();

    let app = Router::new()
        .route("/", get(root))
        .route("/api/display", get(display_handler))
        .route("/api/display/current", get(display_current_handler))
        .route("/api/log", post(log_handler))
        .route("/api/setup", get(setup_handler))
        .route("/render/screen.bmp", get(render_screen_handler))
        .layer(TraceLayer::new_for_http())
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        );

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080")
        .await
        .unwrap();

    info!("TRMNL server running on http://0.0.0.0:8080");
    info!("Device API endpoints:");
    info!("  GET  /api/display         - Fetch the next screen");
    info!("  GET  /api/display/current - Fetch the current screen");
    info!("  POST /api/log             - Log device messages");
    info!("  GET  /api/setup           - Set up device");
    info!("  GET  /render/screen.bmp   - Render screen image");

    axum::serve(listener, app).await.unwrap();
}

async fn root() -> &'static str {
    "TRMNL eink device server"
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
    let percent_charged = headers.get("Percent-Charged").and_then(|h| h.to_str().ok());
    let fw_version = headers.get("FW-Version").and_then(|h| h.to_str().ok());
    let rssi = headers.get("RSSI").and_then(|h| h.to_str().ok());
    let device_height = headers.get("Height").and_then(|h| h.to_str().ok());
    let device_width = headers.get("Width").and_then(|h| h.to_str().ok());
    let special_function = headers.get("Special-Function").and_then(|h| h.to_str().ok());
    let base64 = headers.get("BASE64").and_then(|h| h.to_str().ok());

    info!("Device telemetry - Access-Token: {}, Battery: {:?}, Charge: {:?}, FW: {:?}, RSSI: {:?}, Size: {:?}x{:?}, Special: {:?}, Base64: {:?}",
        access_token, battery_voltage, percent_charged, fw_version, rssi, device_width, device_height, special_function, base64);

    // Get the host from the request headers to build the correct image URL
    let host = headers.get("host")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("localhost:8080");

    // Add timestamp for cache busting and device dimensions
    let timestamp = Utc::now().timestamp();
    let width_param = device_width.unwrap_or("800");
    let height_param = device_height.unwrap_or("480");
    let image_url = format!("http://{}/render/screen.bmp?width={}&height={}&t={}",
        host, width_param, height_param, timestamp);

    // TODO: Implement actual device lookup and image generation
    let response = DisplayResponse {
        // status: 200,
        image_url: Some(image_url),
        filename: Some(format!("screen_{}.bmp", timestamp)),
        refresh_rate: 60,
        // reset_firmware: false,
        update_firmware: false,
        // firmware_url: None,
        // special_function: "identify".to_string(),
        // action: Some("identify".to_string()),
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

    // Get device dimensions if available
    let device_width = headers.get("Width").and_then(|h| h.to_str().ok()).unwrap_or("800");
    let device_height = headers.get("Height").and_then(|h| h.to_str().ok()).unwrap_or("480");

    // Get the host from the request headers to build the correct image URL
    let host = headers.get("host")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("localhost:8080");

    // Add timestamp for cache busting and device dimensions
    let timestamp = Utc::now().timestamp();
    let image_url = format!("http://{}/render/screen.bmp?width={}&height={}&t={}",
        host, device_width, device_height, timestamp);

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

    // Get device dimensions if available
    let device_width = headers.get("Width").and_then(|h| h.to_str().ok()).unwrap_or("800");
    let device_height = headers.get("Height").and_then(|h| h.to_str().ok()).unwrap_or("480");

    // Get the host from the request headers to build the correct image URL
    let host = headers.get("host")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("localhost:8080");

    // Add timestamp for cache busting and device dimensions
    let timestamp = Utc::now().timestamp();
    let image_url = format!("http://{}/render/screen.bmp?width={}&height={}&t={}",
        host, device_width, device_height, timestamp);

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
}

fn default_width() -> u32 { 800 }
fn default_height() -> u32 { 480 }

// GET /render/screen.bmp - Render screen image
async fn render_screen_handler(Query(params): Query<RenderQuery>) -> impl IntoResponse {
    info!("=== GET /render/screen.bmp - {}x{} ===", params.width, params.height);
    renderer::render_screen(params.width, params.height).await
}
