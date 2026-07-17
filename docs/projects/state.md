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

### Rendering
- Liquid template engine with access to device state, time, Prometheus queries (instant and time-range), and HTTP sources
- SVG → 1-bit BMP pipeline (usvg → resvg → tiny-skia → BMP encode)
- Custom Liquid filters: `qrcode`, `qrcode_wifi`
- Virtual device for previewing templates without physical hardware

### Web Dashboard
- Device management (list, assign templates, view logs)
- Template editor with live preview
- AI template generator (`/template/:id/generate`): chat with Claude to build or
  revise a template; Claude explores Prometheus/HTTP data with tools, renders
  proposals (and sees them as images), and the result saves in place. Runs
  entirely in the browser against `api.anthropic.com` with a per-user API key
  held by the server — see
  [completed/20260716-ai-template-generation](completed/20260716-ai-template-generation.md).
- Prometheus query configuration per template (instant and time-range queries)
- HTTP source configuration per template
- User management (including per-user Claude API key)
- Initial setup flow

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
