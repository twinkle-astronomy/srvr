# Migrations

## Filename format

```
migrations/YYYYMMDDHHMMSS_description.sql
```

Migrations run in lexicographic order by filename. This repo uses `20260...` timestamps — continue that sequence:

```
20260328000000_create_http_sources.sql   ← last existing
20260401000000_add_my_table.sql          ← new one
```

## SQLite quirks

- `ADD COLUMN ... NOT NULL` requires a `DEFAULT` value (SQLite limitation)
- To drop or rename a column, use table-rename + recreate (see `20260316000000_multi_template_support.sql` for the pattern)
- Always add indexes on foreign keys and frequently-queried columns:
  ```sql
  CREATE INDEX idx_things_device_id ON things (device_id);
  ```
- Foreign keys are enabled via pragma at connection time (`PRAGMA foreign_keys = ON`) — use `ON DELETE CASCADE` where appropriate

## Example

```sql
CREATE TABLE IF NOT EXISTS things (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    device_id   INTEGER NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    created_at  DATETIME NOT NULL DEFAULT (datetime('now')),
    updated_at  DATETIME NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_things_device_id ON things (device_id);
```
