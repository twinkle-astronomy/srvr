use std::sync::OnceLock;

use dioxus::prelude::*;
use liquid::ParserBuilder;
use liquid::{Error, Object};

use crate::device::liquid_filters::{QrcodeFilterParser, QrcodeWifiFilterParser};
use crate::models::{Device, HttpSource, PrometheusQuery, Template};

pub fn http_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(reqwest::Client::new)
}

impl Template {
    pub fn render(&self, globals: Object) -> Result<String, Error> {
        let parser = ParserBuilder::with_stdlib()
            .filter(QrcodeFilterParser)
            .filter(QrcodeWifiFilterParser)
            .build()?;

        let template = parser.parse(&self.content)?;

        Ok(template.render(&globals)?)
    }
}

impl Device {
    pub fn get_render_obj(&self) -> Object {
        liquid::object!({
            "width": self.width,
            "height": self.height,
            "fw_version": self.fw_version,
            "rssi": self.rssi,
            "friendly_id": self.friendly_id,
            "mac_address": self.mac_address,
            "battery_voltage": self.battery_voltage,
            "battery_percent_charged": self.percent_charged(),
        })
    }
}

impl HttpSource {
    pub async fn get_render_obj(&self) -> Result<liquid::model::Value, reqwest::Error> {
        let response = http_client()
            .get(&self.url)
            .header("Accept", "application/json")
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;

        Ok(json_to_liquid(&response))
    }
}

pub fn json_to_liquid(value: &serde_json::Value) -> liquid::model::Value {
    match value {
        serde_json::Value::Null => liquid::model::Value::Nil,
        serde_json::Value::Bool(b) => liquid::model::Value::scalar(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                liquid::model::Value::scalar(i)
            } else if let Some(f) = n.as_f64() {
                liquid::model::Value::scalar(f)
            } else {
                liquid::model::Value::scalar(n.to_string())
            }
        }
        serde_json::Value::String(s) => liquid::model::Value::scalar(s.clone()),
        serde_json::Value::Array(arr) => {
            liquid::model::Value::Array(arr.iter().map(json_to_liquid).collect())
        }
        serde_json::Value::Object(map) => {
            let obj: liquid::Object = map
                .iter()
                .map(|(k, v)| (k.clone().into(), json_to_liquid(v)))
                .collect();
            liquid::model::Value::Object(obj)
        }
    }
}

impl PrometheusQuery {
    pub async fn get_render_obj(&self) -> Result<Vec<Object>, prometheus_http_query::error::Error> {
        let client =
            prometheus_http_query::Client::from(http_client().clone(), self.addr.as_str())?;

        Ok(client
            .query(self.query.as_str())
            .get()
            .await?
            .data()
            .as_vector()
            .map(|v| {
                v.iter().map(|x| {
                    liquid::object!( {
                        "labels": x.metric(),
                        "value": x.sample().value(),
                    })
                })
            })
            .map(|x| x.collect())
            .unwrap_or_else(|| vec![]))
    }
}
