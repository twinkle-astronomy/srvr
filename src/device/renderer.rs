use std::collections::HashMap;

use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use dioxus::prelude::*;
use liquid::Object;
use thiserror::Error;

use crate::models::RenderContext;

#[derive(Error, Debug)]
pub enum Error {
    #[error("{0}")]
    LiquidError(#[from] liquid::Error),
    #[error("{0}")]
    UsvgError(#[from] usvg::Error),
    #[error("{0}")]
    DbError(#[from] sqlx::error::Error),
    #[error("{0}")]
    PrometheusError(#[from] prometheus_http_query::error::Error),
    #[error("{0}")]
    TzError(#[from] chrono_tz::ParseError),
    #[error("{0}")]
    ReqwestError(#[from] reqwest::Error),
    #[error("{0}")]
    ImageError(#[from] image::ImageError),
}

pub async fn render_vars(render_context: &RenderContext) -> Result<Object, Error> {
    let prometheus_queries = &render_context.prometheus_queries;

    let mut prometheus_data: HashMap<String, Vec<Object>> =
        HashMap::with_capacity(prometheus_queries.len());

    for query in prometheus_queries {
        if let Ok(obj) = query.get_render_obj().await {
            prometheus_data.insert(query.name.clone(), obj);
        }
    }

    let range_queries = &render_context.range_queries;
    let mut range_data: HashMap<String, Vec<Object>> = HashMap::with_capacity(range_queries.len());

    for query in range_queries {
        if let Ok(obj) = query.get_render_obj().await {
            range_data.insert(query.name.clone(), obj);
        }
    }

    let http_sources = &render_context.http_sources;
    let mut http_data: HashMap<String, liquid::model::Value> =
        HashMap::with_capacity(http_sources.len());

    for source in http_sources {
        if let Ok(obj) = source.get_render_obj().await {
            http_data.insert(source.name.clone(), obj);
        }
    }

    let tz: Tz = std::env::var("TZ").unwrap_or("UTC".to_string()).parse()?;

    let utc_now: DateTime<Utc> = Utc::now();
    let time_in_tz: DateTime<Tz> = utc_now.with_timezone(&tz);

    Ok(liquid::object!({
        "device": render_context.device.get_render_obj(),
        "time": time_in_tz.format("%I:%M %P").to_string(),
        "timezone": time_in_tz.format("%Z").to_string(),
        "date": time_in_tz.format("%Y-%m-%d").to_string(),
        "prometheus": liquid::object!(prometheus_data),
        "prometheus_range": liquid::object!(range_data),
        "http": liquid::object!(http_data),
    }))
}

/// Renders a 1-bit BMP image for e-ink displays using SVG + Liquid templates
pub async fn render_screen(render_context: &RenderContext) -> Result<Vec<u8>, Error> {
    // Render SVG from template
    let svg_data = render_context
        .template
        .render(render_vars(render_context).await?)?;

    Ok(svg_to_bmp(&svg_data)?)
}

/// Renders the same 1-bit image as a PNG — for handing to something that
/// can't read BMP (e.g. Claude's vision input, which only accepts
/// jpeg/png/gif/webp). Pixel-for-pixel identical to [`render_screen`]'s BMP:
/// both are built from the same black/white threshold pass.
pub async fn render_screen_png(render_context: &RenderContext) -> Result<Vec<u8>, Error> {
    let svg_data = render_context
        .template
        .render(render_vars(render_context).await?)?;

    Ok(svg_to_png(&svg_data)?)
}

/// Parses and rasterizes SVG, then reduces it to the same black/white pixel
/// grid the eink display will show. Returns (width, height, is_white) with
/// `is_white` in row-major order.
fn svg_to_bilevel(svg_data: &str) -> Result<(usize, usize, Vec<bool>), Error> {
    // Parse SVG
    let mut opt = usvg::Options::default();
    opt.fontdb_mut().load_system_fonts();

    let tree = usvg::Tree::from_str(svg_data, &opt)?;

    // Create pixmap for rendering
    let pixmap_size = tree.size().to_int_size();
    let mut pixmap = tiny_skia::Pixmap::new(pixmap_size.width(), pixmap_size.height())
        .expect("Invalid image size");

    // Render SVG to pixmap
    resvg::render(&tree, tiny_skia::Transform::default(), &mut pixmap.as_mut());

    let width = pixmap.width() as usize;
    let height = pixmap.height() as usize;
    let mut is_white = vec![false; width * height];

    for y in 0..height {
        for x in 0..width {
            let pixel = pixmap.pixel(x as u32, y as u32).unwrap();

            // Convert to grayscale using standard luminance formula
            let gray = (0.299 * pixel.red() as f32
                + 0.587 * pixel.green() as f32
                + 0.114 * pixel.blue() as f32) as u8;

            // Apply threshold: >= 127 is white, < 127 is black
            is_white[y * width + x] = gray >= 127;
        }
    }

    Ok((width, height, is_white))
}

fn svg_to_bmp(svg_data: &str) -> Result<Vec<u8>, Error> {
    let (width, height, is_white) = svg_to_bilevel(svg_data)?;
    bilevel_to_bmp(width, height, &is_white)
}

fn svg_to_png(svg_data: &str) -> Result<Vec<u8>, Error> {
    let (width, height, is_white) = svg_to_bilevel(svg_data)?;
    bilevel_to_png(width, height, &is_white)
}

/// Packs a black/white pixel grid into 1-bit BMP format.
fn bilevel_to_bmp(width: usize, height: usize, is_white: &[bool]) -> Result<Vec<u8>, Error> {
    let row_bytes = (width + 7) / 8; // Round up to nearest byte
    let mut bit_data = vec![0u8; row_bytes * height];

    for y in 0..height {
        for x in 0..width {
            if is_white[y * width + x] {
                let byte_index = y * row_bytes + x / 8;
                let bit_index = 7 - (x % 8); // MSB first
                bit_data[byte_index] |= 1 << bit_index;
            }
        }
    }

    create_bmp_file(width, height, &bit_data)
}

/// Encodes a black/white pixel grid as an 8-bit grayscale PNG (pure 0x00 /
/// 0xFF values — visually identical to the 1-bit BMP, just in a container
/// format vision-capable consumers can decode).
fn bilevel_to_png(width: usize, height: usize, is_white: &[bool]) -> Result<Vec<u8>, Error> {
    let gray_bytes: Vec<u8> = is_white
        .iter()
        .map(|&white| if white { 255u8 } else { 0u8 })
        .collect();
    let img = image::GrayImage::from_raw(width as u32, height as u32, gray_bytes)
        .expect("gray_bytes length matches width * height");

    let mut png_data = std::io::Cursor::new(Vec::new());
    img.write_to(&mut png_data, image::ImageFormat::Png)?;
    Ok(png_data.into_inner())
}

/// Creates a 1-bit BMP file from bit data
fn create_bmp_file(width: usize, height: usize, bit_data: &[u8]) -> Result<Vec<u8>, Error> {
    // BMP requires rows to be padded to 4-byte boundaries
    let row_bytes = (width + 7) / 8;
    let row_size = ((width + 31) / 32) * 4; // Round up to nearest 4 bytes
    let pixel_data_size = row_size * height;
    let color_table_size = 8; // 2 colors * 4 bytes each
    let header_size = 14 + 40; // BMP file header + DIB header
    let file_size = header_size + color_table_size + pixel_data_size;

    let mut bmp_data = Vec::with_capacity(file_size);

    // BMP File Header (14 bytes)
    bmp_data.extend_from_slice(b"BM"); // Signature
    bmp_data.extend_from_slice(&(file_size as u32).to_le_bytes()); // File size
    bmp_data.extend_from_slice(&[0, 0, 0, 0]); // Reserved
    bmp_data.extend_from_slice(&((header_size + color_table_size) as u32).to_le_bytes()); // Pixel data offset

    // DIB Header (BITMAPINFOHEADER, 40 bytes)
    bmp_data.extend_from_slice(&40u32.to_le_bytes()); // Header size
    bmp_data.extend_from_slice(&(width as i32).to_le_bytes()); // Width
    bmp_data.extend_from_slice(&(height as i32).to_le_bytes()); // Height
    bmp_data.extend_from_slice(&1u16.to_le_bytes()); // Planes
    bmp_data.extend_from_slice(&1u16.to_le_bytes()); // Bits per pixel
    bmp_data.extend_from_slice(&0u32.to_le_bytes()); // Compression (none)
    bmp_data.extend_from_slice(&(pixel_data_size as u32).to_le_bytes()); // Image size
    bmp_data.extend_from_slice(&0i32.to_le_bytes()); // X pixels per meter
    bmp_data.extend_from_slice(&0i32.to_le_bytes()); // Y pixels per meter
    bmp_data.extend_from_slice(&2u32.to_le_bytes()); // Colors used (2)
    bmp_data.extend_from_slice(&2u32.to_le_bytes()); // Important colors (2)

    // Color Table (8 bytes: 2 colors * 4 bytes BGRA)
    bmp_data.extend_from_slice(&[0, 0, 0, 0]); // Black (index 0)
    bmp_data.extend_from_slice(&[255, 255, 255, 0]); // White (index 1)

    // Pixel Data (bottom-up, padded rows)
    // BMP stores rows bottom-up, so we need to reverse
    for y in (0..height).rev() {
        let src_offset = y * row_bytes;
        let src_end = (src_offset + row_bytes).min(bit_data.len());
        bmp_data.extend_from_slice(&bit_data[src_offset..src_end]);

        // Add padding to reach 4-byte boundary
        let padding = row_size - row_bytes;
        for _ in 0..padding {
            bmp_data.push(0);
        }
    }

    Ok(bmp_data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_svg_to_png_encodes_black_fill_as_black_pixels() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10" fill="black"/></svg>"#;
        let png_bytes = svg_to_png(svg).expect("encode png");

        let decoded = image::load_from_memory(&png_bytes).expect("decode png");
        assert_eq!(decoded.width(), 10);
        assert_eq!(decoded.height(), 10);
        let pixel = decoded.to_luma8().get_pixel(5, 5).0[0];
        assert_eq!(pixel, 0, "black-filled SVG should render as black pixels");
    }

    #[test]
    fn test_svg_to_png_encodes_white_fill_as_white_pixels() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10" fill="white"/></svg>"#;
        let png_bytes = svg_to_png(svg).expect("encode png");

        let decoded = image::load_from_memory(&png_bytes).expect("decode png");
        let pixel = decoded.to_luma8().get_pixel(5, 5).0[0];
        assert_eq!(pixel, 255, "white-filled SVG should render as white pixels");
    }

    #[test]
    fn test_svg_to_png_matches_svg_to_bmp_dimensions() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="8"><rect width="20" height="8" fill="black"/></svg>"#;
        let png_bytes = svg_to_png(svg).expect("encode png");
        let bmp_bytes = svg_to_bmp(svg).expect("encode bmp");

        let decoded = image::load_from_memory(&png_bytes).expect("decode png");
        assert_eq!(decoded.width(), 20);
        assert_eq!(decoded.height(), 8);
        // BMP width/height live little-endian in the DIB header at fixed offsets.
        let bmp_width = i32::from_le_bytes(bmp_bytes[18..22].try_into().unwrap());
        let bmp_height = i32::from_le_bytes(bmp_bytes[22..26].try_into().unwrap());
        assert_eq!(bmp_width, 20);
        assert_eq!(bmp_height, 8);
    }
}
