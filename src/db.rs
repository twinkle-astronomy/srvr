use std::sync::OnceLock;

use dioxus::prelude::*;
use sqlx::{
    FromRow, SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteRow},
};

use crate::models::{Device, DeviceLog, DeviceLogEntry, PrometheusQuery, Template};

static POOL: OnceLock<SqlitePool> = OnceLock::new();

pub async fn init() -> &'static SqlitePool {
    let db_url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:./data/devices.db".to_string());

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(
            db_url
                .parse::<SqliteConnectOptions>()
                .expect("Invalid DATABASE_URL")
                .create_if_missing(true)
                .journal_mode(SqliteJournalMode::Wal)
                .pragma("foreign_keys", "ON"),
        )
        .await
        .expect("Failed to connect to SQLite");

    POOL.set(pool).expect("Database already initialized");
    POOL.get().unwrap()
}

pub fn get() -> &'static SqlitePool {
    POOL.get()
        .expect("Database not initialized. Call db::init() first.")
}

pub async fn get_template() -> Result<Template, sqlx::error::Error> {
    let conn = get();

    match sqlx::query_as(
        r#"
        SELECT id, content, updated_at, created_at FROM templates
    "#,
    )
    .fetch_optional(conn)
    .await
    {
        Ok(Some(t)) => Ok(t),

        Ok(None) => {
            let svg_template = include_str!("../assets/default_screen.svg.liquid");

            let _ = sqlx::query(
                "INSERT INTO templates (content, updated_at, created_at) \
                VALUES (?, datetime('now'), datetime('now')) \
                RETURNING id, content, updated_at, created_at",
            )
            .bind(&svg_template)
            .execute(conn)
            .await;

            sqlx::query_as(
                r#"
                    SELECT id, content, updated_at, created_at FROM templates
                "#,
            )
            .fetch_one(conn)
            .await
        }
        Err(e) => {
            error!("Error getting template: {:?}", e);
            Err(e)
        }
    }
}

pub async fn get_device_logs(
    device_id: i64,
    limit: i64,
) -> Result<Vec<DeviceLog>, sqlx::error::Error> {
    sqlx::query_as(
        "SELECT id, device_id, device_log_id, battery_voltage, created_at, firmware_version, \
         free_heap_size, max_alloc_size, message, refresh_rate, sleep_duration, source_line, \
         source_path, special_function, wake_reason, wifi_signal, wifi_status, logged_at \
         FROM device_logs \
         WHERE device_id = ? \
         ORDER BY logged_at DESC \
         LIMIT ?",
    )
    .bind(device_id)
    .bind(limit)
    .fetch_all(get())
    .await
}

pub async fn get_device_id_by_access_token(
    access_token: &str,
) -> Result<Option<i64>, sqlx::error::Error> {
    let row: Option<(i64,)> = sqlx::query_as("SELECT id FROM devices WHERE access_token = ?")
        .bind(access_token)
        .fetch_optional(get())
        .await?;
    Ok(row.map(|(id,)| id))
}

pub async fn insert_device_logs(
    device_id: i64,
    logs: &[DeviceLogEntry],
) -> Result<(), sqlx::error::Error> {
    let conn = get();
    for log in logs {
        sqlx::query(
            "INSERT INTO device_logs \
             (device_id, device_log_id, battery_voltage, created_at, firmware_version, \
              free_heap_size, max_alloc_size, message, refresh_rate, sleep_duration, \
              source_line, source_path, special_function, wake_reason, wifi_signal, wifi_status) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(device_id)
        .bind(log.id)
        .bind(log.battery_voltage)
        .bind(log.created_at)
        .bind(&log.firmware_version)
        .bind(log.free_heap_size)
        .bind(log.max_alloc_size)
        .bind(&log.message)
        .bind(log.refresh_rate)
        .bind(log.sleep_duration)
        .bind(log.source_line)
        .bind(&log.source_path)
        .bind(&log.special_function)
        .bind(&log.wake_reason)
        .bind(log.wifi_signal)
        .bind(&log.wifi_status)
        .execute(conn)
        .await?;
    }
    Ok(())
}

pub async fn delete_device(device_id: i64) -> Result<(), sqlx::error::Error> {
    sqlx::query("DELETE FROM devices WHERE id = ?")
        .bind(device_id)
        .execute(get())
        .await?;
    Ok(())
}

pub async fn get_device(device_id: i64) -> Result<Option<Device>, sqlx::error::Error> {
    sqlx::query_as(
        "SELECT id, access_token, mac_address, model, friendly_id, fw_version, width, height, battery_voltage, rssi, last_seen_at, created_at \
         FROM devices
         WHERE id = $1
         ORDER BY last_seen_at DESC"
    )
        .bind(device_id)
        .fetch_optional(get())
        .await
}

pub async fn get_devices() -> Result<Vec<Device>, sqlx::error::Error> {
    sqlx::query_as(
        "SELECT id, access_token, mac_address, model, friendly_id, fw_version, width, height, battery_voltage, rssi, last_seen_at, created_at \
         FROM devices ORDER BY last_seen_at DESC"
    )
        .fetch_all(get())
        .await
}

pub async fn update_template(id: i64, content: &str) -> Result<(), sqlx::error::Error> {
    let conn = get();

    sqlx::query("UPDATE templates SET content = ?, updated_at = datetime('now') WHERE id = ?")
        .bind(content)
        .bind(id)
        .execute(conn)
        .await?;

    Ok(())
}

pub async fn get_and_update_device_by_access_token(
    access_token: &str,
    mac_address: Option<&str>,
    model: Option<&str>,
    fw_version: Option<&str>,
    width: Option<i64>,
    height: Option<i64>,
    battery_voltage: Option<f32>,
    rssi: Option<&str>,
) -> Result<Device, sqlx::error::Error> {
    let device_row: SqliteRow = sqlx::query(
        "UPDATE devices \
        SET mac_address = ?, model = ?, battery_voltage = ?, fw_version = ?, rssi = ?, width = ?, height = ? \
        WHERE access_token = ?
        RETURNING *",
    )
    .bind(mac_address)
    .bind(model)
    .bind(battery_voltage)
    .bind(fw_version)
    .bind(rssi)
    .bind(width)
    .bind(height)
    .bind(access_token)
    .fetch_one(get())
    .await?;

    Device::from_row(&device_row)
}

pub async fn create_device(
    access_token: &str,
    mac_address: Option<&str>,
    model: Option<&str>,
    friendly_id: &str,
    fw_version: Option<&str>,
    width: Option<i64>,
    height: Option<i64>,
    battery_voltage: Option<f32>,
    rssi: Option<&str>,
) -> Result<Device, sqlx::error::Error> {
    let device_id: SqliteRow = sqlx::query(
        "INSERT INTO devices (access_token, mac_address, model, friendly_id, battery_voltage, fw_version, rssi, width, height) \
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) \
        RETURNING *",
    )
    .bind(access_token)
    .bind(mac_address)
    .bind(model)
    .bind(friendly_id)
    .bind(battery_voltage)
    .bind(fw_version)
    .bind(rssi)
    .bind(width)
    .bind(height)
    .fetch_one(get())
    .await?;

    Device::from_row(&device_id)
}

pub async fn update_prometheus_query(
    id: i64,
    name: &str,
    addr: &str,
    query: &str,
) -> Result<(), sqlx::error::Error> {
    sqlx::query(
        "UPDATE prometheus_queries SET name = ?, addr = ?, query = ?, updated_at = datetime('now') WHERE id = ?",
    )
    .bind(name)
    .bind(addr)
    .bind(query)
    .bind(id)
    .execute(get())
    .await?;

    Ok(())
}

pub async fn delete_prometheus_query(id: i64) -> Result<(), sqlx::error::Error> {
    sqlx::query("DELETE FROM prometheus_queries WHERE id = ?")
        .bind(id)
        .execute(get())
        .await?;

    Ok(())
}

pub async fn create_prometheus_query(
    template_id: i64,
    name: &str,
    addr: &str,
    query: &str,
) -> Result<(), sqlx::error::Error> {
    sqlx::query(
        "INSERT INTO prometheus_queries (template_id, name, addr, query, created_at, updated_at) \
         VALUES (?, ?, ?, ?, datetime('now'), datetime('now'))",
    )
    .bind(template_id)
    .bind(name)
    .bind(addr)
    .bind(query)
    .execute(get())
    .await?;

    Ok(())
}

pub async fn get_prometheus_queries(
    template_id: i64,
) -> Result<Vec<PrometheusQuery>, sqlx::error::Error> {
    sqlx::query_as(
        "SELECT id, template_id, name, addr, query, created_at, updated_at \
         FROM prometheus_queries
         WHERE template_id = ?
         ORDER BY name",
    )
    .bind(template_id)
    .fetch_all(get())
    .await
}
