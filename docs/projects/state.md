# Project State

## What this is

A self-hosted backend for [TRMNL](https://trmnl.com) e-ink displays. Physical devices register themselves, poll for screens, and display rendered images. A web dashboard lets you manage devices, build templates, and configure data sources.

## Goals

- Full compatibility with the TRMNL device protocol
- Self-hostable with minimal ops overhead (single binary, SQLite, Docker)
- Flexible templating: pull data from Prometheus or any HTTP JSON endpoint
- Clean, maintainable Rust codebase that's easy to extend

## Features

### Device API
- Device self-registration (`GET /api/setup`)
- Screen polling (`GET /api/display`) — returns a signed image URL
- Telemetry logging (`POST /api/log`) — battery, WiFi signal, heap, firmware version
- Real-time device and log streams via SSE
- HMAC-SHA256 URL signing with 60-second expiry for image fetch security
- Firmware OTA updates: `/api/display` tells opted-in devices to update
  (`update_firmware`/`firmware_url`) when their reported `fw_version`
  doesn't match the active release for their `model`; the binary is served
  from the signed `GET /firmware/download` route — see
  [completed/20260719-firmware-ota-updates](completed/20260719-firmware-ota-updates.md).
- Setup/log request debugging: `POST /api/log` logs each submitted log
  entry's content, and `GET /api/setup` logs all parsed device fields (not
  just raw headers), so an operator can see what a device sent without DB
  access. `GET /api/setup/` (trailing slash) is also accepted. A device that
  omits `model`/`Width`/`Height` (some real-world firmware sends only `ID` +
  `FW-Version`) still completes setup — those fields default rather than
  failing the registration, and a later poll missing the same headers won't
  overwrite already-known values — see
  [completed/20260730-device-request-logging](completed/20260730-device-request-logging.md).

### Rendering
- Liquid template engine with access to device state, time, Prometheus queries (instant and time-range), and HTTP sources
- SVG → 1-bit BMP pipeline
- SVG → 2-bit (4-level) grayscale PNG pipeline for devices with the
  `supports_2bit_grayscale` flag set: `GET /api/display` points them at
  `GET /render/screen_2bit.png` instead of `/render/screen.bmp`, and the
  response's `bitdepth` field reflects which. Same signed-URL gate as the BMP
  route. The PNG is a genuine 2-bit-depth file (packed via the `png` crate
  directly — `image`'s own encoder can't write below 8bpc), not an 8-bit PNG
  that merely uses 4 gray values. Per-device toggle: `POST
  /dashboard/devices/{id}/grayscale`. See
  [completed/20260823-2-bit-grayscale-support](completed/20260823-2-bit-grayscale-support.md).
- Custom Liquid filters: `qrcode`, `qrcode_wifi`
- Virtual device for previewing templates without physical hardware

### Web Dashboard
- Device management (list, assign templates, view logs)
- Template editor with live preview
- AI template generator (`/template/:id/generate`): chat with Claude to build or
  revise a template; Claude explores Prometheus/HTTP data with tools, renders
  proposals (and sees them as images), and the result saves in place. The agent
  loop runs in the browser; requests to Claude are proxied through the server,
  which holds each user's API key (the key never reaches the browser) — see
  [completed/20260716-ai-template-generation](completed/20260716-ai-template-generation.md).
- Prometheus query configuration per template (instant and time-range queries)
- HTTP source configuration per template
- User management (including per-user Claude API key)
- Initial setup flow
- Firmware release management (`/firmware`): upload `.bin` releases tagged
  by device `model` (version auto-detected from the binary's embedded
  ESP-IDF app descriptor, manually editable), activate one release per
  model (rollback = activate an older one), delete inactive releases.
  Per-device opt-in toggle (default off) on the device detail page.

### Infrastructure
- Native frontend test harness (`src/frontend/test_harness.rs`): renders Dioxus
  components through the real dioxus-core runtime and asserts on SSR output, run
  under `cargo test --features server` (no browser) — see
  [completed/20260628-frontend-test-harness](completed/20260628-frontend-test-harness.md).
- Browser E2E tier (`tests/browser_e2e.rs`): drives the real WASM dashboard in
  headless Chromium (docker-compose `chrome` service) against a spawned server,
  covering login, user management, password change, and device/template editing —
  see [completed/20260702-csr-rpc-conversion](completed/20260702-csr-rpc-conversion.md).
- SQLite with WAL mode; schema managed via sqlx migrations
- Session-based auth with Argon2 password hashing
- Optional TLS: manual PEM certs or Let's Encrypt ACME
- Prometheus metrics endpoint (`/metrics`)
- Docker image with multi-arch builds (amd64, arm64, armv7)

## Architecture patterns

- **Axum + Dioxus CSR**: the browser renders the dashboard from scratch (no SSR/hydration); Axum serves the WASM bundle and a plain JSON API under `/dashboard/` as the data layer
- **Global AppStore**: Dioxus Signals shared across the frontend via context
- **Feature-gated compilation**: `server` feature for backend code, `web` feature for WASM code
- **OnceLock pool**: single SQLite connection pool initialized at startup, accessed via sync `db::get()`
- **Clock trait**: time abstraction (`RealClock` / `MockClock`) to keep time-sensitive code testable
