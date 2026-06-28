# Setup & Running

## Commands

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
