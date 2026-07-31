# 2‑bit Grayscale Support for TRMNL Display Server

Add optional generation of 2‑bit (four‑level) grayscale images on the server side, so devices that have firmware supporting 2‑bit grayscale can receive richer visuals.

## Why
Production TRMNL hardware now supports true 4‑level grayscale after firmware v1.6.x (see GitHub PR #201, blog post “No more flicker”). Enabling this on the backend lets those devices display richer UI without client changes.

## How
- Detect device capability via a **device‑model flag** (`supports_2bit_grayscale`).
- Render Liquid SVG to PNG (800×480).
- Convert PNG to 2‑bit using Rust's `image` crate:
  ```rust
  use image::{DynamicImage, GenericImageView, ImageBuffer, Luma};

  fn convert_to_2bit(img: DynamicImage) -> ImageBuffer<Luma<u8>, Vec<u8>> {
      let (w, h) = img.dimensions();
      let mut out = ImageBuffer::new(w, h);
      for (x, y, pixel) in img.pixels() {
          // Grayscale 0‑255
          let gray = pixel.to_luma().0[0];
          // Map to four levels: 0,85,170,255
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
  // Save the resulting ImageBuffer as PNG – it will contain only the four gray levels.
  ```
  This uses the `image` crate to map each pixel to one of four gray shades (`#000000 #555555 #aaaaaa #ffffff`). The resulting PNG can then be signed and served.
- Sign and serve the image URL as before, keeping the existing 1‑bit BMP for legacy devices.

## Benefits
- Improved visual fidelity on supported devices.
- No breaking changes for legacy 1‑bit devices.
- Leverages existing signing pipeline.

## Next Steps
- Add device‑model flag detection in `/api/display`.
- Implement conversion step in Rust using the `image` crate.
- Write tests covering both 1‑bit and 2‑bit paths.