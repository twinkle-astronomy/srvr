#[cfg(feature = "server")]
use sqlx::FromRow;

use chrono::NaiveDateTime;
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};


#[cfg_attr(feature = "server", derive(FromRow))]
#[derive(Debug, Serialize, Deserialize)]
pub struct Template {
    id: i64,
    pub(crate) content: String,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
}

#[cfg(feature = "server")]
use liquid::{Error, Object};
#[cfg(feature = "server")]
use liquid::ParserBuilder;
#[cfg(feature = "server")]
impl Template {
    pub fn render(&self, globals: Object) -> Result<String, Error> {
        

        let parser = ParserBuilder::with_stdlib().build()?;

        let template = parser.parse(&self.content)?;

        Ok(template.render(&globals)?)
    }
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
        } )
    }
}