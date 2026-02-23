# TRMNL eink Display API Server

A Rust-based API server that implements the [TRMNL eink display API](https://trmnl.com/api-docs/index.html) for serving content to 800x480 e-ink displays.

## Features

- **Device API endpoints** - Full implementation of TRMNL device communication protocol
- **SVG to 1-bit BMP rendering** - Converts SVG graphics to monochrome bitmap images optimized for e-ink displays
- **Real-time rendering** - Generates screen images on-demand
- **Docker support** - Containerized development and deployment

## Quick Start

### Prerequisites

- Docker and Docker Compose

### Running with Docker

```bash
docker-compose up -d
docker compose exec -it srvr /bin/bash

```

### Running Locally with Hot Reload

For development with automatic reloading on file changes:

```bash
dx serve --addr 0.0.0.0
```

The server will start on `http://localhost:8080`

## Development

### Environment Variables

- `RUST_LOG` - Set logging level (default: `info,tower_http=debug`)
  ```bash
  RUST_LOG=debug cargo run
  ```

## Deployment

### Docker image

A docker image is available at https://github.com/twinkle-astronomy/srvr/pkgs/container/srvr

To save state the system will create a sqlite database at /data.  To persist it between runs use a volume mount.

```yml
services:
  srvr:
    image: ghcr.io/twinkle-astronomy/srvr:main
    volumes:
      - srvr-data:/data
    init: true
    ports:
      - "80:8080"
    environment:
      - DATABASE_URL=sqlite:///data/data.db


volumes:
  srvr-data:
```

The service will be available at the machine's IP, port 80.


## License

MIT

## Resources

- [TRMNL API Documentation](https://trmnl.com/api-docs/index.html)
- [TRMNL Official Website](https://trmnl.com)
