#!/usr/bin/env bash
#
# Single-command dev workflow with hot reload on both sides.
#
# `dx serve` alone cannot serve this app's API: dx's dev server only knows how
# to serve the WASM bundle + static assets (and, in fullstack mode, dioxus
# `#[server]` functions). Our backend is a hand-rolled Axum server with custom
# routes (/dashboard/*, /api/*, /render/*), so it must run as its own process.
# dx's web-mode proxy (see Dioxus.toml) forwards those paths to it.
#
# This script runs both as one command:
#   - Axum server (API) on :8080  — background, rebuilt+restarted on change by
#     cargo-watch (falls back to a one-shot `cargo run` if cargo-watch is absent)
#   - dx serve (WASM + hot reload) on :3000 — foreground, proxies API to :8080
#
# Hot reload coverage:
#   - Frontend RSX / logic / Tailwind CSS  → dx hot-reloads the browser
#   - Server code (src/**, Cargo.toml) and compiled-in templates (assets/*.liquid)
#     → cargo-watch rebuilds + restarts the Axum server
#
# Open http://<host>:3000 in a browser. Ctrl-C stops both.

set -euo pipefail

export IMAGE_SIGNATURE_SECRET="${IMAGE_SIGNATURE_SECRET:-dev}"
export DATABASE_URL="${DATABASE_URL:-sqlite:./data/devices.db}"
export PORT="${PORT:-8080}"

DX_PORT="${DX_PORT:-3000}"
DX_ADDR="${DX_ADDR:-0.0.0.0}"

mkdir -p data

if command -v cargo-watch >/dev/null 2>&1; then
    echo "▶ Starting Axum API on :${PORT} with cargo-watch (server hot reload) ..."
    # Watch source, manifest, and compiled-in templates; ignore CSS (dx owns it)
    # and the data/ dir (SQLite writes there — watching it would loop forever,
    # but it's outside the --watch paths anyway).
    cargo watch \
        --watch src \
        --watch Cargo.toml \
        --watch assets \
        --ignore '*.css' \
        -x 'run --features server' &
else
    echo "▶ cargo-watch not found — starting Axum API on :${PORT} without server hot reload."
    echo "  Install it for server hot reload:  cargo binstall cargo-watch"
    cargo run --features server &
fi
SERVER_PID=$!

cleanup() {
    echo ""
    echo "▶ Shutting down ..."
    kill "$SERVER_PID" 2>/dev/null || true
    wait "$SERVER_PID" 2>/dev/null || true
    # Safety net: cargo-watch's grandchild server binary can outlive the watcher.
    pkill -f 'target/debug/srvr' 2>/dev/null || true
}
trap cleanup EXIT INT TERM

# Wait for the API to accept connections before starting dx (so the first
# proxied request doesn't race the server's startup).
echo "▶ Waiting for API to come up ..."
for _ in $(seq 1 60); do
    if curl -sf "http://127.0.0.1:${PORT}/dashboard/needs-setup" >/dev/null 2>&1; then
        echo "▶ API is up."
        break
    fi
    sleep 1
done

echo "▶ Starting dx serve on ${DX_ADDR}:${DX_PORT} (open http://<host>:${DX_PORT})"
dx serve --addr "${DX_ADDR}" --port "${DX_PORT}"
