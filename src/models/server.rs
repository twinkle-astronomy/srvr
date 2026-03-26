use std::sync::OnceLock;

use dioxus::prelude::*;
use liquid::ParserBuilder;
use liquid::{Error, Object};

use crate::device::liquid_filters::{QrcodeFilterParser, QrcodeWifiFilterParser};
use crate::models::{Device, PrometheusQuery, Template};

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

impl PrometheusQuery {
    pub async fn get_render_obj(&self) -> Result<Vec<Object>, prometheus_http_query::error::Error> {
        let client = prometheus_http_query::Client::from(http_client().clone(), self.addr.as_str())?;

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
