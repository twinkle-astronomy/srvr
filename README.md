# TRMNL eink Display API Server

A Rust-based API server that implements the [TRMNL eink display API](https://trmnl.com/api-docs/index.html) for serving content to TRMNL e-ink displays.

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
dx serve --addr 0.0.0.0
```

The server will start on `http://localhost:8080`

## Deployment

### Docker image

A docker image is available at https://github.com/twinkle-astronomy/srvr/pkgs/container/srvr

Use this docker-compose.yml to spin up a simple instance.
```yml
services:
  srvr:
    image: ghcr.io/twinkle-astronomy/srvr:0.0.10
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
