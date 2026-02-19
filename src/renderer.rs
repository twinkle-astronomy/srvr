use axum::{
    http::StatusCode,
    response::IntoResponse,
};
use chrono::Utc;
use liquid::ParserBuilder;

/// Renders a 1-bit BMP image for e-ink displays using SVG + Liquid templates
pub async fn render_screen(width: u32, height: u32, scrape_duration: String, fw_version: String) -> impl IntoResponse {
    // Get current time
    let now = Utc::now();

    // Create Liquid template for SVG
    let svg_template = r#"<svg width="{{ width }}" height="{{ height }}" xmlns="http://www.w3.org/2000/svg">
        <!-- White background -->
        <rect width="100%" height="100%" fill="white"/>

        <!-- Black header bar -->
        <rect x="0" y="0" width="{{ width }}" height="80" fill="black"/>

        <!-- Header text (white on black) -->
        <text x="400" y="50" font-family="Liberation Sans, DejaVu Sans, Arial, sans-serif" font-size="36" font-weight="bold" text-anchor="middle" fill="white">
            TRMNL Display
        </text>

        <!-- Current time -->
        <text x="400" y="150" font-family="Liberation Sans, DejaVu Sans, Arial, sans-serif" font-size="32" text-anchor="middle" fill="black">
            {{ time }}
        </text>

        <!-- Metric label -->
        <text x="400" y="220" font-family="Liberation Sans, DejaVu Sans, Arial, sans-serif" font-size="24" text-anchor="middle" fill="black">
            Front Porch
        </text>

        <!-- Metric value -->
        <text x="400" y="290" font-family="Liberation Sans, DejaVu Sans, Arial, sans-serif" font-size="64" font-weight="bold" text-anchor="middle" fill="black">
            {{ scrape_duration }}
        </text>

        <!-- Black footer bar -->
        <rect x="0" y="{{ footer_y }}" width="{{ width }}" height="80" fill="black"/>

        <!-- Footer text (white on black) -->
        <text x="400" y="{{ footer_text_y }}" font-family="Liberation Sans, DejaVu Sans, Arial, sans-serif" font-size="24" text-anchor="middle" fill="white">
            {{ date }} | FW {{ fw_version }}
        </text>
    </svg>"#;

    // Render SVG from template
    let svg_data = match render_svg_template(svg_template, width, height, &now, &scrape_duration, &fw_version) {
        Ok(svg) => svg,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Template error: {}", e)
            ).into_response();
        }
    };

    // Convert SVG to 1-bit BMP
    match svg_to_bmp(&svg_data) {
        Ok((bmp_data, _, _)) => {
            (
                StatusCode::OK,
                [("Content-Type", "image/bmp")],
                bmp_data,
            ).into_response()
        }
        Err(e) => {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Rendering error: {}", e)
            ).into_response()
        }
    }
}

/// Renders a Liquid template to SVG string
fn render_svg_template(template_str: &str, width: u32, height: u32, now: &chrono::DateTime<Utc>, scrape_duration: &str, fw_version: &str) -> Result<String, String> {
    let parser = ParserBuilder::with_stdlib()
        .build()
        .map_err(|e| format!("Failed to build parser: {}", e))?;

    let template = parser.parse(template_str)
        .map_err(|e| format!("Failed to parse template: {}", e))?;

    // Create template context with variables
    let globals = liquid::object!({
        "width": width.to_string(),
        "height": height.to_string(),
        "footer_y": (height - 80).to_string(),
        "footer_text_y": (height - 30).to_string(),
        "time": now.format("%H:%M:%S UTC").to_string(),
        "date": now.format("%Y-%m-%d").to_string(),
        "scrape_duration": scrape_duration.to_string(),
        "fw_version": fw_version.to_string(),
    });

    template.render(&globals)
        .map_err(|e| format!("Failed to render template: {}", e))
}

/// Converts SVG string to 1-bit BMP data
/// Returns (bmp_data, width, height)
fn svg_to_bmp(svg_data: &str) -> Result<(Vec<u8>, u32, u32), String> {
    // Parse SVG
    let mut opt = usvg::Options::default();
    opt.fontdb_mut().load_system_fonts();

    let tree = usvg::Tree::from_str(svg_data, &opt)
        .map_err(|e| format!("Failed to parse SVG: {}", e))?;

    // Create pixmap for rendering
    let pixmap_size = tree.size().to_int_size();
    let mut pixmap = tiny_skia::Pixmap::new(pixmap_size.width(), pixmap_size.height())
        .ok_or("Failed to create pixmap")?;

    // Render SVG to pixmap
    resvg::render(&tree, tiny_skia::Transform::default(), &mut pixmap.as_mut());

    // Convert to 1-bit BMP
    let bmp_data = pixmap_to_bmp(&pixmap)?;
    Ok((bmp_data, pixmap.width(), pixmap.height()))
}

/// Converts a pixmap to 1-bit BMP format
fn pixmap_to_bmp(pixmap: &tiny_skia::Pixmap) -> Result<Vec<u8>, String> {
    let width = pixmap.width() as usize;
    let height = pixmap.height() as usize;

    // Convert RGBA pixmap to 1-bit format
    let row_bytes = (width + 7) / 8; // Round up to nearest byte
    let mut bit_data = vec![0u8; row_bytes * height];

    // Pack pixels into 1-bit format
    for y in 0..height {
        for x in 0..width {
            let pixel = pixmap.pixel(x as u32, y as u32)
                .ok_or("Failed to get pixel")?;

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
fn create_bmp_file(width: usize, height: usize, bit_data: &[u8]) -> Result<Vec<u8>, String> {
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
