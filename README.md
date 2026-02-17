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
cargo watch -x run
```

The server will start on `http://localhost:8080`

## Development

### Modifying the Screen Content

Edit the `render_screen_handler()` function in `src/main.rs` to customize the SVG content. The SVG is rendered to a 1-bit bitmap suitable for e-ink displays.

### Environment Variables

- `RUST_LOG` - Set logging level (default: `info,tower_http=debug`)
  ```bash
  RUST_LOG=debug cargo run
  ```

## License

MIT

## Resources

- [TRMNL API Documentation](https://trmnl.com/api-docs/index.html)
- [TRMNL Official Website](https://trmnl.com)
