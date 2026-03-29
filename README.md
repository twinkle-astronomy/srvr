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


volumes:
  srvr-data:
```

The service will be available at the machine's IP, port 80.


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
