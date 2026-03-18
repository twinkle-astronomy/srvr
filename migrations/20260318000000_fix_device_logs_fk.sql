-- Fix broken FK in device_logs: the multi_template_support migration renamed
-- `devices` to `temp_devices` with foreign_keys ON, which rewrote device_logs'
-- FK to reference temp_devices. After temp_devices was dropped, the FK became
-- dangling. This migration recreates device_logs with the correct FK.

PRAGMA foreign_keys = OFF;

ALTER TABLE device_logs RENAME TO device_logs_backup;

DROP INDEX IF EXISTS idx_device_logs_device_id;

CREATE TABLE device_logs (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    device_id        INTEGER NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    device_log_id    INTEGER,
    battery_voltage  REAL,
    created_at       INTEGER,
    firmware_version TEXT,
    free_heap_size   INTEGER,
    max_alloc_size   INTEGER,
    message          TEXT,
    refresh_rate     INTEGER,
    sleep_duration   INTEGER,
    source_line      INTEGER,
    source_path      TEXT,
    special_function TEXT,
    wake_reason      TEXT,
    wifi_signal      INTEGER,
    wifi_status      TEXT,
    logged_at        DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

INSERT INTO device_logs (id, device_id, device_log_id, battery_voltage, created_at,
    firmware_version, free_heap_size, max_alloc_size, message, refresh_rate,
    sleep_duration, source_line, source_path, special_function, wake_reason,
    wifi_signal, wifi_status, logged_at)
SELECT id, device_id, device_log_id, battery_voltage, created_at,
    firmware_version, free_heap_size, max_alloc_size, message, refresh_rate,
    sleep_duration, source_line, source_path, special_function, wake_reason,
    wifi_signal, wifi_status, logged_at
FROM device_logs_backup;

DROP TABLE device_logs_backup;

CREATE INDEX idx_device_logs_device_id ON device_logs(device_id);

PRAGMA foreign_keys = ON;
