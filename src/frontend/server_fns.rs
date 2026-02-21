use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ServerInfo {
    pub time: String,
    pub date: String,
    pub prometheus_url: String,
    pub port: u16,
}

#[server]
pub async fn get_temperature() -> Result<Option<f64>, ServerFnError> {
    let prometheus_url =
        std::env::var("PROMETHEUS_URL").unwrap_or_else(|_| "http://prometheus:9090".to_string());

    let client = prometheus_http_query::Client::try_from(prometheus_url.as_str())
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    let query =
        r#"sht30_reading{location="Front Porch", sensor="temperature"} * 9/5 + 32"#;

    match client.query(query).get().await {
        Ok(response) => Ok(response
            .data()
            .as_vector()
            .and_then(|v| v.first().map(|sample| sample.sample().value()))),
        Err(e) => {
            tracing::info!("Failed to query Prometheus: {}", e);
            Ok(None)
        }
    }
}

#[server]
pub async fn get_screen_preview(width: u32, height: u32) -> Result<Option<String>, ServerFnError> {
    use base64::Engine;

    let temperature = match get_temperature().await {
        Ok(t) => t,
        Err(_) => None,
    };

    match crate::device::renderer::render_screen(width, height, temperature, "web".to_string()).await {
        Ok(bmp_bytes) => {
            let encoded = base64::engine::general_purpose::STANDARD.encode(&bmp_bytes);
            Ok(Some(encoded))
        }
        Err(e) => {
            tracing::info!("Failed to render screen: {}", e);
            Ok(None)
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DeviceInfo {
    pub id: i64,
    pub mac_address: String,
    pub model: String,
    pub friendly_id: String,
    pub fw_version: Option<String>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub battery_voltage: Option<String>,
    pub rssi: Option<String>,
    pub last_seen_at: String,
    pub created_at: String,
}

#[server]
pub async fn get_devices() -> Result<Vec<DeviceInfo>, ServerFnError> {
    let db = crate::db::get();

    type Row = (i64, String, String, String, Option<String>, Option<i64>, Option<i64>, Option<String>, Option<String>, String, String);

    let rows: Vec<Row> = sqlx::query_as(
        "SELECT id, mac_address, model, friendly_id, fw_version, width, height, battery_voltage, rssi, last_seen_at, created_at \
         FROM devices ORDER BY last_seen_at DESC"
    )
        .fetch_all(db)
        .await
        .map_err(|e: sqlx::Error| ServerFnError::new(e.to_string()))?;

    Ok(rows.into_iter().map(|(id, mac_address, model, friendly_id, fw_version, width, height, battery_voltage, rssi, last_seen_at, created_at)| {
        DeviceInfo { id, mac_address, model, friendly_id, fw_version, width, height, battery_voltage, rssi, last_seen_at, created_at }
    }).collect())
}

#[server]
pub async fn get_server_info() -> Result<ServerInfo, ServerFnError> {
    let now = chrono::Utc::now();
    let prometheus_url =
        std::env::var("PROMETHEUS_URL").unwrap_or_else(|_| "http://prometheus:9090".to_string());

    Ok(ServerInfo {
        time: now.format("%H:%M:%S UTC").to_string(),
        date: now.format("%Y-%m-%d").to_string(),
        prometheus_url,
        port: 8080,
    })
}
