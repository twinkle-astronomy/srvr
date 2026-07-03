# Setup & Running

## Commands

```bash
# ── Dev with hot reload (single command) ───────────────────────────────────
./dev.sh
# Starts the Axum API (:8080) + dx WASM hot-reload server (:3000) together.
# Open http://YOUR_IP:3000  ← note port 3000, not 8080.
# Ctrl-C stops both. Override defaults with env vars: DX_PORT, PORT, DX_ADDR.
#
# Why not just `dx serve`? dx's dev server only serves the WASM bundle + static
# assets; it can't run this app's custom Axum routes (/dashboard/*, /api/*,
# /render/*). So the Axum server runs as its own process and dx's web-mode proxy
# (see Dioxus.toml) forwards those paths to it. dev.sh wires both up.

# ── Simplest dev workflow (no hot reload, single server) ───────────────────
dx build --platform web    # WASM → target/dx/srvr/debug/web/public
PORT=8080 IMAGE_SIGNATURE_SECRET=dev cargo run --features server
# Axum auto-detects the dx build output and serves everything at :8080.

# ── Production ─────────────────────────────────────────────────────────────
# Build WASM (output: target/dx/srvr/release/web/public)
dx build --platform web --release
# Run server
IMAGE_SIGNATURE_SECRET=<secret> cargo run --release --features server

# ── Other ──────────────────────────────────────────────────────────────────
# Fast compile check
dx check

# Tests (browser E2E included — needs the compose `chrome` service up and
# WEBDRIVER_URL set; both are already true inside the compose srvr container)
cargo test --features server

# Docker dev container
docker-compose up -d
docker compose exec srvr bash
```

## Environment Variables

| Variable | Required | Default | Notes |
|---|---|---|---|
| `IMAGE_SIGNATURE_SECRET` | **YES — panics if missing** | — | HMAC key for `/render/screen.bmp` signing |
| `DATABASE_URL` | no | `sqlite:./data/devices.db` | SQLite path |
| `TZ` | no | `UTC` | Timezone for template rendering |
| `SERVER_HOST` | no | from Host header | Override host in image URLs (needed for dev) |
| `PROMETHEUS_URL` | no | `http://prometheus:9090` | Prometheus base URL |
| `RUST_LOG` | no | `info,tower_http=debug` | Log filter |

TLS (optional — omit for plain HTTP): `TLS_CERT_PATH`+`TLS_KEY_PATH` (manual PEM), or `ACME_DOMAIN`+`ACME_EMAIL`+`ACME_CACHE_DIR`+`ACME_STAGING` (Let's Encrypt).
