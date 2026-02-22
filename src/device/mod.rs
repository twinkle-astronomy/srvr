use axum::http::{HeaderMap, header::ToStrError};

use crate::{db::get_or_create_device, models::Device};

use thiserror::Error;

pub mod api;
pub(crate) mod renderer;

#[derive(Error, Debug)]
pub enum Error {
    #[error("{0}")]
    ToStrError(#[from] ToStrError),
    #[error("Missing Access-Token")]
    MissingAccessToken,
    #[error("{0}")]
    SqlxError(#[from] sqlx::error::Error),
}

async fn device_from_headers(headers: &HeaderMap) -> Result<Device, Error> {
    let access_token = match headers.get("Access-Token") {
        Some(token) => token.to_str()?,
        None => return Err(Error::MissingAccessToken),
    };

    // Extract optional headers for device telemetry
    let mac_address = headers.get("ID").and_then(|h| h.to_str().ok());
    let friendly_id = headers
        .get("Frendly-Id")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("Unknown");
    let model = headers.get("model").and_then(|h| h.to_str().ok());
    let battery_voltage = headers
        .get("Battery-Voltage")
        .and_then(|h| h.to_str().ok())
        .and_then(|x| x.parse().ok());
    let fw_version = headers.get("FW-Version").and_then(|h| h.to_str().ok());
    let rssi = headers.get("RSSI").and_then(|h| h.to_str().ok());
    let device_height = headers
        .get("Height")
        .and_then(|h| h.to_str().ok())
        .and_then(|x| x.parse().ok());
    let device_width = headers
        .get("Width")
        .and_then(|h| h.to_str().ok())
        .and_then(|x| x.parse().ok());

    Ok(get_or_create_device(
        access_token,
        mac_address,
        model,
        friendly_id,
        fw_version,
        device_width,
        device_height,
        battery_voltage,
        rssi,
    )
    .await?)
}
