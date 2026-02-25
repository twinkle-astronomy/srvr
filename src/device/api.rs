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
    db::{get_device, get_template},
    device::{device_from_headers, renderer},
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

    let device = match device_from_headers(&headers).await {
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
        refresh_rate: 5 * 60,
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

    info!("Received logs: {:?}", payload.logs);

    // Upsert last_seen in database (fire-and-forget)
    let db = crate::db::get().clone();
    let token = access_token.to_string();

    let _ = sqlx::query(
        "INSERT INTO devices (access_token, mac_address, model, friendly_id) \
            VALUES (?, 'unknown', 'unknown', 'unknown') \
            ON CONFLICT(access_token) DO UPDATE SET \
            last_seen_at = datetime('now'), \
            updated_at = datetime('now')",
    )
    .bind(&token)
    .execute(&db)
    .await;

    StatusCode::NO_CONTENT.into_response()
}

// GET /api/setup - Set up device
async fn setup_handler(headers: HeaderMap) -> impl IntoResponse {
    // Extract required headers
    let device_id = match headers.get("ID") {
        Some(id) => id.to_str().unwrap_or(""),
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "Missing ID header (Device MAC Address)"
                })),
            )
                .into_response();
        }
    };

    let device_model = match headers.get("Model") {
        Some(model) => model.to_str().unwrap_or(""),
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "Missing Model header"
                })),
            )
                .into_response();
        }
    };

    info!("Setup request - ID: {}, Model: {}", device_id, device_model);

    // Get device dimensions and firmware version if available
    let device_width: Option<i64> = headers
        .get("Width")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.parse().ok());
    let device_height: Option<i64> = headers
        .get("Height")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.parse().ok());
    let fw_version = headers.get("FW-Version").and_then(|h| h.to_str().ok());

    let db = crate::db::get();

    // Check if device already exists by MAC address
    let existing = sqlx::query_as::<_, (String, String)>(
        "SELECT access_token, friendly_id FROM devices WHERE mac_address = ?",
    )
    .bind(device_id)
    .fetch_optional(db)
    .await;

    let (api_key, friendly_id) = match existing {
        Ok(Some((token, fid))) => {
            // Device exists — update its info and return existing token
            let _ = sqlx::query(
                "UPDATE devices SET model = ?, width = COALESCE(?, width), height = COALESCE(?, height), \
                 fw_version = COALESCE(?, fw_version), \
                 last_seen_at = datetime('now'), updated_at = datetime('now') \
                 WHERE mac_address = ?"
            )
                .bind(device_model)
                .bind(device_width)
                .bind(device_height)
                .bind(fw_version)
                .bind(device_id)
                .execute(db)
                .await;

            info!("Existing device re-registered: {}", device_id);
            (token, fid)
        }
        Ok(None) => {
            // New device — generate token and insert
            let token = generate_access_token();
            let fid = format!("device-{}", &device_id[..device_id.len().min(6)]);

            let result = sqlx::query(
                "INSERT INTO devices (mac_address, model, access_token, friendly_id, \
                 width, height, fw_version) VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(device_id)
            .bind(device_model)
            .bind(&token)
            .bind(&fid)
            .bind(device_width)
            .bind(device_height)
            .bind(fw_version)
            .execute(db)
            .await;

            if let Err(e) = result {
                tracing::error!("Failed to insert device: {}", e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({
                        "error": "Failed to register device"
                    })),
                )
                    .into_response();
            }

            info!("New device registered: {} -> {}", device_id, fid);
            (token, fid)
        }
        Err(e) => {
            tracing::error!("Database error during setup: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "Database error"
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

    let timestamp = Utc::now().timestamp();
    let width_param = device_width
        .map(|w| w.to_string())
        .unwrap_or_else(|| "800".to_string());
    let height_param = device_height
        .map(|h| h.to_string())
        .unwrap_or_else(|| "480".to_string());
    let fw_param = fw_version.unwrap_or("unknown");
    let image_url = format!(
        "http://{}/render/screen.bmp?width={}&height={}&fw={}&t={}",
        host, width_param, height_param, fw_param, timestamp
    );

    let response = SetupResponse {
        status: 200,
        api_key: Some(api_key),
        friendly_id: Some(friendly_id),
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
