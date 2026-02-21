use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::models::Device;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ServerInfo {
    pub time: String,
    pub date: String,
    pub prometheus_url: String,
    pub port: u16,
}

#[server]
pub async fn get_temperature() -> Result<Option<f64>, ServerFnError> {
    use crate::data_sources::get_prometheus;

    Ok(get_prometheus().await)
}

#[server]
pub async fn get_screen_preview(device_id: i64) -> Result<Option<String>, ServerFnError> {
    use base64::Engine;

    use crate::db::get_device;

    let device = get_device(device_id)
        .await
        .map_err(|e| ServerFnError::new(format!("Unablle to query db: {:?}", e)))?
        .ok_or_else(|| ServerFnError::new(format!("Unable to find device with id: {:?}", device_id)))?;


    match crate::device::renderer::render_screen(&device).await {
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

#[server]
pub async fn get_devices() -> Result<Vec<Device>, ServerFnError> {
    crate::db::get_devices()
        .await
        .map_err(|e: sqlx::Error| ServerFnError::new(e.to_string()))
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
