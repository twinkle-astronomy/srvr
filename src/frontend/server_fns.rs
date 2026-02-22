use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::models::{Device, Template};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ServerInfo {
    pub time: String,
    pub date: String,
    pub prometheus_url: String,
    pub port: u16,
}

#[server]
pub async fn get_screen_preview(device_id: i64) -> Result<Option<String>, ServerFnError> {
    use crate::db::{get_device, get_template};
    use base64::Engine;

    let device = get_device(device_id)
        .await
        .map_err(|e| ServerFnError::new(format!("Unablle to query db: {:?}", e)))?
        .ok_or_else(|| {
            ServerFnError::new(format!("Unable to find device with id: {:?}", device_id))
        })?;

    let template = get_template()
        .await
        .map_err(|e| ServerFnError::new(format!("Unablle to query db: {:?}", e)))?;

    match crate::device::renderer::render_screen(&device, &template).await {
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
pub async fn get_template_preview(
    device_id: i64,
    template: Template,
) -> Result<Option<String>, ServerFnError> {
    use crate::db::get_device;
    use base64::Engine;

    let device = get_device(device_id)
        .await
        .map_err(|e| ServerFnError::new(format!("Unablle to query db: {:?}", e)))?
        .ok_or_else(|| {
            ServerFnError::new(format!("Unable to find device with id: {:?}", device_id))
        })?;

    match crate::device::renderer::render_screen(&device, &template).await {
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
pub async fn get_template() -> Result<Template, ServerFnError> {
    let template = crate::db::get_template()
        .await
        .map_err(|e| ServerFnError::new(format!("Unable to query db: {:?}", e)))?;

    Ok(template)
}

#[server]
pub async fn save_template(id: i64, content: String) -> Result<(), ServerFnError> {
    crate::db::update_template(id, &content)
        .await
        .map_err(|e| ServerFnError::new(format!("Unable to save template: {:?}", e)))?;

    Ok(())
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
