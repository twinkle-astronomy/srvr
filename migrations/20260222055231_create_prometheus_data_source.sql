-- Add migration script here
CREATE TABLE IF NOT EXISTS prometheus_queries (
    id              INTEGER  PRIMARY KEY AUTOINCREMENT,
    template_id     INTEGER  REFERENCES templates(id),
    name            TEXT     NOT NULL,
    addr            TEXT     NOT NULL,
    query           TEXT     NOT NULL,
    created_at      DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at      DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE        INDEX IF NOT EXISTS prometheus_queries_template_id      ON prometheus_queries(template_id);
CREATE UNIQUE INDEX IF NOT EXISTS prometheus_queries_name_template_id ON prometheus_queries(name, template_id);
