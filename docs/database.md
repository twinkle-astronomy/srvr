# Database

## Pool access

`db::get()` is a sync function returning `&'static SqlitePool`. Call it directly — no `.await`:

```rust
sqlx::query_as("SELECT id, name FROM templates WHERE id = ?")
    .bind(id)
    .fetch_optional(crate::db::get())  // sync, no .await
    .await?
```

All DB wrapper functions in `src/db.rs` are `async` — `.await` them:

```rust
let devices = crate::db::get_devices().await?;
```

## Query style

Always use runtime string form. Never use the compile-time `query_as!(...)` macro — `DATABASE_URL` is not configured at build time:

```rust
// CORRECT
sqlx::query_as("SELECT id, name, content, updated_at, created_at FROM templates WHERE id = ?")

// WRONG — macro requires DATABASE_URL at compile time
sqlx::query_as!("SELECT ...")
```

Always list columns explicitly — never `SELECT *`. Column order must match struct field declaration order:

```rust
// Column order must match Device struct field order
sqlx::query_as("SELECT id, friendly_id, access_token, mac_address, template_id, \
                last_seen_at, created_at FROM devices ORDER BY last_seen_at DESC")
    .fetch_all(crate::db::get())
    .await
```

## INSERT with RETURNING

```rust
let row = sqlx::query(
    "INSERT INTO templates (name, content, updated_at, created_at) \
     VALUES (?, ?, datetime('now'), datetime('now')) \
     RETURNING id, name, content, updated_at, created_at",
)
.bind(&name)
.bind(&content)
.fetch_one(crate::db::get())
.await?;
Template::from_row(&row)?
```

## Error handling

DB functions return `Result<T, sqlx::Error>`. When calling from a `#[server]` function, convert with `.to_string()`:

```rust
crate::db::get_templates().await
    .map_err(|e| ServerFnError::new(e.to_string()))
```

## Where to put new DB functions

All new CRUD functions go in `src/db.rs` and return `Result<T, sqlx::Error>`.
