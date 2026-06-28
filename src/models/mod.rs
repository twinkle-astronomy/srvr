use dioxus::prelude::*;

#[cfg(feature = "server")]
use sqlx::FromRow;

use std::collections::HashMap;

use chrono::{NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};

#[cfg(feature = "server")]
pub mod server;

#[cfg_attr(feature = "server", derive(FromRow))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PrometheusQuery {
    pub id: Option<i64>,
    pub name: String,
    pub template_id: i64,
    pub addr: String,
    pub query: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

impl PrometheusQuery {
    pub fn new(template_id: i64) -> Self {
        Self {
            id: None,
            template_id,
            name: "".to_string(),
            addr: "".to_string(),
            query: "".to_string(),
            created_at: Utc::now().naive_utc(),
            updated_at: Utc::now().naive_utc(),
        }
    }
}

#[cfg_attr(feature = "server", derive(FromRow))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RangeQuery {
    pub id: Option<i64>,
    pub name: String,
    pub template_id: i64,
    pub addr: String,
    pub query: String,
    /// Window length back from now, Prometheus-style (e.g. "1h", "30m", "24h").
    pub duration: String,
    /// Resolution between points, Prometheus-style (e.g. "60s", "5m").
    pub step: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

impl RangeQuery {
    pub fn new(template_id: i64) -> Self {
        Self {
            id: None,
            template_id,
            name: "".to_string(),
            addr: "".to_string(),
            query: "".to_string(),
            duration: "1h".to_string(),
            step: "60s".to_string(),
            created_at: Utc::now().naive_utc(),
            updated_at: Utc::now().naive_utc(),
        }
    }
}

/// One time series returned by a range query, plus scaling helpers so template
/// authors can map values into a viewport without iterating in Liquid.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RangeSeries {
    pub labels: HashMap<String, String>,
    pub points: Vec<RangePoint>,
    pub min: f64,
    pub max: f64,
    pub first: f64,
    pub last: f64,
    pub count: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RangePoint {
    /// Unix timestamp in seconds.
    pub t: f64,
    pub value: f64,
}

// Built only on the server (from a Prometheus response); the web target just
// deserializes the resulting `RangeSeries`.
#[cfg(feature = "server")]
impl RangeSeries {
    /// Build a series from its labels and ordered points, computing the scaling
    /// helpers (min/max/first/last/count). An empty series reports zeros.
    pub fn from_points(labels: HashMap<String, String>, points: Vec<RangePoint>) -> Self {
        let count = points.len();
        let first = points.first().map(|p| p.value).unwrap_or(0.0);
        let last = points.last().map(|p| p.value).unwrap_or(0.0);
        let (min, max) = points
            .iter()
            .fold(None, |acc, p| {
                let (lo, hi) = acc.unwrap_or((p.value, p.value));
                Some((lo.min(p.value), hi.max(p.value)))
            })
            .unwrap_or((0.0, 0.0));

        Self {
            labels,
            points,
            min,
            max,
            first,
            last,
            count,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RangeQueryResult {
    pub query_name: String,
    pub series: Vec<RangeSeries>,
    pub error: Option<String>,
}

#[cfg_attr(feature = "server", derive(FromRow))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Store)]
pub struct Template {
    pub id: i64,
    pub name: String,
    pub content: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[cfg_attr(feature = "server", derive(FromRow))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Store)]
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
    pub template_id: i64,
    pub maximum_compatibility: bool,
    pub last_seen_at: String,
    pub created_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Store)]
pub struct RenderContext {
    pub device: Device,
    pub template: Template,
    pub prometheus_queries: Vec<PrometheusQuery>,
    pub range_queries: Vec<RangeQuery>,
    pub http_sources: Vec<HttpSource>,
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
pub struct HttpSource {
    pub id: Option<i64>,
    pub name: String,
    pub template_id: i64,
    pub url: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

impl HttpSource {
    pub fn new(template_id: i64) -> Self {
        Self {
            id: None,
            template_id,
            name: "".to_string(),
            url: "".to_string(),
            created_at: Utc::now().naive_utc(),
            updated_at: Utc::now().naive_utc(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct HttpSourceResult {
    pub source_name: String,
    pub data: Option<serde_json::Value>,
    pub error: Option<String>,
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

#[cfg(feature = "server")]
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

#[cfg(feature = "server")]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, FromRow)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub password_hash: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AuthenticatedUser {
    pub id: i64,
    pub username: String,
}

impl Device {
    pub fn virtual_device() -> Self {
        Device {
            id: 0,
            access_token: String::new(),
            mac_address: "00:00:00:00:00:00".to_string(),
            model: "Virtual".to_string(),
            friendly_id: "virtual-device".to_string(),
            fw_version: Some("1.0.0".to_string()),
            width: 800,
            height: 480,
            battery_voltage: Some(3.9),
            rssi: Some("-65".to_string()),
            template_id: 0,
            maximum_compatibility: false,
            last_seen_at: String::new(),
            created_at: String::new(),
        }
    }

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

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;

    #[test]
    fn test_range_series_from_points_computes_summary() {
        let points = vec![
            RangePoint { t: 1.0, value: 3.0 },
            RangePoint { t: 2.0, value: 1.0 },
            RangePoint { t: 3.0, value: 5.0 },
        ];
        let series = RangeSeries::from_points(HashMap::new(), points);
        assert_eq!(series.count, 3);
        assert_eq!(series.min, 1.0);
        assert_eq!(series.max, 5.0);
        assert_eq!(
            series.first, 3.0,
            "first should track point order, not sort"
        );
        assert_eq!(series.last, 5.0);
        assert_eq!(series.points.len(), 3, "points should be preserved");
    }

    #[test]
    fn test_range_series_from_points_empty_reports_zeros() {
        let series = RangeSeries::from_points(HashMap::new(), vec![]);
        assert_eq!(series.count, 0);
        assert_eq!(series.min, 0.0);
        assert_eq!(series.max, 0.0);
        assert_eq!(series.first, 0.0);
        assert_eq!(series.last, 0.0);
    }
}
