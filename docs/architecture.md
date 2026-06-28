# Architecture & Module Map

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

## Feature Flags

- `#[cfg(feature = "server")]` — gates server-only code (db, axum, TLS, rendering)
- `#[cfg(feature = "web")]` — gates WASM/browser-only code (EventSource, wasm_bindgen)
- Server-only impls for shared model types go in `src/models/server.rs`, not `mod.rs`
