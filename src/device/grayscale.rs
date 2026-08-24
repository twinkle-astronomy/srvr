use image::{DynamicImage, GenericImageView, ImageBuffer, Luma, Pixel};

/// Maps each pixel's luminance into one of four gray levels (0, 85, 170,
/// 255) — the display-ready values a 2-bit-grayscale-capable e-ink panel
/// renders natively. These map 1:1 onto PNG's own bit-depth-2 sample scaling
/// (sample s in 0..=3 -> round(255 * s / 3)), which `encode_2bit_png` relies
/// on below.
pub fn convert_to_2bit(img: &DynamicImage) -> ImageBuffer<Luma<u8>, Vec<u8>> {
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

/// Packs a 4-level grayscale image (values must be one of 0/85/170/255, as
/// produced by `convert_to_2bit`) into a genuine 2-bit-depth grayscale PNG —
/// 4 samples per byte, MSB-first, one `ceil(width / 4)`-byte row per scanline
/// (PNG rows are byte-aligned, unlike BMP's 4-byte row padding).
pub fn encode_2bit_png(
    img: &ImageBuffer<Luma<u8>, Vec<u8>>,
) -> Result<Vec<u8>, png::EncodingError> {
    let width = img.width();
    let height = img.height();
    let row_bytes = (width as usize + 3) / 4;
    let mut packed = vec![0u8; row_bytes * height as usize];

    for (x, y, pixel) in img.enumerate_pixels() {
        let sample: u8 = match pixel.0[0] {
            0 => 0,
            85 => 1,
            170 => 2,
            _ => 3,
        };
        let byte_index = y as usize * row_bytes + x as usize / 4;
        let shift = 6 - 2 * (x as usize % 4);
        packed[byte_index] |= sample << shift;
    }

    let mut buffer = std::io::Cursor::new(Vec::new());
    {
        let mut encoder = png::Encoder::new(&mut buffer, width, height);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Two);
        let mut writer = encoder.write_header()?;
        writer.write_image_data(&packed)?;
        writer.finish()?;
    }
    Ok(buffer.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid_image(width: u32, height: u32, gray: u8) -> DynamicImage {
        DynamicImage::ImageLuma8(ImageBuffer::from_pixel(width, height, Luma([gray])))
    }

    #[test]
    fn convert_to_2bit_maps_full_white_to_255() {
        let img = solid_image(4, 4, 255);
        let out = convert_to_2bit(&img);
        assert!(out.pixels().all(|p| p.0[0] == 255));
    }

    #[test]
    fn convert_to_2bit_maps_full_black_to_0() {
        let img = solid_image(4, 4, 0);
        let out = convert_to_2bit(&img);
        assert!(out.pixels().all(|p| p.0[0] == 0));
    }

    #[test]
    fn convert_to_2bit_maps_each_range_to_its_bucket() {
        let cases = [
            (0u8, 0u8),
            (63, 0),
            (64, 85),
            (127, 85),
            (128, 170),
            (191, 170),
            (192, 255),
            (255, 255),
        ];
        for (input, expected) in cases {
            let img = solid_image(1, 1, input);
            let out = convert_to_2bit(&img);
            assert_eq!(
                out.get_pixel(0, 0).0[0],
                expected,
                "gray value {input} should map to level {expected}"
            );
        }
    }

    #[test]
    fn convert_to_2bit_gradient_yields_only_four_distinct_levels() {
        let mut img = ImageBuffer::new(256, 1);
        for x in 0..256u32 {
            img.put_pixel(x, 0, Luma([x as u8]));
        }
        let out = convert_to_2bit(&DynamicImage::ImageLuma8(img));
        let mut levels: Vec<u8> = out.pixels().map(|p| p.0[0]).collect();
        levels.sort_unstable();
        levels.dedup();
        assert_eq!(levels, vec![0, 85, 170, 255]);
    }

    #[test]
    fn encode_2bit_png_round_trips_through_image_crate() {
        let mut img = ImageBuffer::new(4, 1);
        img.put_pixel(0, 0, Luma([0]));
        img.put_pixel(1, 0, Luma([85]));
        img.put_pixel(2, 0, Luma([170]));
        img.put_pixel(3, 0, Luma([255]));

        let png_bytes = encode_2bit_png(&img).expect("encode 2-bit png");
        let decoded = image::load_from_memory(&png_bytes).expect("decode png");
        assert_eq!(decoded.to_luma8().pixels().map(|p| p.0[0]).collect::<Vec<_>>(), vec![0, 85, 170, 255]);
    }

    #[test]
    fn encode_2bit_png_reports_a_genuine_bit_depth_of_two() {
        let img = ImageBuffer::from_pixel(4, 4, Luma([170u8]));
        let png_bytes = encode_2bit_png(&img).expect("encode 2-bit png");

        let decoder = png::Decoder::new(std::io::Cursor::new(&png_bytes[..]));
        let reader = decoder.read_info().expect("read png info");
        assert_eq!(
            reader.info().bit_depth,
            png::BitDepth::Two,
            "output must be a real 2-bit-depth PNG, not an 8-bit PNG with 4 used values"
        );
        assert_eq!(reader.info().color_type, png::ColorType::Grayscale);
    }
}
