use std::sync::OnceLock;

use dioxus::prelude::*;
use liquid::ParserBuilder;
use liquid::{Error, Object};

use crate::device::liquid_filters::{QrcodeFilterParser, QrcodeWifiFilterParser};
use crate::models::{
    Device, HttpSource, PrometheusQuery, RangePoint, RangeQuery, RangeSeries, Template,
};
use crate::time::{Clock, RealClock};

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

impl RangeQuery {
    /// Run the range query against Prometheus and return one `RangeSeries` per
    /// matched time series (with scaling helpers computed). Errors — bad
    /// duration/step, an invalid address, or a Prometheus failure — surface as a
    /// `String` so the renderer can skip the query and the editor can show it.
    pub async fn fetch_series(&self) -> Result<Vec<RangeSeries>, String> {
        let (start, end, step) =
            crate::time::range_window(RealClock.now_secs(), &self.duration, &self.step)?;

        let client = prometheus_http_query::Client::from(http_client().clone(), self.addr.as_str())
            .map_err(|e| e.to_string())?;

        let response = client
            .query_range(self.query.as_str(), start, end, step)
            .get()
            .await
            .map_err(|e| e.to_string())?;

        let series = response
            .data()
            .as_matrix()
            .map(|matrix| {
                matrix
                    .iter()
                    .map(|rv| {
                        let points = rv
                            .samples()
                            .iter()
                            .map(|s| RangePoint {
                                t: s.timestamp(),
                                value: s.value(),
                            })
                            .collect();
                        RangeSeries::from_points(rv.metric().clone(), points)
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(series)
    }

    pub async fn get_render_obj(&self) -> Result<Vec<Object>, String> {
        Ok(self
            .fetch_series()
            .await?
            .iter()
            .map(range_series_to_object)
            .collect())
    }
}

fn range_series_to_object(s: &RangeSeries) -> Object {
    let points: Vec<liquid::model::Value> = s
        .points
        .iter()
        .map(|p| liquid::model::Value::Object(liquid::object!({ "t": p.t, "value": p.value })))
        .collect();

    liquid::object!({
        "labels": &s.labels,
        "points": points,
        "min": s.min,
        "max": s.max,
        "first": s.first,
        "last": s.last,
        "count": s.count as i64,
    })
}

#[cfg(test)]
mod range_render_tests {
    use super::*;
    use crate::models::{RangePoint, RangeSeries, Template};
    use std::collections::HashMap;

    fn template_with(content: &str) -> Template {
        Template {
            id: 1,
            name: "t".into(),
            content: content.into(),
            created_at: chrono::Utc::now().naive_utc(),
            updated_at: chrono::Utc::now().naive_utc(),
        }
    }

    #[test]
    fn test_range_series_is_drawable_from_a_template() {
        let series = RangeSeries::from_points(
            HashMap::new(),
            vec![
                RangePoint {
                    t: 1.0,
                    value: 10.0,
                },
                RangePoint {
                    t: 2.0,
                    value: 30.0,
                },
                RangePoint {
                    t: 3.0,
                    value: 20.0,
                },
            ],
        );
        // Mimic exactly how the renderer exposes `prometheus_range.<name>`.
        let globals = liquid::object!({
            "prometheus_range": liquid::object!({
                "cpu": vec![liquid::model::Value::Object(range_series_to_object(&series))],
            }),
        });
        let tpl = template_with(
            "n={{ prometheus_range.cpu[0].count }} \
             min={{ prometheus_range.cpu[0].min }} \
             max={{ prometheus_range.cpu[0].max }} \
             last={{ prometheus_range.cpu[0].last }} \
             pts={% for p in prometheus_range.cpu[0].points %}{{ p.value }};{% endfor %}",
        );
        let out = tpl
            .render(globals)
            .expect("template should render range data");
        assert_eq!(out, "n=3 min=10 max=30 last=20 pts=10;30;20;");
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
