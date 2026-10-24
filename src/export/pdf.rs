//! PDF export.
//!
//! Pages keep their vector content. Text is exported as outlines, so it is not selectable
//! yet. krilla is used internally; none of its types are part of this API.

use krilla::color::rgb;
use krilla::geom::{Path, PathBuilder, Size, Transform};
use krilla::image::Image;
use krilla::num::NormalizedF32;
use krilla::page::PageSettings;
use krilla::paint::{self, Fill, LinearGradient, RadialGradient, SpreadMethod, Stop};
use kurbo::{Affine, BezPath, PathEl, Shape};

use crate::error::{Error, Result};
use crate::id::{NodeId, PageId};
use crate::model::{AssetStore, Document};
use crate::paint::{BlendMode, Color, FillRule, GradientStop, LineCap, LineJoin, Paint, Stroke};
use crate::scene::{DisplayItem, DisplayList, SceneBuilder};

/// PDF points per page unit (pixel).
const PT_PER_PX: f64 = 0.75;

/// Exports nodes of a page (usually frames) as one PDF, one page per node.
pub fn export_nodes(
    doc: &Document,
    page: PageId,
    ids: &[NodeId],
    builder: &mut SceneBuilder,
) -> Result<Vec<u8>> {
    to_pdf(&builder.build_nodes(doc, page, ids)?, &doc.assets)
}

/// Exports a region of a page (page space) as a one-page PDF.
pub fn export_area(
    doc: &Document,
    page: PageId,
    area: kurbo::Rect,
    builder: &mut SceneBuilder,
) -> Result<Vec<u8>> {
    let page = doc.page(page).ok_or(Error::PageNotFound(page))?;
    to_pdf(&[builder.build_area(doc, page, area)], &doc.assets)
}

/// Writes display lists as the pages of a PDF.
pub fn to_pdf(lists: &[DisplayList], assets: &AssetStore) -> Result<Vec<u8>> {
    let mut pdf = krilla::Document::new();
    for list in lists {
        let size = Size::from_wh(
            (list.size().width * PT_PER_PX) as f32,
            (list.size().height * PT_PER_PX) as f32,
        )
        .ok_or_else(|| Error::Pdf("invalid page size".into()))?;
        let mut page = pdf.start_page_with(PageSettings::new(size));
        let mut surface = page.surface();
        // Page units to points, with the list's area at the page corner.
        surface.push_transform(&transform_of(
            Affine::scale(PT_PER_PX) * list.area_transform(),
        ));

        if let Some(background) = list.background {
            draw_fill(
                &mut surface,
                &list.area.to_path(0.1),
                Affine::IDENTITY,
                &Paint::solid(background),
                FillRule::NonZero,
            );
        }

        // How many surface pops close each open layer or clip.
        let mut stack: Vec<usize> = Vec::new();
        for item in &list.items {
            match item {
                DisplayItem::Fill {
                    path,
                    transform,
                    paint,
                    rule,
                } => draw_fill(&mut surface, path, *transform, paint, *rule),
                DisplayItem::Stroke {
                    path,
                    transform,
                    stroke,
                } => draw_stroke(&mut surface, path, *transform, stroke),
                DisplayItem::Image {
                    asset,
                    size,
                    transform,
                } => {
                    let asset = assets.get(*asset).ok_or(Error::AssetNotFound(*asset))?;
                    let data = krilla::Data::from(asset.data.to_vec());
                    let image = match asset.media_type.as_str() {
                        "image/png" => Image::from_png(data, true),
                        "image/jpeg" => Image::from_jpeg(data, true),
                        other => Err(format!("unsupported image type {other}")),
                    }
                    .map_err(Error::Pdf)?;
                    let size = Size::from_wh(size.width as f32, size.height as f32)
                        .ok_or_else(|| Error::Pdf("invalid image size".into()))?;
                    surface.push_transform(&transform_of(*transform));
                    surface.draw_image(image, size);
                    surface.pop();
                }
                DisplayItem::PushLayer { opacity, blend } => {
                    surface.push_blend_mode(blend_mode(*blend));
                    surface.push_opacity(normalized(*opacity));
                    stack.push(2);
                }
                DisplayItem::PushClip { path, transform } => {
                    // The clip path goes in page space, under the page transform already pushed.
                    match convert_path(&(*transform * path.clone())) {
                        Some(clip) => {
                            surface.push_clip_path(&clip, &paint::FillRule::NonZero);
                            stack.push(1);
                        }
                        None => stack.push(0),
                    }
                }
                DisplayItem::PopLayer | DisplayItem::PopClip => {
                    for _ in 0..stack.pop().unwrap_or(0) {
                        surface.pop();
                    }
                }
            }
        }
        for pops in stack.into_iter().rev() {
            for _ in 0..pops {
                surface.pop();
            }
        }
        surface.pop();
        surface.finish();
        page.finish();
    }
    pdf.finish().map_err(|e| Error::Pdf(format!("{e:?}")))
}

fn draw_fill(
    surface: &mut krilla::surface::Surface<'_>,
    path: &BezPath,
    transform: Affine,
    paint: &Paint,
    rule: FillRule,
) {
    let Some(pdf_path) = convert_path(path) else {
        return;
    };
    let (paint, opacity) = convert_paint(paint);
    surface.push_transform(&transform_of(transform));
    surface.set_stroke(None);
    surface.set_fill(Some(Fill {
        paint,
        opacity,
        rule: match rule {
            FillRule::NonZero => paint::FillRule::NonZero,
            FillRule::EvenOdd => paint::FillRule::EvenOdd,
        },
    }));
    surface.draw_path(&pdf_path);
    surface.pop();
}

fn draw_stroke(
    surface: &mut krilla::surface::Surface<'_>,
    path: &BezPath,
    transform: Affine,
    stroke: &Stroke,
) {
    let Some(pdf_path) = convert_path(path) else {
        return;
    };
    let (paint, opacity) = convert_paint(&stroke.paint);
    surface.push_transform(&transform_of(transform));
    surface.set_fill(None);
    surface.set_stroke(Some(paint::Stroke {
        paint,
        width: stroke.width as f32,
        miter_limit: stroke.miter_limit as f32,
        line_cap: match stroke.cap {
            LineCap::Butt => paint::LineCap::Butt,
            LineCap::Round => paint::LineCap::Round,
            LineCap::Square => paint::LineCap::Square,
        },
        line_join: match stroke.join {
            LineJoin::Miter => paint::LineJoin::Miter,
            LineJoin::Round => paint::LineJoin::Round,
            LineJoin::Bevel => paint::LineJoin::Bevel,
        },
        opacity,
        dash: (!stroke.dash.is_empty()).then(|| paint::StrokeDash {
            array: stroke.dash.iter().map(|d| *d as f32).collect(),
            offset: stroke.dash_offset as f32,
        }),
    }));
    surface.draw_path(&pdf_path);
    surface.pop();
}

/// Returns the paint and its overall opacity (solid colors carry their alpha there).
fn convert_paint(paint: &Paint) -> (paint::Paint, NormalizedF32) {
    match paint {
        Paint::Solid { color } => (
            rgb_color(*color).into(),
            normalized(f32::from(color.a) / 255.0),
        ),
        Paint::LinearGradient { start, end, stops } => (
            LinearGradient {
                x1: start.x as f32,
                y1: start.y as f32,
                x2: end.x as f32,
                y2: end.y as f32,
                transform: Transform::identity(),
                spread_method: SpreadMethod::Pad,
                stops: convert_stops(stops),
                anti_alias: true,
            }
            .into(),
            NormalizedF32::ONE,
        ),
        Paint::RadialGradient {
            center,
            radius,
            stops,
        } => (
            RadialGradient {
                fx: center.x as f32,
                fy: center.y as f32,
                fr: 0.0,
                cx: center.x as f32,
                cy: center.y as f32,
                cr: *radius as f32,
                transform: Transform::identity(),
                spread_method: SpreadMethod::Pad,
                stops: convert_stops(stops),
                anti_alias: true,
            }
            .into(),
            NormalizedF32::ONE,
        ),
    }
}

fn convert_stops(stops: &[GradientStop]) -> Vec<Stop> {
    stops
        .iter()
        .map(|s| Stop {
            offset: normalized(s.offset),
            color: rgb_color(s.color).into(),
            opacity: normalized(f32::from(s.color.a) / 255.0),
        })
        .collect()
}

fn rgb_color(c: Color) -> rgb::Color {
    rgb::Color::new(c.r, c.g, c.b)
}

fn normalized(v: f32) -> NormalizedF32 {
    NormalizedF32::new(v.clamp(0.0, 1.0)).unwrap_or(NormalizedF32::ONE)
}

fn transform_of(t: Affine) -> Transform {
    let [a, b, c, d, e, f] = t.as_coeffs().map(|v| v as f32);
    Transform::from_row(a, b, c, d, e, f)
}

fn convert_path(path: &BezPath) -> Option<Path> {
    let mut builder = PathBuilder::new();
    for el in path.elements() {
        match *el {
            PathEl::MoveTo(p) => builder.move_to(p.x as f32, p.y as f32),
            PathEl::LineTo(p) => builder.line_to(p.x as f32, p.y as f32),
            PathEl::QuadTo(p1, p) => {
                builder.quad_to(p1.x as f32, p1.y as f32, p.x as f32, p.y as f32)
            }
            PathEl::CurveTo(p1, p2, p) => builder.cubic_to(
                p1.x as f32,
                p1.y as f32,
                p2.x as f32,
                p2.y as f32,
                p.x as f32,
                p.y as f32,
            ),
            PathEl::ClosePath => builder.close(),
        }
    }
    builder.finish()
}

fn blend_mode(blend: BlendMode) -> krilla::blend::BlendMode {
    use krilla::blend::BlendMode as B;
    match blend {
        BlendMode::Normal => B::Normal,
        BlendMode::Multiply => B::Multiply,
        BlendMode::Screen => B::Screen,
        BlendMode::Overlay => B::Overlay,
        BlendMode::Darken => B::Darken,
        BlendMode::Lighten => B::Lighten,
        BlendMode::ColorDodge => B::ColorDodge,
        BlendMode::ColorBurn => B::ColorBurn,
        BlendMode::HardLight => B::HardLight,
        BlendMode::SoftLight => B::SoftLight,
        BlendMode::Difference => B::Difference,
        BlendMode::Exclusion => B::Exclusion,
        BlendMode::Hue => B::Hue,
        BlendMode::Saturation => B::Saturation,
        BlendMode::Color => B::Color,
        BlendMode::Luminosity => B::Luminosity,
    }
}
