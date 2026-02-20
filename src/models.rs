use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct Device {
    pub id: i32,
    pub mac_address: String,
    pub friendly_name: Option<String>,
    pub api_key: String,
    pub firmware_version: Option<String>,
    // pub created_at: NaiveDateTime,
    // pub last_seen_at: Option<NaiveDateTime>,
}
