//! SVG export.
//!
//! Text is exported as outlines, so the file looks the same without the fonts installed.

use std::fmt::Write as _;

use kurbo::{Affine, BezPath, Rect};

use crate::error::{Error, Result};
use crate::id::{NodeId, PageId};
use crate::model::{AssetStore, Document};
use crate::paint::{BlendMode, Color, FillRule, GradientStop, LineCap, LineJoin, Paint, Stroke};
use crate::scene::{DisplayItem, DisplayList, SceneBuilder};

/// Exports nodes of a page (usually frames) as one SVG document each.
pub fn export_nodes(
    doc: &Document,
    page: PageId,
    ids: &[NodeId],
    builder: &mut SceneBuilder,
) -> Result<Vec<String>> {
    builder
        .build_nodes(doc, page, ids)?
        .iter()
        .map(|list| to_svg(list, &doc.assets))
        .collect()
}

/// Exports a region of a page (page space) as SVG.
pub fn export_area(
    doc: &Document,
    page: PageId,
    area: Rect,
    builder: &mut SceneBuilder,
) -> Result<String> {
    let page = doc.page(page).ok_or(Error::PageNotFound(page))?;
    to_svg(&builder.build_area(doc, page, area), &doc.assets)
}

/// Serializes a display list as SVG.
pub fn to_svg(list: &DisplayList, assets: &AssetStore) -> Result<String> {
    let mut w = SvgWriter::default();
    let area = list.area;
    let (width, height) = (area.width(), area.height());
    // The viewBox selects the area in page coordinates, so paths keep their page positions.
    writeln!(
        w.body,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="{} {} {width} {height}">"#,
        area.x0, area.y0
    )
    .unwrap();
    let header_len = w.body.len();

    if let Some(color) = list.background {
        writeln!(
            w.body,
            r#"<rect x="{}" y="{}" width="{width}" height="{height}"{}/>"#,
            area.x0,
            area.y0,
            fill_attrs(color)
        )
        .unwrap();
    }

    let mut depth = 0usize;
    for item in &list.items {
        match item {
            DisplayItem::Fill {
                path,
                transform,
                paint,
                rule,
            } => {
                let fill = w.paint(paint, "fill");
                let rule = match rule {
                    FillRule::NonZero => "",
                    FillRule::EvenOdd => r#" fill-rule="evenodd""#,
                };
                writeln!(
                    w.body,
                    r#"<path d="{}"{}{fill}{rule}/>"#,
                    path_data(path),
                    transform_attr(transform)
                )
                .unwrap();
            }
            DisplayItem::Stroke {
                path,
                transform,
                stroke,
            } => {
                let attrs = w.stroke(stroke);
                writeln!(
                    w.body,
                    r#"<path d="{}"{} fill="none"{attrs}/>"#,
                    path_data(path),
                    transform_attr(transform)
                )
                .unwrap();
            }
            DisplayItem::Image {
                asset,
                size,
                transform,
            } => {
                let asset = assets.get(*asset).ok_or(Error::AssetNotFound(*asset))?;
                writeln!(
                    w.body,
                    r#"<image width="{}" height="{}" preserveAspectRatio="none"{} href="data:{};base64,{}"/>"#,
                    size.width,
                    size.height,
                    transform_attr(transform),
                    asset.media_type,
                    base64(&asset.data)
                )
                .unwrap();
            }
            DisplayItem::PushLayer { opacity, blend } => {
                depth += 1;
                w.body.push_str("<g");
                if *opacity < 1.0 {
                    write!(w.body, r#" opacity="{opacity}""#).unwrap();
                }
                if *blend != BlendMode::Normal {
                    write!(w.body, r#" style="mix-blend-mode:{}""#, blend_name(*blend)).unwrap();
                }
                w.body.push_str(">\n");
            }
            DisplayItem::PushClip { path, transform } => {
                depth += 1;
                let id = w.clip_id();
                writeln!(
                    w.defs,
                    r#"<clipPath id="{id}"><path d="{}"{}/></clipPath>"#,
                    path_data(path),
                    transform_attr(transform)
                )
                .unwrap();
                writeln!(w.body, r#"<g clip-path="url(#{id})">"#).unwrap();
            }
            DisplayItem::PopLayer | DisplayItem::PopClip => {
                if depth > 0 {
                    depth -= 1;
                    w.body.push_str("</g>\n");
                }
            }
        }
    }
    for _ in 0..depth {
        w.body.push_str("</g>\n");
    }
    w.body.push_str("</svg>\n");

    if !w.defs.is_empty() {
        w.body
            .insert_str(header_len, &format!("<defs>\n{}</defs>\n", w.defs));
    }
    Ok(w.body)
}

#[derive(Default)]
struct SvgWriter {
    body: String,
    defs: String,
    next_id: usize,
}

impl SvgWriter {
    /// Returns ` fill="…"` / ` stroke="…"` attributes, adding gradients to `<defs>`.
    fn paint(&mut self, paint: &Paint, attr: &str) -> String {
        match paint {
            Paint::Solid { color } => color_attrs(*color, attr),
            Paint::LinearGradient { start, end, stops } => {
                let id = self.gradient_id();
                writeln!(
                    self.defs,
                    r#"<linearGradient id="{id}" gradientUnits="userSpaceOnUse" x1="{}" y1="{}" x2="{}" y2="{}">{}</linearGradient>"#,
                    start.x, start.y, end.x, end.y, stops_xml(stops)
                )
                .unwrap();
                format!(r#" {attr}="url(#{id})""#)
            }
            Paint::RadialGradient {
                center,
                radius,
                stops,
            } => {
                let id = self.gradient_id();
                writeln!(
                    self.defs,
                    r#"<radialGradient id="{id}" gradientUnits="userSpaceOnUse" cx="{}" cy="{}" r="{radius}">{}</radialGradient>"#,
                    center.x, center.y, stops_xml(stops)
                )
                .unwrap();
                format!(r#" {attr}="url(#{id})""#)
            }
        }
    }

    fn stroke(&mut self, stroke: &Stroke) -> String {
        let mut attrs = self.paint(&stroke.paint, "stroke");
        write!(attrs, r#" stroke-width="{}""#, stroke.width).unwrap();
        match stroke.join {
            LineJoin::Miter => {
                if stroke.miter_limit != 4.0 {
                    write!(attrs, r#" stroke-miterlimit="{}""#, stroke.miter_limit).unwrap();
                }
            }
            LineJoin::Round => attrs.push_str(r#" stroke-linejoin="round""#),
            LineJoin::Bevel => attrs.push_str(r#" stroke-linejoin="bevel""#),
        }
        match stroke.cap {
            LineCap::Butt => {}
            LineCap::Round => attrs.push_str(r#" stroke-linecap="round""#),
            LineCap::Square => attrs.push_str(r#" stroke-linecap="square""#),
        }
        if !stroke.dash.is_empty() {
            let dashes: Vec<String> = stroke.dash.iter().map(f64::to_string).collect();
            write!(attrs, r#" stroke-dasharray="{}""#, dashes.join(" ")).unwrap();
            if stroke.dash_offset != 0.0 {
                write!(attrs, r#" stroke-dashoffset="{}""#, stroke.dash_offset).unwrap();
            }
        }
        attrs
    }

    fn clip_id(&mut self) -> String {
        self.next_id += 1;
        format!("clip{}", self.next_id)
    }

    fn gradient_id(&mut self) -> String {
        self.next_id += 1;
        format!("gradient{}", self.next_id)
    }
}

fn fill_attrs(color: Color) -> String {
    color_attrs(color, "fill")
}

fn color_attrs(color: Color, attr: &str) -> String {
    let rgb = format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b);
    if color.a == 255 {
        format!(r#" {attr}="{rgb}""#)
    } else {
        format!(r#" {attr}="{rgb}" {attr}-opacity="{}""#, alpha(color))
    }
}

fn stops_xml(stops: &[GradientStop]) -> String {
    let mut out = String::new();
    for stop in stops {
        let c = stop.color;
        write!(
            out,
            r##"<stop offset="{}" stop-color="#{:02x}{:02x}{:02x}""##,
            stop.offset, c.r, c.g, c.b
        )
        .unwrap();
        if c.a != 255 {
            write!(out, r#" stop-opacity="{}""#, alpha(c)).unwrap();
        }
        out.push_str("/>");
    }
    out
}

fn alpha(color: Color) -> f32 {
    // Round to 3 decimals: enough for 8-bit alpha and keeps the output short.
    (f32::from(color.a) / 255.0 * 1000.0).round() / 1000.0
}

fn transform_attr(t: &Affine) -> String {
    if *t == Affine::IDENTITY {
        return String::new();
    }
    let [a, b, c, d, e, f] = t.as_coeffs();
    if (a, b, c, d) == (1.0, 0.0, 0.0, 1.0) {
        format!(r#" transform="translate({e} {f})""#)
    } else {
        format!(r#" transform="matrix({a} {b} {c} {d} {e} {f})""#)
    }
}

fn path_data(path: &BezPath) -> String {
    path.to_svg()
}

fn blend_name(blend: BlendMode) -> &'static str {
    match blend {
        BlendMode::Normal => "normal",
        BlendMode::Multiply => "multiply",
        BlendMode::Screen => "screen",
        BlendMode::Overlay => "overlay",
        BlendMode::Darken => "darken",
        BlendMode::Lighten => "lighten",
        BlendMode::ColorDodge => "color-dodge",
        BlendMode::ColorBurn => "color-burn",
        BlendMode::HardLight => "hard-light",
        BlendMode::SoftLight => "soft-light",
        BlendMode::Difference => "difference",
        BlendMode::Exclusion => "exclusion",
        BlendMode::Hue => "hue",
        BlendMode::Saturation => "saturation",
        BlendMode::Color => "color",
        BlendMode::Luminosity => "luminosity",
    }
}

fn base64(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * i)) as usize & 63] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_rfc4648_vectors() {
        let cases = [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foobar", "Zm9vYmFy"),
        ];
        for (input, expected) in cases {
            assert_eq!(base64(input.as_bytes()), expected);
        }
    }
}
