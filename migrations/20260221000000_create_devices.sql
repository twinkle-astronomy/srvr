CREATE TABLE IF NOT EXISTS devices (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    mac_address     TEXT    NOT NULL UNIQUE,
    model           TEXT    NOT NULL,
    access_token    TEXT    NOT NULL UNIQUE,
    friendly_id     TEXT    NOT NULL,
    width           INTEGER,
    height          INTEGER,
    fw_version      TEXT,
    battery_voltage TEXT,
    rssi            TEXT,
    last_seen_at    TEXT    NOT NULL DEFAULT (datetime('now')),
    created_at      TEXT    NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_devices_access_token ON devices(access_token);
CREATE INDEX idx_devices_mac_address ON devices(mac_address);
