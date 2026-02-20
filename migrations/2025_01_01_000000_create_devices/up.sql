CREATE TABLE devices (
    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    mac_address TEXT NOT NULL UNIQUE,
    friendly_name TEXT,
    api_key TEXT NOT NULL,
    firmware_version TEXT,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_seen_at TIMESTAMP
);
