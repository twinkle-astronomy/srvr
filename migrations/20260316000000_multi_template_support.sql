-- Add name column to templates
ALTER TABLE templates ADD COLUMN name TEXT NOT NULL DEFAULT 'Default';

-- Recreate devices table with template_id NOT NULL FK
ALTER TABLE devices RENAME TO temp_devices;

DROP INDEX IF EXISTS idx_devices_access_token;
DROP INDEX IF EXISTS idx_devices_mac_address;

CREATE TABLE IF NOT EXISTS devices (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    mac_address     TEXT    NOT NULL UNIQUE,
    model           TEXT    NOT NULL,
    access_token    TEXT    NOT NULL UNIQUE,
    friendly_id     TEXT    NOT NULL,
    width           INTEGER,
    height          INTEGER,
    fw_version      TEXT,
    battery_voltage FLOAT,
    rssi            TEXT,
    template_id     INTEGER NOT NULL REFERENCES templates(id),
    last_seen_at    TEXT    NOT NULL DEFAULT CURRENT_TIMESTAMP,
    created_at      DATETIME    NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at      DATETIME    NOT NULL DEFAULT CURRENT_TIMESTAMP
);

INSERT INTO devices (id, mac_address, model, access_token, friendly_id, width, height, fw_version, battery_voltage, rssi, template_id, last_seen_at, created_at, updated_at)
SELECT id, mac_address, model, access_token, friendly_id, width, height, fw_version, battery_voltage, rssi,
       (SELECT id FROM templates ORDER BY id ASC LIMIT 1),
       last_seen_at, created_at, updated_at
FROM temp_devices;

CREATE INDEX idx_devices_access_token ON devices(access_token);
CREATE INDEX idx_devices_mac_address ON devices(mac_address);
CREATE INDEX idx_devices_template_id ON devices(template_id);

DROP TABLE temp_devices;
