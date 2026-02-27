CREATE TABLE IF NOT EXISTS device_logs (
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

CREATE INDEX idx_device_logs_device_id ON device_logs(device_id);
