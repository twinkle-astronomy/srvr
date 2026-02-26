use std::collections::HashMap;

use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use dioxus::prelude::*;
use liquid::Object;
use thiserror::Error;

use crate::{
    db,
    models::{Device, Template},
};

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
}

pub async fn render_vars(device: &Device, template: &Template) -> Result<Object, Error> {
    let prometheus_queries = db::get_prometheus_queries(template.id).await?;
    let mut prometheus_data: HashMap<String, Vec<Object>> =
        HashMap::with_capacity(prometheus_queries.len());
    for query in prometheus_queries {
        prometheus_data.insert(query.name.clone(), query.get_render_obj().await?);
    }

    let tz: Tz = std::env::var("TZ").unwrap_or("UTC".to_string()).parse()?;

    let utc_now: DateTime<Utc> = Utc::now();
    let time_in_tz: DateTime<Tz> = utc_now.with_timezone(&tz);

    Ok(liquid::object!({
        "device": device.get_render_obj(),
        "time": time_in_tz.format("%I:%M %P").to_string(),
        "timezone": time_in_tz.format("%Z").to_string(),
        "date": time_in_tz.format("%Y-%m-%d").to_string(),
        "prometheus": liquid::object!(prometheus_data),
    }))
}

/// Renders a 1-bit BMP image for e-ink displays using SVG + Liquid templates
pub async fn render_screen(device: &Device, template: &Template) -> Result<Vec<u8>, Error> {
    // Render SVG from template
    let svg_data = template.render(render_vars(device, template).await?)?;

    Ok(svg_to_bmp(&svg_data)?)
}

/// Converts SVG string to 1-bit BMP data
/// Returns (bmp_data, width, height)
fn svg_to_bmp(svg_data: &str) -> Result<Vec<u8>, Error> {
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

    // Convert to 1-bit BMP
    let bmp_data = pixmap_to_bmp(&pixmap)?;
    Ok(bmp_data)
}

/// Converts a pixmap to 1-bit BMP format
fn pixmap_to_bmp(pixmap: &tiny_skia::Pixmap) -> Result<Vec<u8>, Error> {
    let width = pixmap.width() as usize;
    let height = pixmap.height() as usize;

    // Convert RGBA pixmap to 1-bit format
    let row_bytes = (width + 7) / 8; // Round up to nearest byte
    let mut bit_data = vec![0u8; row_bytes * height];

    // Pack pixels into 1-bit format
    for y in 0..height {
        for x in 0..width {
            let pixel = pixmap.pixel(x as u32, y as u32).unwrap();

            // Convert to grayscale using standard luminance formula
            let gray = (0.299 * pixel.red() as f32
                + 0.587 * pixel.green() as f32
                + 0.114 * pixel.blue() as f32) as u8;

            // Apply threshold: >= 127 is white (1), < 127 is black (0)
            if gray >= 127 {
                let byte_index = y * row_bytes + x / 8;
                let bit_index = 7 - (x % 8); // MSB first
                bit_data[byte_index] |= 1 << bit_index;
            }
        }
    }

    // Create BMP file
    create_bmp_file(width, height, &bit_data)
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
