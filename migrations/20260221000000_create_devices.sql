CREATE TABLE IF NOT EXISTS devices (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    mac_address     TEXT    NOT NULL UNIQUE,
    model           TEXT    NOT NULL,
    access_token    TEXT    NOT NULL UNIQUE,
    friendly_id     TEXT    NOT NULL,
    width           INTEGER NOT NULL,
    height          INTEGER NOT NULL,
    fw_version      TEXT,
    battery_voltage FLOAT,
    rssi            TEXT,
    last_seen_at    TEXT    NOT NULL DEFAULT CURRENT_TIMESTAMP,
    created_at      DATETIME    NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at      DATETIME    NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_devices_access_token ON devices(access_token);
CREATE INDEX idx_devices_mac_address ON devices(mac_address);
