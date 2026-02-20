pub mod api;
pub mod renderer;

use chrono::NaiveDateTime;
use diesel::prelude::*;

#[derive(Queryable, Selectable, Debug)]
#[diesel(table_name = crate::schema::devices)]
pub struct Device {
    pub id: i32,
    pub mac_address: String,
    pub friendly_name: Option<String>,
    pub api_key: String,
    pub firmware_version: Option<String>,
    pub created_at: NaiveDateTime,
    pub last_seen_at: Option<NaiveDateTime>,
}

#[derive(Insertable)]
#[diesel(table_name = crate::schema::devices)]
pub struct NewDevice<'a> {
    pub mac_address: &'a str,
    pub api_key: &'a str,
    pub firmware_version: Option<&'a str>,
}
