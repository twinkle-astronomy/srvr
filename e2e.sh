#!/usr/bin/env bash
#
# Run just the browser end-to-end tests (tests/browser_e2e.rs).
#
# These also run as part of the normal `cargo test --features server`; this
# script is a convenience to run only that crate.
#
# WEBDRIVER_URL is required — without it every browser test fails (the tier is
# mandatory, not best-effort). The browser is the docker-compose `chrome`
# service (headless Chromium behind chromedriver). Bring it up and point the
# tests at it:
#     docker compose up -d chrome
#     WEBDRIVER_URL=http://chrome:4444 ./e2e.sh
# In the compose `srvr` service WEBDRIVER_URL is already set, so `./e2e.sh` works
# directly.

set -euo pipefail
cd "$(dirname "$0")"

command -v dx >/dev/null 2>&1 || { echo "✗ 'dx' (dioxus CLI) not found on PATH" >&2; exit 1; }

if [ -z "${WEBDRIVER_URL:-}" ]; then
    echo "✗ WEBDRIVER_URL is not set — start the compose 'chrome' service:" >&2
    echo "    docker compose up -d chrome" >&2
    echo "  then run inside the srvr container (WEBDRIVER_URL=http://chrome:4444)." >&2
    exit 1
fi

echo "▶ Building WASM bundle…"
dx build --platform web

echo "▶ Running browser E2E tests against ${WEBDRIVER_URL} …"
cargo test --features server --test browser_e2e -- --test-threads=1 "$@"
