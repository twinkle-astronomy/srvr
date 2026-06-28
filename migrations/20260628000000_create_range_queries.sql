CREATE TABLE IF NOT EXISTS range_queries (
    id              INTEGER  PRIMARY KEY AUTOINCREMENT,
    template_id     INTEGER  REFERENCES templates(id),
    name            TEXT     NOT NULL,
    addr            TEXT     NOT NULL,
    query           TEXT     NOT NULL,
    duration        TEXT     NOT NULL,
    step            TEXT     NOT NULL,
    created_at      DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at      DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE        INDEX IF NOT EXISTS range_queries_template_id      ON range_queries(template_id);
CREATE UNIQUE INDEX IF NOT EXISTS range_queries_name_template_id ON range_queries(name, template_id);
