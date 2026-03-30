# TRMNL eink Display API Server

A Rust-based API server that implements the [TRMNL eink display API](https://trmnl.com/api-docs/index.html) for serving content to [TRMNL](https://trmnl.com/) e-ink displays.

## Features

- **Device API endpoints** - Full implementation of TRMNL device communication protocol
- **SVG to 1-bit BMP rendering** - Converts SVG graphics to monochrome bitmap images optimized for e-ink displays
- **Real-time rendering** - Generates screen images on-demand
- **Docker support** - Containerized development and deployment

## Prerequisites

- Docker and Docker Compose

## Development

### Running with Docker

```bash
docker-compose up -d
docker compose exec -it srvr /bin/bash

```

### Running Locally with Hot Reload

For development with automatic reloading on file changes:

```bash
SERVER_HOST=YOUR_IP:8080 dx serve --addr 0.0.0.0
```

The server will start on `http://localhost:8080`

## Deployment

### Docker image

A docker image is available at https://github.com/twinkle-astronomy/srvr/pkgs/container/srvr

Use this docker-compose.yml to spin up a simple instance.
```yml
services:
  srvr:
    image: ghcr.io/twinkle-astronomy/srvr:0.1.2
    volumes:
      - srvr-data:/data
    init: true
    ports:
      - "80:8080"
    environment:
      - PROMETHEUS_URL=http://prometheus:9090
      - DATABASE_URL=sqlite:///data/data.db
      - TZ=America/Los_Angeles
      ## Manual TLS — provide cert and key files:
      # - TLS_CERT_PATH=/certs/fullchain.pem
      # - TLS_KEY_PATH=/certs/privkey.pem
      ## ACME / Let's Encrypt — automatic certificate management:
      # - ACME_DOMAIN=example.com
      # - ACME_EMAIL=admin@example.com
      # - ACME_CACHE_DIR=/data/acme
      # - ACME_STAGING=true
      ## Shared TLS settings:
      # - HTTPS_PORT=443

volumes:
  srvr-data:
```

The service will be available at the machine's IP, port 80.


## TLS

By default the server runs over plain HTTP. Two modes are available for HTTPS.

### Manual certificates

Set `TLS_CERT_PATH` and `TLS_KEY_PATH` to the paths of your PEM certificate and key files. The server will listen for HTTPS on port 443 (override with `HTTPS_PORT`) and redirect HTTP traffic on port 8080 (override with `PORT`) to HTTPS.

```yml
environment:
  - TLS_CERT_PATH=/certs/fullchain.pem
  - TLS_KEY_PATH=/certs/privkey.pem
```

### Automatic certificates via Let's Encrypt (ACME)

Set `ACME_DOMAIN` to your domain name (comma-separated for multiple). The server will obtain and renew certificates automatically using the TLS-ALPN-01 challenge.

```yml
environment:
  - ACME_DOMAIN=example.com
  - ACME_EMAIL=admin@example.com   # optional, recommended
  - ACME_CACHE_DIR=/data/acme      # optional, default: .data/acme
```

Set `ACME_STAGING=true` to use the Let's Encrypt staging environment while testing.

> **Note:** ACME requires ports 80 and 443 to be publicly reachable on the configured domain.

## Template Filters

In addition to the standard [Liquid filters](https://shopify.github.io/liquid/filters/), the following custom filters are available in templates:

### `qrcode`

Renders a string as a QR code inline SVG fragment.

```liquid
{{ "https://example.com" | qrcode }}
{{ some_var | qrcode: module_size: 3 }}
```

| Parameter | Required | Description |
|-----------|----------|-------------|
| `module_size` | optional | Pixel size of each QR module (default: 1) |

### `qrcode_wifi`

Renders WiFi credentials as a QR code inline SVG fragment. The input value is the SSID.

```liquid
{{ "MyNetwork" | qrcode_wifi: password: "secret" }}
{{ "MyNetwork" | qrcode_wifi: password: "secret", security: "WEP", module_size: 3 }}
{{ "OpenNet" | qrcode_wifi: password: "", security: "nopass" }}
```

| Parameter | Required | Description |
|-----------|----------|-------------|
| `password` | required | WiFi password |
| `security` | optional | Security type: `WPA` (default), `WEP`, or `nopass` |
| `module_size` | optional | Pixel size of each QR module (default: 1) |

## License

MIT

## Resources

- [TRMNL API Documentation](https://trmnl.com/api-docs/index.html)
- [TRMNL Official Website](https://trmnl.com)
