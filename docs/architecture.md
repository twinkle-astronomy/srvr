# Architecture & Module Map

```
src/
├── main.rs                  # Axum router; TLS mode dispatch; tracing setup
│                            # Serves the CSR WASM bundle via ServeDir (DIOXUS_ASSET_DIR
│                            #   in dev, ./dist in prod) with an index.html fallback
│                            #   so the SPA router handles all non-API paths
├── auth.rs                  # argon2 password verify/hash; axum-login Backend;
│                            #   form-style auth routes at /auth/*
├── api/                     # Dashboard JSON API under /dashboard/ (server-only)
│   ├── mod.rs               # Router assembly; ApiError (IntoResponse + From<sqlx::Error>);
│   │                        #   require_auth() — auth is per-handler, not middleware
│   ├── auth.rs              # check_auth, needs-setup, server-info, JSON auth endpoints
│   ├── devices.rs           # device CRUD, logs, render contexts
│   ├── templates.rs         # template CRUD, previews, template context/vars
│   ├── users.rs             # list/delete users
│   ├── prometheus.rs        # instant-query config + execution
│   ├── range.rs             # range-query config + execution
│   └── http_sources.rs      # HTTP source config + execution
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
    ├── api.rs               # web-feature fetch layer: gloo-net helpers, one fn per
    │                        #   /dashboard endpoint, all returning ServerFnError
    ├── server_fns.rs        # Shared types (ServerInfo, TemplateVar) + ServerFnError shim.
    │                        #   web build: re-exports the fetch fns from api.rs;
    │                        #   server build: direct-to-db impls so the native
    │                        #   component-test tier compiles and renders
    ├── store.rs             # AppStore: Dioxus Signals for devices/templates/users/auth
    ├── components/          # Nav and shared UI components
    └── pages/               # login, setup, dashboard, devices, templates,
                             #   template_editor/, users
tests/
└── browser_e2e.rs           # fantoccini-driven E2E: real WASM app in headless Chromium
                             #   against a spawned srvr (skips unless WEBDRIVER_URL set)
```

## Feature Flags

- `#[cfg(feature = "server")]` — gates server-only code (db, axum, TLS, rendering)
- `#[cfg(feature = "web")]` — gates WASM/browser-only code (EventSource, wasm_bindgen)
- Server-only impls for shared model types go in `src/models/server.rs`, not `mod.rs`
