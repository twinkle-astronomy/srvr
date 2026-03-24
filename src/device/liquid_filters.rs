use std::fmt;

use liquid_core::{
    Error, Expression, Filter, Result, Runtime, Value, ValueView,
    parser::{FilterArguments, FilterReflection, ParameterReflection, ParseFilter},
};
use qrcode::{Color, QrCode};

// ─── QR code SVG generation ───────────────────────────────────────────────────

/// Generates an inline SVG `<g>` element for a QR code.
/// `module_size` is the pixel size of each module (dark/light square).
fn qrcode_to_svg_group(
    data: &str,
    module_size: u32,
) -> std::result::Result<String, qrcode::types::QrError> {
    let code = QrCode::new(data.as_bytes())?;
    let width = code.width();

    // Quiet zone: 4 modules on each side (per QR spec)
    let quiet = 4u32;
    let total = (width as u32 + quiet * 2) * module_size;

    let mut svg =
        format!(r#"<g><rect x="0" y="0" width="{total}" height="{total}" fill="white"/>"#);

    for y in 0..width {
        for x in 0..width {
            if code[(x, y)] == Color::Dark {
                let px = (x as u32 + quiet) * module_size;
                let py = (y as u32 + quiet) * module_size;
                svg.push_str(&format!(
                    r#"<rect x="{px}" y="{py}" width="{module_size}" height="{module_size}"/>"#
                ));
            }
        }
    }

    svg.push_str("</g>");
    Ok(svg)
}

/// Encodes WiFi credentials into the standard WiFi QR URI format.
/// `security` should be one of: `WPA`, `WEP`, `nopass` (default: `WPA`).
fn wifi_qr_string(ssid: &str, password: &str, security: &str) -> String {
    // Escape special characters in SSID and password
    let escape = |s: &str| {
        s.replace('\\', "\\\\")
            .replace(';', "\\;")
            .replace(',', "\\,")
            .replace('"', "\\\"")
    };
    format!(
        "WIFI:S:{};T:{};P:{};;",
        escape(ssid),
        security,
        escape(password)
    )
}

fn eval_str(expr: &Expression, runtime: &dyn Runtime) -> Result<String> {
    Ok(expr
        .evaluate(runtime)?
        .as_scalar()
        .map(|s| s.into_cow_str().to_string())
        .unwrap_or_default())
}

fn eval_u32(expr: &Expression, runtime: &dyn Runtime, default: u32) -> Result<u32> {
    Ok(expr
        .evaluate(runtime)?
        .as_scalar()
        .and_then(|s| s.to_integer())
        .map(|n| n.max(1) as u32)
        .unwrap_or(default))
}

// ─── `qrcode` filter ──────────────────────────────────────────────────────────
//
// Usage:
//   {{ "https://example.com" | qrcode }}
//   {{ some_var | qrcode, module_size: 3 }}

#[derive(Debug)]
pub struct QrcodeFilter {
    module_size: Option<Expression>,
}

impl fmt::Display for QrcodeFilter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "qrcode")
    }
}

impl Filter for QrcodeFilter {
    fn evaluate(&self, input: &dyn ValueView, runtime: &dyn Runtime) -> Result<Value> {
        let module_size = match &self.module_size {
            Some(expr) => eval_u32(expr, runtime, 1)?,
            None => 1,
        };

        let data = input.to_kstr().to_string();
        let svg = qrcode_to_svg_group(&data, module_size)
            .map_err(|e| Error::with_msg(format!("qrcode filter: {e}")))?;

        Ok(Value::scalar(svg))
    }
}

#[derive(Clone)]
pub struct QrcodeFilterParser;

impl FilterReflection for QrcodeFilterParser {
    fn name(&self) -> &str {
        "qrcode"
    }
    fn description(&self) -> &str {
        "Renders a string as a QR code inline SVG fragment."
    }
    fn positional_parameters(&self) -> &'static [ParameterReflection] {
        &[]
    }
    fn keyword_parameters(&self) -> &'static [ParameterReflection] {
        &[ParameterReflection {
            name: "module_size",
            description: "Pixel size of each QR module (default: 1)",
            is_optional: true,
        }]
    }
}

impl ParseFilter for QrcodeFilterParser {
    fn parse(&self, mut arguments: FilterArguments) -> Result<Box<dyn Filter>> {
        let mut module_size = None;
        for (key, expr) in &mut arguments.keyword {
            match key {
                "module_size" => module_size = Some(expr),
                _ => return Err(Error::with_msg(format!("qrcode: unknown argument '{key}'"))),
            }
        }
        Ok(Box::new(QrcodeFilter { module_size }))
    }
    fn reflection(&self) -> &dyn FilterReflection {
        self
    }
}

// ─── `qrcode_wifi` filter ─────────────────────────────────────────────────────
//
// Usage:
//   {{ "MyNetwork" | qrcode_wifi: password: "secret" }}
//   {{ "MyNetwork" | qrcode_wifi: password: "secret", security: "WEP" }}
//   {{ "OpenNet"   | qrcode_wifi: password: "", security: "nopass" }}
//   {{ "MyNetwork" | qrcode_wifi: password: "secret", module_size: 3 }}

#[derive(Debug)]
pub struct QrcodeWifiFilter {
    password: Expression,
    security: Option<Expression>,
    module_size: Option<Expression>,
}

impl fmt::Display for QrcodeWifiFilter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "qrcode_wifi")
    }
}

impl Filter for QrcodeWifiFilter {
    fn evaluate(&self, input: &dyn ValueView, runtime: &dyn Runtime) -> Result<Value> {
        let ssid = input.to_kstr().to_string();
        let password = eval_str(&self.password, runtime)?;
        let security = match &self.security {
            Some(expr) => eval_str(expr, runtime)?,
            None => "WPA".to_string(),
        };
        let module_size = match &self.module_size {
            Some(expr) => eval_u32(expr, runtime, 1)?,
            None => 1,
        };

        let qr_data = wifi_qr_string(&ssid, &password, &security);
        let svg = qrcode_to_svg_group(&qr_data, module_size)
            .map_err(|e| Error::with_msg(format!("qrcode_wifi filter: {e}")))?;

        Ok(Value::scalar(svg))
    }
}

#[derive(Clone)]
pub struct QrcodeWifiFilterParser;

impl FilterReflection for QrcodeWifiFilterParser {
    fn name(&self) -> &str {
        "qrcode_wifi"
    }
    fn description(&self) -> &str {
        "Renders WiFi credentials as a QR code inline SVG fragment. Input is the SSID."
    }
    fn positional_parameters(&self) -> &'static [ParameterReflection] {
        &[]
    }
    fn keyword_parameters(&self) -> &'static [ParameterReflection] {
        &[
            ParameterReflection {
                name: "password",
                description: "WiFi password",
                is_optional: false,
            },
            ParameterReflection {
                name: "security",
                description: "Security type: WPA (default), WEP, or nopass",
                is_optional: true,
            },
            ParameterReflection {
                name: "module_size",
                description: "Pixel size of each QR module (default: 1)",
                is_optional: true,
            },
        ]
    }
}

impl ParseFilter for QrcodeWifiFilterParser {
    fn parse(&self, mut arguments: FilterArguments) -> Result<Box<dyn Filter>> {
        let mut password = None;
        let mut security = None;
        let mut module_size = None;

        for (key, expr) in &mut arguments.keyword {
            match key {
                "password" => password = Some(expr),
                "security" => security = Some(expr),
                "module_size" => module_size = Some(expr),
                _ => {
                    return Err(Error::with_msg(format!(
                        "qrcode_wifi: unknown argument '{key}'"
                    )));
                }
            }
        }

        let password =
            password.ok_or_else(|| Error::with_msg("qrcode_wifi: 'password' is required"))?;

        Ok(Box::new(QrcodeWifiFilter {
            password,
            security,
            module_size,
        }))
    }
    fn reflection(&self) -> &dyn FilterReflection {
        self
    }
}
