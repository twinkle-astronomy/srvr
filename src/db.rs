use std::sync::OnceLock;

use dioxus::prelude::*;
use sqlx::{
    FromRow, SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteRow},
};

use crate::models::{Device, PrometheusQuery, Template};

static POOL: OnceLock<SqlitePool> = OnceLock::new();

pub async fn init() -> &'static SqlitePool {
    let db_url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:./data/devices.db".to_string());

    std::fs::create_dir_all("./data").expect("Failed to create data directory");

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(
            db_url
                .parse::<SqliteConnectOptions>()
                .expect("Invalid DATABASE_URL")
                .create_if_missing(true)
                .journal_mode(SqliteJournalMode::Wal),
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

pub async fn get_device_by_token(token: &str) -> Result<Option<Device>, sqlx::error::Error> {
    sqlx::query_as(
        "SELECT id, access_token, mac_address, model, friendly_id, fw_version, width, height, battery_voltage, rssi, last_seen_at, created_at \
         FROM devices
         WHERE access_token = $1
         ORDER BY last_seen_at DESC"
    )
        .bind(token)
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

pub async fn get_or_create_device(
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
    if let Ok(Some(device)) = get_device_by_token(access_token).await {
        return Ok(device);
    }

    let device_id: SqliteRow = sqlx::query(
        "INSERT INTO devices (access_token, mac_address, model, friendly_id, battery_voltage, fw_version, rssi, width, height) \
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) \
        ON CONFLICT(access_token) DO UPDATE SET \
        battery_voltage = COALESCE(excluded.battery_voltage, battery_voltage), \
        fw_version = COALESCE(excluded.fw_version, fw_version), \
        rssi = COALESCE(excluded.rssi, rssi), \
        width = COALESCE(excluded.width, width), \
        height = COALESCE(excluded.height, height), \
        last_seen_at = datetime('now'), \
        updated_at = datetime('now') \
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
