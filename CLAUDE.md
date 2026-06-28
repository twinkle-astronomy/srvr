# TRMNL eink Display Server

A self-hosted TRMNL eink display backend. Devices poll `/api/display`, receive a signed image URL, then fetch `/render/screen.bmp` — a Liquid SVG template rendered to a 1-bit BMP.

**Stack:** Axum + Dioxus Fullstack (SSR + WASM) · SQLite via sqlx · Tokio · Liquid templates · resvg → 1-bit BMP · argon2 + axum-login · HMAC-SHA256 URL signing

## Docs

| Topic | File |
|---|---|
| Build, run, env vars | [docs/setup.md](docs/setup.md) |
| Module map, feature flags | [docs/architecture.md](docs/architecture.md) |
| sqlx queries, `db::get()`, INSERT/RETURNING | [docs/database.md](docs/database.md) |
| `#[server]` functions, auth middleware, ServerFnError | [docs/server-functions.md](docs/server-functions.md) |
| Struct derives, feature flags, `models/server.rs` | [docs/models.md](docs/models.md) |
| Dioxus components, store, routing, adding pages | [docs/frontend.md](docs/frontend.md) |
| Migration naming, SQLite quirks | [docs/migrations.md](docs/migrations.md) |
| Liquid template variables and filters | [docs/templates.md](docs/templates.md) |
| Inline tests, MockClock, tokio::test | [docs/testing.md](docs/testing.md) |
| New feature flow, plan phase, TDD workflow | [docs/development-process.md](docs/development-process.md) |
| Current features, architecture, goals | [docs/projects/state.md](docs/projects/state.md) |
| Completed projects | [docs/projects/completed/](docs/projects/completed/) |
| Pending project ideas | [docs/projects/ideas/](docs/projects/ideas/) |
