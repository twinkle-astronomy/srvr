CREATE TABLE IF NOT EXISTS http_sources (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    template_id INTEGER REFERENCES templates(id),
    name        TEXT NOT NULL,
    url         TEXT NOT NULL,
    created_at  DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at  DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS http_sources_template_id ON http_sources(template_id);
CREATE UNIQUE INDEX IF NOT EXISTS http_sources_name_template_id ON http_sources(name, template_id);
