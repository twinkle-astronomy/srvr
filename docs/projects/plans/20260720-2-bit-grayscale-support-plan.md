# Plan: 2‑bit Grayscale Support for TRMNL Display Server

**Goal** – Serve a 2‑bit (four‑level) grayscale image when the device indicates it can handle it, while preserving the existing 1‑bit BMP path for legacy devices.

## Overview
The server currently renders a Liquid SVG template to a 1‑bit BMP (`/render/screen.bmp`). To support 2‑bit grayscale we will:
1. Detect device capability via a request header or query parameter.
2. Render the SVG to an intermediate PNG (800 × 480).
3. Convert that PNG to a true 2‑bit image using Rust’s `image` crate.
4. Sign and serve the resulting PNG URL, falling back to BMP for devices without the capability flag.

## Detailed Steps
### 1️⃣ Device Capability Detection
- Extend `/api/display` request handling (`src/api/display.rs`) to read a new header `X‑TRMNL‑BITDEPTH`. Accept values `1` (default) or `2`.
- Add a query param fallback `?bitdepth=2` for devices that cannot set custom headers.
- Propagate the detected depth through the request context (`RequestContext`) to the rendering pipeline.

### 2️⃣ Render SVG → PNG (intermediate)
- Re‑use existing Liquid template rendering logic (`src/render/template.rs`).
- Instead of directly converting to BMP, output a PNG buffer using `resvg` with `png::Encoder`.
- Store the PNG in memory; no file write needed at this stage.

### 3️⃣ Convert PNG → 2‑bit PNG (Rust)
- Add a new module `src/render/grayscale.rs` containing the conversion function from the idea document:
```rust
use image::{DynamicImage, GenericImageView, ImageBuffer, Luma};

pub fn convert_to_2bit(img: DynamicImage) -> ImageBuffer<Luma<u8>, Vec<u8>> {
    let (w, h) = img.dimensions();
    let mut out = ImageBuffer::new(w, h);
    for (x, y, pixel) in img.pixels() {
        let gray = pixel.to_luma().0[0];
        let level = match gray {
            0..=63 => 0,
            64..=127 => 85,
            128..=191 => 170,
            _ => 255,
        };
        out.put_pixel(x, y, Luma([level]));
    }
    out
}
```
- Call this function only when the detected bit depth is `2`.
- Encode the resulting buffer back to PNG (`image::codecs::png::PngEncoder`). The output will contain exactly four gray levels and therefore be a true 2‑bit image.

### 4️⃣ Signing & URL Generation
- Extend the existing HMAC signing helper (`src/api/sign.rs`) to accept an optional `extension` argument (e.g., `.png` vs `.bmp`).
- When bit depth is `2`, generate a signed URL pointing at `/render/screen.png`; otherwise keep the current BMP flow.
- Update the JSON response schema in `docs/api.md` to include `image_url: String` and an optional `bitdepth: u8` field for clarity.

### 5️⃣ Fallback & Backward Compatibility
- Devices that do not send the header will continue receiving the signed BMP URL unchanged.
- Add a feature flag (`grayscale_support`) in `Cargo.toml`/`features.rs` so the entire pipeline can be toggled off if needed for debugging.

### 6️⃣ Tests
#### Unit tests (src/tests/grayscale.rs)
- Verify `convert_to_2bit` maps pixel ranges correctly.
- Test that a full‑white PNG becomes all `255`, and a gradient yields only the four expected levels.
#### Integration test (src/tests/api_grayscale.rs)
- Spin up an in‑memory Axum server, send `/api/display` with `X‑TRMNL‑BITDEPTH: 2`.
- Assert the JSON response contains a signed `.png` URL and that fetching it returns a PNG whose `bit-depth` is reported as `2` (use `image::io::Reader`).
- Repeat without the header to ensure BMP path remains unchanged.
#### End‑to‑end verification
- Use the `/verify` skill after implementation to run a headless browser, load the dashboard, and confirm the image URL changes based on the header.

### 7️⃣ Documentation Updates
- **docs/templates.md** – Add a note about grayscale palettes (`#000000 #555555 #aaaaaa #ffffff`).
- **docs/api.md** – Document the new request header/query param and updated response fields.
- **docs/projects/ideas/2-bit-grayscale-support.md** – Mark as completed (or move to `completed` folder) once the plan is merged.

## Milestones & Timeline
| Milestone | Owner | ETA |
|-----------|-------|-----|
| Header detection & request context | backend team | 2026‑07‑24 |
| PNG rendering pipeline refactor | render team | 2026‑07‑28 |
| `image` crate conversion module | graphics subteam | 2026‑08‑02 |
| Signing & URL changes | security team | 2026‑08‑05 |
| Unit + integration tests | QA | 2026‑08‑07 |
| Documentation & final review | docs lead | 2026‑08‑09 |
| Merge to `main` | release manager | 2026‑08‑12 |

---
*This plan is stored under `docs/projects/plans/`. Subsequent commits will flesh out each milestone with code changes and test implementations.*