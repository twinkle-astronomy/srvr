CREATE TABLE firmware_releases (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    model       TEXT NOT NULL,
    version     TEXT NOT NULL,
    filename    TEXT NOT NULL,
    size_bytes  INTEGER NOT NULL,
    binary      BLOB NOT NULL,
    active      INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE UNIQUE INDEX idx_firmware_releases_model_version ON firmware_releases (model, version);
-- enforces "at most one active release per model" at the DB level
CREATE UNIQUE INDEX idx_firmware_releases_one_active_per_model ON firmware_releases (model) WHERE active = 1;
