# TRMNL eink Display Server

A self-hosted TRMNL eink display backend. Devices poll `/api/display`, receive a signed image URL, then fetch `/render/screen.bmp` — a Liquid SVG template rendered to a 1-bit BMP.

**Stack:** Axum + Dioxus Fullstack (SSR + WASM) · SQLite via sqlx · Tokio · Liquid templates · resvg → 1-bit BMP · argon2 + axum-login · HMAC-SHA256 URL signing

---

## Build & Run

```bash
# Development with hot reload
SERVER_HOST=YOUR_IP:8080 dx serve --addr 0.0.0.0

# Fast compile check
dx check

# Tests
cargo test

# Docker dev container
docker-compose up -d
docker compose exec srvr bash
```

**Required env vars:**

| Variable | Required | Default | Notes |
|---|---|---|---|
| `IMAGE_SIGNATURE_SECRET` | **YES — panics if missing** | — | HMAC key for `/render/screen.bmp` signing |
| `DATABASE_URL` | no | `sqlite:./data/devices.db` | SQLite path |
| `TZ` | no | `UTC` | Timezone for template rendering |
| `SERVER_HOST` | no | from Host header | Override host in image URLs (needed for dev) |
| `PROMETHEUS_URL` | no | `http://prometheus:9090` | Prometheus base URL |
| `RUST_LOG` | no | `info,tower_http=debug` | Log filter |

TLS env vars (optional — omit for plain HTTP): `TLS_CERT_PATH`+`TLS_KEY_PATH` (manual PEM), or `ACME_DOMAIN`+`ACME_EMAIL`+`ACME_CACHE_DIR`+`ACME_STAGING` (Let's Encrypt).

---

## Module Map

```
src/
├── main.rs                  # Axum router; TLS mode dispatch; tracing setup
├── auth.rs                  # argon2 password verify/hash; axum-login Backend; auth routes
│                            # server_fn_auth_middleware: all POSTs require auth except
│                            #   paths containing "check_auth" or "check_needs_setup"
├── db.rs                    # SQLite OnceLock pool; async init(); sync get() pool accessor
│                            # All CRUD async functions live here
├── hmac.rs                  # generate_signature_bytes / validate_signature (60s window)
├── time.rs                  # Clock trait: RealClock (production); MockClock (cfg(test) only)
├── tls.rs                   # TlsMode enum; serve_manual_tls; serve_acme; HTTP redirect
├── models/
│   ├── mod.rs               # Shared types: Device, Template, User, PrometheusQuery,
│   │                        #   HttpSource, DeviceLog, DeviceLogEntry, RenderContext
│   └── server.rs            # Server-only impls: Template::render(), Device::get_render_obj(),
│                            #   HttpSource::get_render_obj(), json_to_liquid(), http_client()
├── device/
│   ├── mod.rs               # Error enum; header extraction helpers
│   ├── api.rs               # REST: GET /api/display, POST /api/log, GET /api/setup,
│   │                        #   GET /render/screen.bmp, SSE /api/devices/stream
│   ├── renderer.rs          # render_vars() → liquid::Object; render_screen() → Vec<u8> BMP
│   │                        #   svg_to_bmp(): usvg parse → resvg render → 1-bit BMP encode
│   └── liquid_filters.rs    # Custom Liquid filters: qrcode, qrcode_wifi
└── frontend/
    ├── mod.rs               # Dioxus App; Route enum (with layout guards)
    ├── server_fns.rs        # ~33 #[server] async functions (Dioxus RPC over POST)
    ├── store.rs             # AppStore: Dioxus Signals for devices/templates/users/auth
    ├── components/          # Nav and shared UI components
    └── pages/               # login, setup, dashboard, devices, templates,
                             #   template_editor/, users
```

---

## Detailed Docs

Topic-specific conventions and patterns are in [docs/](docs/):

| Topic | File |
|---|---|
| sqlx queries, `db::get()`, INSERT/RETURNING | [docs/database.md](docs/database.md) |
| `#[server]` functions, auth middleware, ServerFnError | [docs/server-functions.md](docs/server-functions.md) |
| Struct derives, feature flags, `models/server.rs` | [docs/models.md](docs/models.md) |
| Dioxus components, store, routing, adding pages | [docs/frontend.md](docs/frontend.md) |
| Migration naming, SQLite quirks | [docs/migrations.md](docs/migrations.md) |
| Inline tests, MockClock, tokio::test | [docs/testing.md](docs/testing.md) |
| Plan phase, TDD workflow | [docs/development-process.md](docs/development-process.md) |
| Current features, architecture, goals | [docs/projects/state.md](docs/projects/state.md) |
| Completed projects | [docs/projects/completed/](docs/projects/completed/) |
| Pending project ideas | [docs/projects/ideas/](docs/projects/ideas/) |

---

## Adding a New Feature (Typical Flow)

1. Add struct to `src/models/mod.rs` with appropriate derives (see [docs/models.md](docs/models.md))
2. Add migration: `migrations/YYYYMMDDHHMMSS_description.sql` (continue `20260...` prefix — see [docs/migrations.md](docs/migrations.md))
3. Add async CRUD functions to `src/db.rs` returning `Result<T, sqlx::Error>`
4. Add `#[server]` functions to `src/frontend/server_fns.rs`
5. Create page: `src/frontend/pages/mypage.rs`
6. Register in `src/frontend/pages/mod.rs`: `mod mypage; pub use mypage::MyPage;`
7. Register route in `src/frontend/mod.rs`: add to `use pages::` import and to `Route` enum

---

## Liquid Template Variables

Templates are SVG files with Liquid syntax. Available variables:

```
device.width, device.height, device.friendly_id, device.mac_address
device.battery_voltage, device.battery_percent_charged, device.rssi, device.fw_version
time (HH:MM AM/PM), date (YYYY-MM-DD), timezone (e.g. PST)
prometheus.<name>[i].value, prometheus.<name>[i].labels.<key>
http.<source_name>.<json.path>
```

Custom filters: `{{ val | qrcode }}`, `{{ val | qrcode: module_size: 3 }}`,
`{{ ssid | qrcode_wifi: password: "pw" }}`, `{{ ssid | qrcode_wifi: password: "pw", security: "WEP" }}`
