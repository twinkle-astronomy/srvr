use axum::{
    extract::Json,
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
use tracing::{info, debug};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

// Response structs for Device API

#[derive(Serialize)]
struct DisplayResponse {
    // status: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    image_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    filename: Option<String>,
    // refresh_rate: u32,
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
    let height = headers.get("Height").and_then(|h| h.to_str().ok());
    let width = headers.get("Width").and_then(|h| h.to_str().ok());
    let special_function = headers.get("Special-Function").and_then(|h| h.to_str().ok());
    let base64 = headers.get("BASE64").and_then(|h| h.to_str().ok());

    info!("Device telemetry - Access-Token: {}, Battery: {:?}, Charge: {:?}, FW: {:?}, RSSI: {:?}, Size: {:?}x{:?}, Special: {:?}, Base64: {:?}",
        access_token, battery_voltage, percent_charged, fw_version, rssi, width, height, special_function, base64);

    // Get the host from the request headers to build the correct image URL
    let host = headers.get("host")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("localhost:8080");
    let image_url = format!("http://{}/render/screen.bmp", host);

    // TODO: Implement actual device lookup and image generation
    let response = DisplayResponse {
        // status: 200,
        image_url: Some(image_url),
        filename: Some("screen.bmp".to_string()),
        // refresh_rate: 300,
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

    // Get the host from the request headers to build the correct image URL
    let host = headers.get("host")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("localhost:8080");
    let image_url = format!("http://{}/render/screen.bmp", host);

    // TODO: Implement actual current screen lookup
    let response = DisplayCurrentResponse {
        status: 200,
        refresh_rate: 300,
        image_url: Some(image_url),
        filename: Some("screen.bmp".to_string()),
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
    debug!("Received logs: {:?}", payload.logs);

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

    // Get the host from the request headers to build the correct image URL
    let host = headers.get("host")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("localhost:8080");
    let image_url = format!("http://{}/render/screen.bmp", host);

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

// GET /render/screen.bmp - Render screen image using resvg
async fn render_screen_handler() -> impl IntoResponse {
    info!("=== GET /render/screen.bmp - Rendering screen image ===");

    // Standard TRMNL e-ink display resolution
    let width = 800;
    let height = 480;

    // Create SVG with text and graphics
    let svg_data = format!(
        r#"<svg width="{}" height="{}" xmlns="http://www.w3.org/2000/svg">
            <!-- White background -->
            <rect width="100%" height="100%" fill="white"/>

            <!-- Black header bar -->
            <rect x="0" y="0" width="{}" height="80" fill="black"/>

            <!-- Header text (white on black) -->
            <text x="400" y="50" font-family="Liberation Sans, DejaVu Sans, Arial, sans-serif" font-size="36" font-weight="bold" text-anchor="middle" fill="white">
                TRMNL Display
            </text>

            <!-- Main content text -->
            <text x="400" y="200" font-family="Liberation Sans, DejaVu Sans, Arial, sans-serif" font-size="48" font-weight="bold" text-anchor="middle" fill="black">
                Hello World!
            </text>

            <!-- Subtitle -->
            <text x="400" y="260" font-family="Liberation Sans, DejaVu Sans, Arial, sans-serif" font-size="28" text-anchor="middle" fill="black">
                800x480 e-ink screen
            </text>

            <!-- Info text -->
            <text x="400" y="320" font-family="Liberation Sans, DejaVu Sans, Arial, sans-serif" font-size="20" text-anchor="middle" fill="black">
                Powered by Rust + resvg
            </text>

            <!-- Black footer bar -->
            <rect x="0" y="{}" width="{}" height="80" fill="black"/>

            <!-- Footer text (white on black) -->
            <text x="400" y="{}" font-family="Liberation Sans, DejaVu Sans, Arial, sans-serif" font-size="24" text-anchor="middle" fill="white">
                1-bit monochrome BMP
            </text>
        </svg>"#,
        width, height, width, height - 80, width, height - 30
    );

    // Parse SVG
    let mut opt = usvg::Options::default();
    opt.fontdb_mut().load_system_fonts();

    let tree = match usvg::Tree::from_str(&svg_data, &opt) {
        Ok(tree) => tree,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to parse SVG: {}", e)
            ).into_response();
        }
    };

    // Create pixmap for rendering
    let pixmap_size = tree.size().to_int_size();
    let mut pixmap = match tiny_skia::Pixmap::new(pixmap_size.width(), pixmap_size.height()) {
        Some(pixmap) => pixmap,
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to create pixmap"
            ).into_response();
        }
    };

    // Render SVG to pixmap
    resvg::render(&tree, tiny_skia::Transform::default(), &mut pixmap.as_mut());

    // Convert RGBA pixmap to 1-bit format
    // Calculate the size needed for 1-bit data (1 bit per pixel, packed into bytes)
    let width = pixmap_size.width() as usize;
    let height = pixmap_size.height() as usize;
    let row_bytes = (width + 7) / 8; // Round up to nearest byte
    let mut bit_data = vec![0u8; row_bytes * height];

    // Pack pixels into 1-bit format
    for y in 0..height {
        for x in 0..width {
            let pixel = pixmap.pixel(x as u32, y as u32).unwrap();
            // Convert to grayscale using standard luminance formula
            let gray = (0.299 * pixel.red() as f32
                      + 0.587 * pixel.green() as f32
                      + 0.114 * pixel.blue() as f32) as u8;

            // Apply threshold: >= 127 is white (1), < 127 is black (0)
            if gray >= 127 {
                let byte_index = y * row_bytes + x / 8;
                let bit_index = 7 - (x % 8); // MSB first
                bit_data[byte_index] |= 1 << bit_index;
            }
        }
    }

    // Manually create 1-bit BMP file
    // BMP requires rows to be padded to 4-byte boundaries
    let row_size = ((width + 31) / 32) * 4; // Round up to nearest 4 bytes
    let pixel_data_size = row_size * height;
    let color_table_size = 8; // 2 colors * 4 bytes each
    let header_size = 14 + 40; // BMP file header + DIB header
    let file_size = header_size + color_table_size + pixel_data_size;

    let mut bmp_data = Vec::with_capacity(file_size);

    // BMP File Header (14 bytes)
    bmp_data.extend_from_slice(b"BM"); // Signature
    bmp_data.extend_from_slice(&(file_size as u32).to_le_bytes()); // File size
    bmp_data.extend_from_slice(&[0, 0, 0, 0]); // Reserved
    bmp_data.extend_from_slice(&((header_size + color_table_size) as u32).to_le_bytes()); // Pixel data offset

    // DIB Header (BITMAPINFOHEADER, 40 bytes)
    bmp_data.extend_from_slice(&40u32.to_le_bytes()); // Header size
    bmp_data.extend_from_slice(&(width as i32).to_le_bytes()); // Width
    bmp_data.extend_from_slice(&(height as i32).to_le_bytes()); // Height
    bmp_data.extend_from_slice(&1u16.to_le_bytes()); // Planes
    bmp_data.extend_from_slice(&1u16.to_le_bytes()); // Bits per pixel
    bmp_data.extend_from_slice(&0u32.to_le_bytes()); // Compression (none)
    bmp_data.extend_from_slice(&(pixel_data_size as u32).to_le_bytes()); // Image size
    bmp_data.extend_from_slice(&0i32.to_le_bytes()); // X pixels per meter
    bmp_data.extend_from_slice(&0i32.to_le_bytes()); // Y pixels per meter
    bmp_data.extend_from_slice(&2u32.to_le_bytes()); // Colors used (2)
    bmp_data.extend_from_slice(&2u32.to_le_bytes()); // Important colors (2)

    // Color Table (8 bytes: 2 colors * 4 bytes BGRA)
    bmp_data.extend_from_slice(&[0, 0, 0, 0]); // Black (index 0)
    bmp_data.extend_from_slice(&[255, 255, 255, 0]); // White (index 1)

    // Pixel Data (bottom-up, padded rows)
    // BMP stores rows bottom-up, so we need to reverse
    for y in (0..height).rev() {
        let src_offset = y * row_bytes;
        let src_end = (src_offset + row_bytes).min(bit_data.len());
        bmp_data.extend_from_slice(&bit_data[src_offset..src_end]);

        // Add padding to reach 4-byte boundary
        let padding = row_size - row_bytes;
        for _ in 0..padding {
            bmp_data.push(0);
        }
    }

    info!("Generated 1-bit BMP: {}x{}, {} bytes", width, height, bmp_data.len());

    (
        StatusCode::OK,
        [("Content-Type", "image/bmp")],
        bmp_data,
    )
        .into_response()
}
