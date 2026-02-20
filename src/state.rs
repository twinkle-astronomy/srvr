use std::sync::Arc;

use axum::extract::FromRef;
use chrono::Utc;
use diesel::{ExpressionMethods, QueryDsl, SelectableHelper, SqliteConnection};
use diesel_async::{RunQueryDsl, sync_connection_wrapper::SyncConnectionWrapper};
use leptos::config::LeptosOptions;
use thiserror::Error;
use tokio::sync::Mutex;
use tracing::{error, instrument};

use crate::{
    device::{Device, NewDevice},
    schema::devices::{self},
};
use std::ops::DerefMut;

#[derive(Error, Debug)]
pub enum Error {
    #[error("{0}")]
    DieselError(#[from] diesel::result::Error),
    #[error("{0}")]
    ConnectionError(#[from] diesel::ConnectionError),
}

pub type Db = Arc<Mutex<SyncConnectionWrapper<SqliteConnection>>>;

#[derive(Clone)]
pub struct AppState {
    pub leptos_options: LeptosOptions,
    pub db: Db,
}

impl FromRef<AppState> for LeptosOptions {
    fn from_ref(state: &AppState) -> Self {
        state.leptos_options.clone()
    }
}

impl AppState {
    #[instrument(skip(self))]
    pub async fn get_all_devices(&self) -> Result<Vec<Device>, diesel::result::Error> {
        let mut conn = self.db.lock().await;

        devices::table
            .select(Device::as_select())
            .order_by(devices::created_at.desc())
            .get_results(conn.deref_mut())
            .await
    }
    #[instrument(skip(self))]
    pub async fn get_or_create_device(
        &self,
        mac_address: Option<&str>,
        access_token: &str,
        fw_version: Option<&str>,
    ) -> Result<Device, diesel::result::Error> {
        let mut conn = self.db.lock().await;

        // Get or create the device by api_key
        let device: Device = match devices::table
            .filter(devices::api_key.eq(access_token))
            .select(Device::as_select())
            .first(conn.deref_mut())
            .await
        {
            Ok(device) => device,
            Err(diesel::result::Error::NotFound) => {
                let mac = mac_address.unwrap_or(access_token);
                let new_device = NewDevice {
                    mac_address: mac,
                    api_key: &access_token,
                    firmware_version: fw_version,
                };
                diesel::insert_into(devices::table)
                    .values(&new_device)
                    .returning(Device::as_returning())
                    .get_result(conn.deref_mut())
                    .await?
            }
            Err(e) => {
                error!("Error getting device: {:?}", e);
                return Err(e);
            }
        };

        // Update last_seen_at
        let update_query = diesel::update(devices::table.find(device.id))
            .set(devices::last_seen_at.eq(Utc::now().naive_utc()));
        let _ = update_query.execute(conn.deref_mut()).await;

        // Update firmware_version if provided
        if let Some(fw) = fw_version {
            let update_query = diesel::update(devices::table.find(device.id))
                .set(devices::firmware_version.eq(fw));
            let _ = update_query.execute(conn.deref_mut()).await;
        }
        Ok(device)
    }
}
