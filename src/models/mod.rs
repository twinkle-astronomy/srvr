#[cfg(feature = "server")]
use sqlx::FromRow;

use std::collections::HashMap;

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

#[cfg(feature = "server")]
pub mod server;

#[cfg_attr(feature = "server", derive(FromRow))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PrometheusQuery {
    pub id: i64,
    pub name: String,
    pub template_id: i64,
    pub addr: String,
    pub query: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[cfg_attr(feature = "server", derive(FromRow))]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Template {
    pub id: i64,
    pub content: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[cfg_attr(feature = "server", derive(FromRow))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Device {
    pub id: i64,
    pub access_token: String,
    pub mac_address: String,
    pub model: String,
    pub friendly_id: String,
    pub fw_version: Option<String>,
    pub width: i64,
    pub height: i64,
    pub battery_voltage: Option<f32>,
    pub rssi: Option<String>,
    pub last_seen_at: String,
    pub created_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PrometheusQueryResult {
    pub query_name: String,
    pub results: Vec<PrometheusMetricResult>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PrometheusMetricResult {
    pub labels: HashMap<String, String>,
    pub value: f64,
}

#[cfg_attr(feature = "server", derive(FromRow))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DeviceLog {
    pub id: i64,
    pub device_id: i64,
    pub device_log_id: Option<i64>,
    pub battery_voltage: Option<f64>,
    pub created_at: Option<i64>,
    pub firmware_version: Option<String>,
    pub free_heap_size: Option<i64>,
    pub max_alloc_size: Option<i64>,
    pub message: Option<String>,
    pub refresh_rate: Option<i64>,
    pub sleep_duration: Option<i64>,
    pub source_line: Option<i64>,
    pub source_path: Option<String>,
    pub special_function: Option<String>,
    pub wake_reason: Option<String>,
    pub wifi_signal: Option<i64>,
    pub wifi_status: Option<String>,
    pub logged_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeviceLogEntry {
    pub id: Option<i64>,
    pub battery_voltage: Option<f64>,
    pub created_at: Option<i64>,
    pub firmware_version: Option<String>,
    pub free_heap_size: Option<i64>,
    pub max_alloc_size: Option<i64>,
    pub message: Option<String>,
    pub refresh_rate: Option<i64>,
    pub sleep_duration: Option<i64>,
    pub source_line: Option<i64>,
    pub source_path: Option<String>,
    pub special_function: Option<String>,
    pub wake_reason: Option<String>,
    pub wifi_signal: Option<i64>,
    pub wifi_status: Option<String>,
}

impl Device {
    pub fn percent_charged(&self) -> Option<f32> {
        self.battery_voltage.map(|battery_voltage| {
            let pct_charged = (battery_voltage - 3.) / 0.012;

            match pct_charged {
                88.0..=f32::INFINITY => 100.0,
                85.0..88.0 => 95.0,
                83.0..85.0 => 90.0,
                10.0..83.0 => pct_charged,
                _ => 0.0,
            }
        })
    }
}
