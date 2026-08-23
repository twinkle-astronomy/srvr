# Plan: 2‑bit Grayscale Support for TRMNL Display Server

**Goal** – Serve a 2‑bit (four‑level) grayscale image when the device indicates it can handle it, while preserving the existing 1‑bit BMP path for legacy devices.

## Overview
The server serves an image in two separate requests, not one:
1. The device polls `GET /api/display` (`display_handler` in `src/device/api.rs`). This handler does **not** render anything — it looks up the device, builds an HMAC‑signed URL pointing at `/render/screen.bmp`, and returns that URL in the JSON response (`image_url`).
2. The device then fetches that signed URL — `GET /render/screen.bmp` (`render_screen_handler`, same file). This is where rendering actually happens: it loads the device's `RenderContext` (`render_context_for_device`), renders the Liquid template to SVG, and rasterizes it to a 1‑bit BMP via `renderer::render_screen` (`src/device/renderer.rs`).

To support 2‑bit grayscale we will:
1. Detect device capability via a **device‑model flag** (`supports_2bit_grayscale`) stored in the database.
2. Have `display_handler` embed a URL to a new render route (e.g. `/render/screen_2bit.png`) instead of `/render/screen.bmp` when the flag is set — it still only builds a URL; no rendering happens in this handler.
3. Add a new handler for that route, alongside `render_screen_handler`, that renders the SVG and converts it to a true 2‑bit image using Rust's `image` crate.
4. Fall back to the existing 1‑bit BMP path for devices without the capability flag.

## Detailed Steps
### 1️⃣ Device Capability Detection
- Extend `display_handler` (`src/device/api.rs`) to look up a **device‑model flag** indicating grayscale support. Add a new column `supports_2bit_grayscale BOOLEAN` (default `false`) to the `devices` table.
- The `Device` row returned by `get_and_update_device_from_headers` already carries this once the column exists — no separate request-context type is needed. (There is no `RequestContext` in this codebase; the render-time equivalent is `RenderContext`, built by `render_context_for_device` when `/render/screen.bmp` is actually fetched.)
- Devices with `supports_2bit_grayscale = true` will have `display_handler` embed a link to the 2‑bit render route; all others fall back to the existing `/render/screen.bmp` URL.


### 2️⃣ Render SVG → PNG (intermediate)
- Re‑use existing Liquid template rendering logic in `src/device/renderer.rs` (`render_vars`, and the SVG rasterization already shared by `render_screen`/`render_screen_png`).
- Instead of thresholding straight to black/white (as `svg_to_bilevel` does today), rasterize to grayscale and output a PNG buffer.
- Store the PNG in memory; no file write needed at this stage — this mirrors how `render_screen_handler` already works, returning bytes directly in the HTTP response.

### 3️⃣ Convert PNG → 2‑bit PNG (Rust)
- Add a new module `src/device/grayscale.rs` (there is no `src/render/` directory — rendering code lives under `src/device/`, alongside `renderer.rs`) containing the conversion function from the idea document:
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
- No changes needed to the signing helper itself (`src/hmac.rs`, not `src/api/sign.rs`): `generate_signature_bytes`/`validate_signature` are scoped to `device_id` + timestamp only, independent of which path the URL points at — the same signature works for `/render/screen.bmp` and a new `/render/screen_2bit.png` route.
- `display_handler` just needs to pick which path/extension to format into `image_url` based on `device.supports_2bit_grayscale`, reusing the signature bytes it already generates.
- Update the JSON response schema in `docs/api.md` to include `image_url: String` and an optional `bitdepth: u8` field for clarity.

### 5️⃣ Fallback & Backward Compatibility
- Devices whose `supports_2bit_grayscale` flag is **false** will continue receiving the signed BMP URL unchanged.

### 6️⃣ Tests
Tests are inline `#[cfg(test)] mod tests` blocks in the same file as the code under test (see [docs/testing.md](../../testing.md)) — there is no `src/tests/` directory in this codebase.
#### Unit tests (inline in `src/device/grayscale.rs`)
- Verify `convert_to_2bit` maps pixel ranges correctly.
- Test that a full‑white PNG becomes all `255`, and a gradient yields only the four expected levels.
#### Integration test (inline in `src/device/api.rs`, alongside the existing `display_handler`/`render_screen_handler` tests)
- Spin up an in‑memory Axum server (`super::router::<()>(false)`, per the existing tests in that file), send `/api/display` for a device whose `supports_2bit_grayscale` flag is true (e.g., insert a test row into the `devices` table with that flag set).
- Assert the JSON response contains a signed `.png` URL and that fetching it returns a PNG whose `bit-depth` is reported as `2` (use `image::io::Reader`).
- Repeat with a device whose `supports_2bit_grayscale` flag is **false** to ensure the BMP path remains unchanged.
#### End‑to‑end verification
- Use the `/verify` skill after implementation to run a headless browser, load the dashboard, and confirm the image URL changes based on the device’s `supports_2bit_grayscale` flag.

### 7️⃣ Documentation Updates
- **docs/templates.md** – Add a note about grayscale palettes (`#000000 #555555 #aaaaaa #ffffff`).
- **docs/api.md** – Document the new device‑model flag (`supports_2bit_grayscale`) and updated response fields.
- **docs/projects/ideas/2-bit-grayscale-support.md** – Mark as completed (or move to `completed` folder) once the plan is merged.

## Milestones & Timeline
| Milestone | Owner | ETA |
|-----------|-------|-----|
| Device flag detection & display-handler URL branch | backend team | 2026‑07‑24 |
| PNG rendering pipeline refactor | render team | 2026‑07‑28 |
| `image` crate conversion module | graphics subteam | 2026‑08‑02 |
| Signing & URL changes | security team | 2026‑08‑05 |
| Unit + integration tests | QA | 2026‑08‑07 |
| Documentation & final review | docs lead | 2026‑08‑09 |
| Merge to `main` | release manager | 2026‑08‑12 |

---
*This plan is stored under `docs/projects/plans/`. Subsequent commits will flesh out each milestone with code changes and test implementations.*