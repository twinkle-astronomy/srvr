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

    match crate::renderer::render_screen(width, height, temperature, "web".to_string()).await {
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
