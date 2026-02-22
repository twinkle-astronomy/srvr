use dioxus::prelude::*;
use liquid::ParserBuilder;
use liquid::{Error, Object};

use crate::models::{Device, PrometheusQuery, Template};

impl Template {
    pub fn render(&self, globals: Object) -> Result<String, Error> {
        let parser = ParserBuilder::with_stdlib().build()?;

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
        })
    }
}

impl PrometheusQuery {
    pub async fn get_render_obj(&self) -> Result<Vec<Object>, prometheus_http_query::error::Error> {
        // let prometheus_url =
        //     std::env::var("PROMETHEUS_URL").unwrap_or_else(|_| "http://prometheus:9090".to_string());
        let client = prometheus_http_query::Client::try_from(self.addr.as_str()).unwrap();

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
