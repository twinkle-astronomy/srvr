use std::{fs, path::Path, sync::OnceLock};

use dioxus::prelude::*;
use sqlx::{
    FromRow, SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteRow},
};

use crate::models::{
    Device, DeviceLog, DeviceLogEntry, FirmwareRelease, HttpSource, PrometheusQuery, RangeQuery,
    Template, User,
};

static POOL: OnceLock<SqlitePool> = OnceLock::new();

pub async fn init() -> &'static SqlitePool {
    let db_url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:///app/.data/data.db".to_string());

    let db_path = db_url.strip_prefix("sqlite://").unwrap_or(&db_url);
    let path = Path::new(db_path);

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("Failed to create directory");
    }

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

pub async fn get_default_template() -> Result<Template, sqlx::error::Error> {
    let conn = get();

    match sqlx::query_as(
        "SELECT id, name, content, updated_at, created_at FROM templates ORDER BY id ASC LIMIT 1",
    )
    .fetch_optional(conn)
    .await
    {
        Ok(Some(t)) => Ok(t),

        Ok(None) => {
            let svg_template = include_str!("../assets/default_screen.svg.liquid");

            let _ = sqlx::query(
                "INSERT INTO templates (name, content, updated_at, created_at) \
                VALUES ('Default', ?, datetime('now'), datetime('now'))",
            )
            .bind(&svg_template)
            .execute(conn)
            .await;

            sqlx::query_as(
                "SELECT id, name, content, updated_at, created_at FROM templates ORDER BY id ASC LIMIT 1",
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

pub async fn get_templates() -> Result<Vec<Template>, sqlx::error::Error> {
    sqlx::query_as("SELECT id, name, content, updated_at, created_at FROM templates ORDER BY name")
        .fetch_all(get())
        .await
}

pub async fn get_template_by_id(id: i64) -> Result<Template, sqlx::error::Error> {
    sqlx::query_as("SELECT id, name, content, updated_at, created_at FROM templates WHERE id = ?")
        .bind(id)
        .fetch_one(get())
        .await
}

pub async fn get_template_for_device(device_id: i64) -> Result<Template, sqlx::error::Error> {
    sqlx::query_as(
        "SELECT t.id, t.name, t.content, t.updated_at, t.created_at \
         FROM templates t \
         JOIN devices d ON d.template_id = t.id \
         WHERE d.id = ?",
    )
    .bind(device_id)
    .fetch_one(get())
    .await
}

pub async fn create_template(name: &str, content: &str) -> Result<Template, sqlx::error::Error> {
    let row: SqliteRow = sqlx::query(
        "INSERT INTO templates (name, content, updated_at, created_at) \
         VALUES (?, ?, datetime('now'), datetime('now')) \
         RETURNING *",
    )
    .bind(name)
    .bind(content)
    .fetch_one(get())
    .await?;

    Template::from_row(&row)
}

pub async fn delete_template(id: i64) -> Result<(), sqlx::error::Error> {
    for http in get_http_sources(id).await? {
        delete_http_source(http.id.unwrap()).await?;
    }

    for prom in get_prometheus_queries(id).await? {
        delete_prometheus_query(prom.id.unwrap()).await?;
    }

    for range in get_range_queries(id).await? {
        delete_range_query(range.id.unwrap()).await?;
    }

    sqlx::query("DELETE FROM templates WHERE id = ?")
        .bind(id)
        .execute(get())
        .await?;
    Ok(())
}

pub async fn update_device_template(
    device_id: i64,
    template_id: i64,
) -> Result<(), sqlx::error::Error> {
    sqlx::query("UPDATE devices SET template_id = ? WHERE id = ?")
        .bind(template_id)
        .bind(device_id)
        .execute(get())
        .await?;
    Ok(())
}

pub async fn update_device_maximum_compatibility(
    device_id: i64,
    maximum_compatibility: bool,
) -> Result<(), sqlx::error::Error> {
    sqlx::query("UPDATE devices SET maximum_compatibility = ? WHERE id = ?")
        .bind(maximum_compatibility)
        .bind(device_id)
        .execute(get())
        .await?;
    Ok(())
}

// Mechanical mirror of update_device_maximum_compatibility above — same
// shape, different column. Covered by a characterization round-trip test
// rather than strict test-first (per development-process.md's allowance for
// near-verbatim CRUD mirrors).
pub async fn update_device_firmware_updates_enabled(
    device_id: i64,
    firmware_updates_enabled: bool,
) -> Result<(), sqlx::error::Error> {
    sqlx::query("UPDATE devices SET firmware_updates_enabled = ? WHERE id = ?")
        .bind(firmware_updates_enabled)
        .bind(device_id)
        .execute(get())
        .await?;
    Ok(())
}

// Mechanical mirror of update_device_maximum_compatibility above — same
// shape, different column. Covered by a characterization round-trip test
// rather than strict test-first (per development-process.md's allowance for
// near-verbatim CRUD mirrors).
pub async fn update_device_supports_2bit_grayscale(
    device_id: i64,
    supports_2bit_grayscale: bool,
) -> Result<(), sqlx::error::Error> {
    sqlx::query("UPDATE devices SET supports_2bit_grayscale = ? WHERE id = ?")
        .bind(supports_2bit_grayscale)
        .bind(device_id)
        .execute(get())
        .await?;
    Ok(())
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

pub async fn get_device(device_id: i64) -> Result<Device, sqlx::error::Error> {
    sqlx::query_as(
        "SELECT id, access_token, mac_address, model, friendly_id, fw_version, width, height, battery_voltage, rssi, template_id, maximum_compatibility, firmware_updates_enabled, supports_2bit_grayscale, last_seen_at, created_at \
         FROM devices
         WHERE id = $1
         ORDER BY last_seen_at DESC"
    )
        .bind(device_id)
        .fetch_one(get())
        .await
}

pub async fn get_devices() -> Result<Vec<Device>, sqlx::error::Error> {
    sqlx::query_as(
        "SELECT id, access_token, mac_address, model, friendly_id, fw_version, width, height, battery_voltage, rssi, template_id, maximum_compatibility, firmware_updates_enabled, supports_2bit_grayscale, last_seen_at, created_at \
         FROM devices ORDER BY last_seen_at DESC"
    )
        .fetch_all(get())
        .await
}

pub async fn update_template(id: i64, name: &str, content: &str) -> Result<(), sqlx::error::Error> {
    let conn = get();

    sqlx::query(
        "UPDATE templates SET name = ?, content = ?, updated_at = datetime('now') WHERE id = ?",
    )
    .bind(name)
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
    // model/width/height are NOT NULL; COALESCE keeps the existing value on
    // a poll that omits one of those headers instead of failing the update
    // (some real-world firmware only ever sends ID + FW-Version).
    let device_row: SqliteRow = sqlx::query(
        "UPDATE devices \
        SET mac_address = ?, model = COALESCE(?, model), battery_voltage = ?, fw_version = ?, rssi = ?, width = COALESCE(?, width), height = COALESCE(?, height) \
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
    let default_template = get_default_template().await?;

    let device_id: SqliteRow = sqlx::query(
        "INSERT INTO devices (access_token, mac_address, model, friendly_id, battery_voltage, fw_version, rssi, width, height, template_id) \
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
        ON CONFLICT(mac_address) DO UPDATE SET mac_address = excluded.mac_address \
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
    .bind(default_template.id)
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
) -> Result<PrometheusQuery, sqlx::error::Error> {
    let r = sqlx::query(
        "INSERT INTO prometheus_queries (template_id, name, addr, query, created_at, updated_at) \
         VALUES (?, ?, ?, ?, datetime('now'), datetime('now'))
         RETURNING *",
    )
    .bind(template_id)
    .bind(name)
    .bind(addr)
    .bind(query)
    .fetch_one(get())
    .await?;

    PrometheusQuery::from_row(&r)
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

pub async fn get_range_queries(template_id: i64) -> Result<Vec<RangeQuery>, sqlx::error::Error> {
    sqlx::query_as(
        "SELECT id, template_id, name, addr, query, duration, step, created_at, updated_at \
         FROM range_queries
         WHERE template_id = ?
         ORDER BY name",
    )
    .bind(template_id)
    .fetch_all(get())
    .await
}

pub async fn create_range_query(
    template_id: i64,
    name: &str,
    addr: &str,
    query: &str,
    duration: &str,
    step: &str,
) -> Result<RangeQuery, sqlx::error::Error> {
    let r = sqlx::query(
        "INSERT INTO range_queries (template_id, name, addr, query, duration, step, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, datetime('now'), datetime('now'))
         RETURNING *",
    )
    .bind(template_id)
    .bind(name)
    .bind(addr)
    .bind(query)
    .bind(duration)
    .bind(step)
    .fetch_one(get())
    .await?;

    RangeQuery::from_row(&r)
}

pub async fn update_range_query(
    id: i64,
    name: &str,
    addr: &str,
    query: &str,
    duration: &str,
    step: &str,
) -> Result<(), sqlx::error::Error> {
    sqlx::query(
        "UPDATE range_queries SET name = ?, addr = ?, query = ?, duration = ?, step = ?, \
         updated_at = datetime('now') WHERE id = ?",
    )
    .bind(name)
    .bind(addr)
    .bind(query)
    .bind(duration)
    .bind(step)
    .bind(id)
    .execute(get())
    .await?;

    Ok(())
}

pub async fn delete_range_query(id: i64) -> Result<(), sqlx::error::Error> {
    sqlx::query("DELETE FROM range_queries WHERE id = ?")
        .bind(id)
        .execute(get())
        .await?;

    Ok(())
}

pub async fn get_http_sources(template_id: i64) -> Result<Vec<HttpSource>, sqlx::error::Error> {
    sqlx::query_as(
        "SELECT id, template_id, name, url, created_at, updated_at \
         FROM http_sources
         WHERE template_id = ?
         ORDER BY name",
    )
    .bind(template_id)
    .fetch_all(get())
    .await
}

pub async fn create_http_source(
    template_id: i64,
    name: &str,
    url: &str,
) -> Result<HttpSource, sqlx::error::Error> {
    let r = sqlx::query(
        "INSERT INTO http_sources (template_id, name, url, created_at, updated_at) \
         VALUES (?, ?, ?, datetime('now'), datetime('now'))
         RETURNING *",
    )
    .bind(template_id)
    .bind(name)
    .bind(url)
    .fetch_one(get())
    .await?;

    HttpSource::from_row(&r)
}

pub async fn update_http_source(id: i64, name: &str, url: &str) -> Result<(), sqlx::error::Error> {
    sqlx::query(
        "UPDATE http_sources SET name = ?, url = ?, updated_at = datetime('now') WHERE id = ?",
    )
    .bind(name)
    .bind(url)
    .bind(id)
    .execute(get())
    .await?;

    Ok(())
}

pub async fn delete_http_source(id: i64) -> Result<(), sqlx::error::Error> {
    sqlx::query("DELETE FROM http_sources WHERE id = ?")
        .bind(id)
        .execute(get())
        .await?;

    Ok(())
}

// --- Firmware releases ---
//
// Mechanical mirrors of the range_query / http_source CRUD above (INSERT
// RETURNING, SELECT list, DELETE by id) — covered by round-trip
// characterization tests rather than strict test-first, per
// development-process.md's allowance for near-verbatim CRUD. The one
// exception is `activate_firmware_release`, which has real branching
// (deactivate the sibling release before activating the target) and is
// test-first below.

pub async fn create_firmware_release(
    model: &str,
    version: &str,
    filename: &str,
    size_bytes: i64,
    binary: &[u8],
) -> Result<FirmwareRelease, sqlx::error::Error> {
    sqlx::query_as(
        "INSERT INTO firmware_releases (model, version, filename, size_bytes, binary, active, created_at) \
         VALUES (?, ?, ?, ?, ?, 0, datetime('now')) \
         RETURNING id, model, version, filename, size_bytes, active, created_at",
    )
    .bind(model)
    .bind(version)
    .bind(filename)
    .bind(size_bytes)
    .bind(binary)
    .fetch_one(get())
    .await
}

pub async fn get_firmware_releases() -> Result<Vec<FirmwareRelease>, sqlx::error::Error> {
    sqlx::query_as(
        "SELECT id, model, version, filename, size_bytes, active, created_at \
         FROM firmware_releases ORDER BY model ASC, created_at DESC",
    )
    .fetch_all(get())
    .await
}

pub async fn get_firmware_release(id: i64) -> Result<FirmwareRelease, sqlx::error::Error> {
    sqlx::query_as(
        "SELECT id, model, version, filename, size_bytes, active, created_at \
         FROM firmware_releases WHERE id = ?",
    )
    .bind(id)
    .fetch_one(get())
    .await
}

pub async fn get_active_firmware_release(
    model: &str,
) -> Result<Option<FirmwareRelease>, sqlx::error::Error> {
    sqlx::query_as(
        "SELECT id, model, version, filename, size_bytes, active, created_at \
         FROM firmware_releases WHERE model = ? AND active = 1",
    )
    .bind(model)
    .fetch_optional(get())
    .await
}

/// Filename + raw bytes for the active release of a model, for the device
/// download endpoint. Kept separate from `FirmwareRelease` so the binary
/// column is never pulled into the admin list/get queries above.
pub async fn get_active_firmware_binary(
    model: &str,
) -> Result<Option<(String, Vec<u8>)>, sqlx::error::Error> {
    sqlx::query_as("SELECT filename, binary FROM firmware_releases WHERE model = ? AND active = 1")
        .bind(model)
        .fetch_optional(get())
        .await
}

/// Deletes only if the release is (still) inactive; returns whether a row
/// was deleted. The `active = 0` predicate makes the handler's
/// check-then-delete race-free: a release activated between the handler's
/// check and this statement survives.
pub async fn delete_firmware_release_if_inactive(id: i64) -> Result<bool, sqlx::error::Error> {
    let result = sqlx::query("DELETE FROM firmware_releases WHERE id = ? AND active = 0")
        .bind(id)
        .execute(get())
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Activate `id` as the active release for its model, deactivating whichever
/// release currently holds that spot. `firmware_releases` has a unique index
/// on `(model) WHERE active = 1`, so leaving the old row active while
/// flipping the new one on would violate it — this must happen as a
/// deactivate-then-activate transaction, not a single UPDATE.
pub async fn activate_firmware_release(id: i64) -> Result<(), sqlx::error::Error> {
    let mut tx = get().begin().await?;
    sqlx::query(
        "UPDATE firmware_releases SET active = 0 \
         WHERE model = (SELECT model FROM firmware_releases WHERE id = ?)",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;
    let result = sqlx::query("UPDATE firmware_releases SET active = 1 WHERE id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    // Both UPDATEs silently no-op for an unknown id (the subquery yields
    // NULL), which would turn "activate a release someone just deleted" into
    // a 204 success. Surface it as RowNotFound instead; the dropped tx
    // rolls back.
    if result.rows_affected() == 0 {
        return Err(sqlx::Error::RowNotFound);
    }
    tx.commit().await?;
    Ok(())
}

pub async fn copy_template(source_id: i64) -> Result<Template, sqlx::error::Error> {
    let source = get_template_by_id(source_id).await?;

    let new_name = format!(
        "{} (Copy {})",
        source.name,
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
    );
    let new_template = create_template(&new_name, &source.content).await?;

    let prom_queries = get_prometheus_queries(source_id).await?;
    for pq in prom_queries {
        create_prometheus_query(new_template.id, &pq.name, &pq.addr, &pq.query).await?;
    }

    let range_queries = get_range_queries(source_id).await?;
    for rq in range_queries {
        create_range_query(
            new_template.id,
            &rq.name,
            &rq.addr,
            &rq.query,
            &rq.duration,
            &rq.step,
        )
        .await?;
    }

    let http_sources = get_http_sources(source_id).await?;
    for hs in http_sources {
        create_http_source(new_template.id, &hs.name, &hs.url).await?;
    }

    Ok(new_template)
}

pub async fn user_count() -> Result<i64, sqlx::error::Error> {
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
        .fetch_one(get())
        .await?;
    Ok(row.0)
}

pub async fn get_user_by_id(id: i64) -> Result<Option<User>, sqlx::error::Error> {
    sqlx::query_as(
        "SELECT id, username, password_hash, claude_api_key, created_at, updated_at \
         FROM users WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(get())
    .await
}

pub async fn get_user_by_username(username: &str) -> Result<Option<User>, sqlx::error::Error> {
    sqlx::query_as(
        "SELECT id, username, password_hash, claude_api_key, created_at, updated_at \
         FROM users WHERE username = ?",
    )
    .bind(username)
    .fetch_optional(get())
    .await
}

pub async fn create_user(username: &str, password_hash: &str) -> Result<User, sqlx::error::Error> {
    let row: SqliteRow = sqlx::query(
        "INSERT INTO users (username, password_hash, created_at, updated_at) \
         VALUES (?, ?, datetime('now'), datetime('now')) \
         RETURNING *",
    )
    .bind(username)
    .bind(password_hash)
    .fetch_one(get())
    .await?;

    User::from_row(&row)
}

pub async fn get_users() -> Result<Vec<User>, sqlx::error::Error> {
    sqlx::query_as(
        "SELECT id, username, password_hash, claude_api_key, created_at, updated_at \
         FROM users ORDER BY created_at",
    )
    .fetch_all(get())
    .await
}

pub async fn update_user_password(id: i64, password_hash: &str) -> Result<(), sqlx::error::Error> {
    sqlx::query("UPDATE users SET password_hash = ?, updated_at = datetime('now') WHERE id = ?")
        .bind(password_hash)
        .bind(id)
        .execute(get())
        .await?;
    Ok(())
}

pub async fn delete_user(id: i64) -> Result<(), sqlx::error::Error> {
    sqlx::query("DELETE FROM users WHERE id = ?")
        .bind(id)
        .execute(get())
        .await?;
    Ok(())
}

pub async fn get_claude_api_key(user_id: i64) -> Result<Option<String>, sqlx::error::Error> {
    let row: Option<(Option<String>,)> =
        sqlx::query_as("SELECT claude_api_key FROM users WHERE id = ?")
            .bind(user_id)
            .fetch_optional(get())
            .await?;
    Ok(row.and_then(|(key,)| key))
}

pub async fn set_claude_api_key(user_id: i64, key: &str) -> Result<(), sqlx::error::Error> {
    sqlx::query("UPDATE users SET claude_api_key = ?, updated_at = datetime('now') WHERE id = ?")
        .bind(key)
        .bind(user_id)
        .execute(get())
        .await?;
    Ok(())
}

pub async fn clear_claude_api_key(user_id: i64) -> Result<(), sqlx::error::Error> {
    sqlx::query(
        "UPDATE users SET claude_api_key = NULL, updated_at = datetime('now') WHERE id = ?",
    )
    .bind(user_id)
    .execute(get())
    .await?;
    Ok(())
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use sqlx::{Connection, sqlite::SqliteConnection};
    use tokio::sync::OnceCell;

    static INIT: OnceCell<()> = OnceCell::const_new();
    static INIT_TRACING: std::sync::Once = std::sync::Once::new();

    /// `tracing`'s per-callsite `Interest` cache is process-wide: whichever
    /// subscriber (or lack of one) first evaluates a given `info!`/`error!`
    /// call site can cache "not interested" for it, silently suppressing
    /// that call site for every thread afterwards — including a test that
    /// later installs its own thread-local subscriber via
    /// `tracing::subscriber::set_default` to capture output, since that
    /// override only controls *where* an event goes, not whether the cached
    /// Interest lets it fire at all. Installing one permissive **global**
    /// default, once, before any test's handlers can run keeps every
    /// call site's cached Interest at "always fire", so later per-test
    /// `set_default` overrides route reliably. `call_once` blocks
    /// concurrent callers until the first one finishes, so this closes the
    /// race even under parallel test execution.
    fn ensure_global_tracing_default() {
        INIT_TRACING.call_once(|| {
            let subscriber = tracing_subscriber::fmt().with_writer(std::io::sink).finish();
            let _ = tracing::subscriber::set_global_default(subscriber);
        });
    }

    /// Initialize a process-wide in-memory SQLite pool with migrations applied,
    /// stored in the same global `POOL` that `db::get()` reads. Idempotent and
    /// safe to call from every test; the first caller wins and the rest reuse it.
    pub async fn init_test_db() {
        ensure_global_tracing_default();
        INIT.get_or_init(|| async {
            // Each `#[tokio::test]` runs on its own runtime. A plain
            // `sqlite::memory:` database is private to a single connection, so
            // when a later test on a different runtime acquires a fresh
            // connection it sees an empty database ("no such table"). A *named,
            // shared-cache* in-memory database is shared by every connection
            // that opens the same URI and persists as long as one connection
            // stays open — so all tests, on any runtime, see the same migrated
            // schema.
            let opts = "sqlite:file:trmnl_shared_test?mode=memory&cache=shared"
                .parse::<SqliteConnectOptions>()
                .expect("parse shared in-memory sqlite url")
                .create_if_missing(true)
                .pragma("foreign_keys", "ON");

            // A leaked keep-alive connection guarantees the shared in-memory DB
            // is never torn down (it vanishes once the last connection closes),
            // independent of the pool reaping idle connections.
            let keepalive = SqliteConnection::connect_with(&opts)
                .await
                .expect("open keep-alive connection to shared in-memory DB");
            std::mem::forget(keepalive);

            let pool = SqlitePoolOptions::new()
                .max_connections(5)
                .connect_with(opts)
                .await
                .expect("create shared in-memory test pool");
            sqlx::migrate!()
                .run(&pool)
                .await
                .expect("run migrations on test pool");
            POOL.set(pool).ok();
        })
        .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use test_support::init_test_db;

    static NEXT: AtomicU64 = AtomicU64::new(0);

    fn unique_suffix() -> String {
        format!("{}_{}", std::process::id(), NEXT.fetch_add(1, Ordering::SeqCst))
    }

    #[tokio::test]
    async fn test_activate_firmware_release_deactivates_sibling() {
        init_test_db().await;

        let model = format!("trmnl-og-{}", unique_suffix());
        let v1 = create_firmware_release(&model, "1.0.0", "fw-1.0.0.bin", 4, b"aaaa")
            .await
            .expect("create v1");
        let v2 = create_firmware_release(&model, "1.1.0", "fw-1.1.0.bin", 4, b"bbbb")
            .await
            .expect("create v2");

        activate_firmware_release(v1.id).await.expect("activate v1");
        let active = get_active_firmware_release(&model)
            .await
            .expect("get active")
            .expect("v1 should be active");
        assert_eq!(active.id, v1.id);

        // Activating v2 must flip v1 off — the DB has a unique index on
        // (model) WHERE active = 1, so leaving both on isn't just wrong,
        // it's unrepresentable.
        activate_firmware_release(v2.id).await.expect("activate v2");
        let active = get_active_firmware_release(&model)
            .await
            .expect("get active")
            .expect("v2 should be active");
        assert_eq!(active.id, v2.id, "activating v2 should supersede v1");

        let v1_after = get_firmware_release(v1.id).await.expect("get v1 after");
        assert!(!v1_after.active, "v1 should have been deactivated");
    }

    #[tokio::test]
    async fn test_firmware_release_crud_round_trip() {
        init_test_db().await;

        let model = format!("trmnl-crud-{}", unique_suffix());
        assert_eq!(
            get_active_firmware_release(&model).await.expect("get active before"),
            None,
            "no release should be active before any exist"
        );

        let created = create_firmware_release(&model, "2.0.0", "fw-2.0.0.bin", 4, b"cccc")
            .await
            .expect("create release");
        assert_eq!(created.model, model);
        assert_eq!(created.version, "2.0.0");
        assert_eq!(created.size_bytes, 4);
        assert!(!created.active, "newly created releases start inactive");

        let listed = get_firmware_releases().await.expect("list releases");
        assert!(listed.iter().any(|r| r.id == created.id));

        let binary = get_active_firmware_binary(&model)
            .await
            .expect("get active binary");
        assert_eq!(binary, None, "inactive release should not be returned as active");

        activate_firmware_release(created.id).await.expect("activate");
        let (filename, bytes) = get_active_firmware_binary(&model)
            .await
            .expect("get active binary after activate")
            .expect("should have an active binary now");
        assert_eq!(filename, "fw-2.0.0.bin");
        assert_eq!(bytes, b"cccc");

        // The delete predicate refuses while active…
        let deleted = delete_firmware_release_if_inactive(created.id)
            .await
            .expect("attempt delete of active release");
        assert!(!deleted, "an active release must survive the delete");

        // …and succeeds once a sibling supersedes it.
        let v2 = create_firmware_release(&model, "2.1.0", "fw-2.1.0.bin", 4, b"dddd")
            .await
            .expect("create superseding release");
        activate_firmware_release(v2.id).await.expect("activate v2");
        let deleted = delete_firmware_release_if_inactive(created.id)
            .await
            .expect("delete deactivated release");
        assert!(deleted, "a deactivated release should be deletable");
        let after_delete = get_firmware_releases().await.expect("list after delete");
        assert!(!after_delete.iter().any(|r| r.id == created.id));
    }

    #[tokio::test]
    async fn test_update_device_firmware_updates_enabled_round_trip() {
        init_test_db().await;

        let suffix = unique_suffix();
        let device = create_device(
            &format!("fw-toggle-token-{suffix}"),
            Some(&format!("aa:bb:cc:dd:ee:{suffix}")),
            Some("trmnl-og"),
            &format!("fw-toggle-device-{suffix}"),
            Some("1.0.0"),
            Some(800),
            Some(480),
            Some(3.9),
            Some("-60"),
        )
        .await
        .expect("create device");
        assert!(
            !device.firmware_updates_enabled,
            "firmware updates should default off"
        );

        update_device_firmware_updates_enabled(device.id, true)
            .await
            .expect("enable firmware updates");
        let after = get_device(device.id).await.expect("get device after enable");
        assert!(after.firmware_updates_enabled);

        update_device_firmware_updates_enabled(device.id, false)
            .await
            .expect("disable firmware updates");
        let after = get_device(device.id).await.expect("get device after disable");
        assert!(!after.firmware_updates_enabled);
    }

    #[tokio::test]
    async fn test_update_device_supports_2bit_grayscale_round_trip() {
        init_test_db().await;

        let suffix = unique_suffix();
        let device = create_device(
            &format!("grayscale-toggle-token-{suffix}"),
            Some(&format!("aa:bb:cc:dd:gg:{suffix}")),
            Some("trmnl-og"),
            &format!("grayscale-toggle-device-{suffix}"),
            Some("1.0.0"),
            Some(800),
            Some(480),
            Some(3.9),
            Some("-60"),
        )
        .await
        .expect("create device");
        assert!(
            !device.supports_2bit_grayscale,
            "2-bit grayscale should default off"
        );

        update_device_supports_2bit_grayscale(device.id, true)
            .await
            .expect("enable 2-bit grayscale");
        let after = get_device(device.id).await.expect("get device after enable");
        assert!(after.supports_2bit_grayscale);

        update_device_supports_2bit_grayscale(device.id, false)
            .await
            .expect("disable 2-bit grayscale");
        let after = get_device(device.id).await.expect("get device after disable");
        assert!(!after.supports_2bit_grayscale);
    }

    #[tokio::test]
    async fn test_range_query_crud_round_trip() {
        init_test_db().await;

        let template = create_template("range-crud-tpl", "<svg/>")
            .await
            .expect("create template");

        let created = create_range_query(
            template.id,
            "cpu",
            "http://prom:9090",
            "rate(cpu[5m])",
            "1h",
            "60s",
        )
        .await
        .expect("create range query");
        assert_eq!(created.name, "cpu");
        assert_eq!(created.duration, "1h");
        assert_eq!(created.step, "60s");
        let id = created.id.expect("created row has id");

        let fetched = get_range_queries(template.id)
            .await
            .expect("get range queries");
        assert_eq!(fetched.len(), 1);
        assert_eq!(fetched[0].query, "rate(cpu[5m])");

        update_range_query(id, "cpu2", "http://prom:9090", "rate(cpu[1m])", "30m", "5m")
            .await
            .expect("update range query");
        let after = get_range_queries(template.id).await.expect("re-get");
        assert_eq!(after[0].name, "cpu2");
        assert_eq!(after[0].duration, "30m");
        assert_eq!(after[0].step, "5m");
        assert_eq!(after[0].query, "rate(cpu[1m])");

        delete_range_query(id).await.expect("delete range query");
        let empty = get_range_queries(template.id)
            .await
            .expect("get after delete");
        assert!(empty.is_empty(), "range query should be gone after delete");
    }

    #[tokio::test]
    async fn test_claude_api_key_round_trip() {
        init_test_db().await;

        let username = format!("claude_key_user_{}", std::process::id());
        let hash = "not-a-real-hash";
        let user = create_user(&username, hash).await.expect("create user");

        // (c) no key set yet
        let before = get_claude_api_key(user.id).await.expect("get before save");
        assert_eq!(before, None, "no key should be set for a fresh user");

        // (a) save a key, read it back
        set_claude_api_key(user.id, "sk-ant-test-key")
            .await
            .expect("save key");
        let saved = get_claude_api_key(user.id).await.expect("get after save");
        assert_eq!(saved, Some("sk-ant-test-key".to_string()));

        // (b) delete it, read back
        clear_claude_api_key(user.id).await.expect("clear key");
        let after = get_claude_api_key(user.id).await.expect("get after clear");
        assert_eq!(after, None, "key should be cleared");
    }
}
