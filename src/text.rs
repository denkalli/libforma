//! Text layout with Parley, using the fonts installed on the system.
//!
//! Text is converted to glyph outlines, so every backend (GPU, SVG, PDF) draws it as plain
//! paths and exported files look the same on machines without the fonts.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

use kurbo::{Affine, BezPath, Size};
use parley::{
    Alignment, AlignmentOptions, FontContext, FontFamily, FontStyle, FontWeight, Layout,
    LayoutContext, LineHeight, PositionedLayoutItem, StyleProperty,
};
use skrifa::instance::{LocationRef, NormalizedCoord};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::raw::TableProvider;
use skrifa::{FontRef, GlyphId, MetadataProvider};

use crate::model::{TextAlign, TextNode, TextVerticalAlign};
use crate::paint::{FillRule, Paint, Stroke};
use crate::scene::DisplayItem;

/// Lays out text nodes. Loading the system font collection is slow, so create one engine
/// and reuse it.
pub struct TextEngine {
    fonts: FontContext,
    layout: LayoutContext<()>,
    /// Glyph outlines in font units, by font and glyph.
    outlines: HashMap<GlyphKey, Option<BezPath>>,
    /// Laid-out text, so unchanged text nodes are not shaped again on every redraw.
    shaped: HashMap<ShapeKey, Arc<Shaped>>,
}

/// Most texts kept in the layout cache; it is emptied when full.
const MAX_SHAPED: usize = 4096;

/// Everything that affects a text's layout (the color does not).
#[derive(PartialEq, Eq, Hash)]
struct ShapeKey {
    content: String,
    family: String,
    size: u64,
    weight: u16,
    italic: bool,
    line_height: u32,
    letter_spacing: u32,
    align: u8,
    width: Option<u64>,
}

impl ShapeKey {
    fn new(text: &TextNode) -> Self {
        let style = &text.style;
        Self {
            content: text.content.clone(),
            family: style.font_family.clone(),
            size: style.font_size.to_bits(),
            weight: style.font_weight,
            italic: style.italic,
            line_height: style.line_height.to_bits(),
            letter_spacing: style.letter_spacing.to_bits(),
            align: text.align as u8,
            width: text.width.map(f64::to_bits),
        }
    }
}

/// A laid-out text: its size and one outline per glyph run, in local units.
struct Shaped {
    size: Size,
    runs: Vec<ShapedRun>,
}

struct ShapedRun {
    path: BezPath,
    /// Stroke width that thickens the run when the font has no bold face.
    embolden: Option<f64>,
}

#[derive(PartialEq, Eq, Hash)]
struct GlyphKey {
    font: u64,
    index: u32,
    glyph: u32,
    coords: Vec<i16>,
}

impl Default for TextEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl TextEngine {
    pub fn new() -> Self {
        Self {
            fonts: FontContext::new(),
            layout: LayoutContext::new(),
            outlines: HashMap::new(),
            shaped: HashMap::new(),
        }
    }

    /// Size of the laid-out text, in local units.
    pub fn measure(&mut self, text: &TextNode) -> Size {
        self.shape(text).size
    }

    /// Names of the font families that can be used, sorted without regard to case.
    pub fn families(&mut self) -> Vec<String> {
        let mut names: Vec<String> = self
            .fonts
            .collection
            .family_names()
            .map(str::to_owned)
            .collect();
        names.sort_by_key(|n| n.to_lowercase());
        names.dedup();
        names
    }

    /// Weights (100 to 900) of the upright faces of `family`, sorted. Empty for an unknown
    /// family. Variable fonts report the weights of their named instances.
    pub fn weights(&mut self, family: &str) -> Vec<u16> {
        let Some(info) = self.fonts.collection.family_by_name(family) else {
            return Vec::new();
        };
        let mut weights: Vec<u16> = info
            .fonts()
            .iter()
            .filter(|f| f.style() == parley::fontique::FontStyle::Normal)
            .map(|f| f.weight().value().round().clamp(1.0, 1000.0) as u16)
            .collect();
        weights.sort_unstable();
        weights.dedup();
        weights
    }

    /// Registers the fonts in a font file (TrueType, OpenType or a collection) and returns
    /// the names of their families. Unreadable data registers nothing.
    pub fn register_font(&mut self, data: Vec<u8>) -> Vec<String> {
        let blob = parley::fontique::Blob::new(Arc::new(data));
        let added = self.fonts.collection.register_fonts(blob, None);
        // Text may have used a fallback for a family that now exists.
        self.shaped.clear();
        let mut names: Vec<String> = added
            .into_iter()
            .filter_map(|(id, _)| Some(self.fonts.collection.family(id)?.name().to_owned()))
            .collect();
        names.dedup();
        names
    }

    /// The file data and face index of the regular face of `family` (or its closest
    /// weight), for showing previews with another renderer.
    pub fn font_data(&mut self, family: &str) -> Option<(Vec<u8>, u32)> {
        let info = self.fonts.collection.family_by_name(family)?;
        let font = info
            .fonts()
            .iter()
            .min_by_key(|f| {
                let upright = f.style() == parley::fontique::FontStyle::Normal;
                (!upright, (f.weight().value() - 400.0).abs() as u32)
            })?
            .clone();
        let blob = font.load(Some(&mut self.fonts.source_cache))?;
        Some((blob.data().to_vec(), font.index()))
    }

    /// Size of the text's box: its fixed width and height where set, the laid-out text
    /// otherwise.
    pub fn box_size(&mut self, text: &TextNode) -> Size {
        let measured = self.measure(text);
        Size::new(
            text.width.unwrap_or(measured.width),
            text.height
                .filter(|_| text.width.is_some())
                .unwrap_or(measured.height),
        )
    }

    /// The layout of `text`, from the cache when it was laid out before.
    fn shape(&mut self, text: &TextNode) -> Arc<Shaped> {
        let key = ShapeKey::new(text);
        if let Some(shaped) = self.shaped.get(&key) {
            return shaped.clone();
        }
        if self.shaped.len() >= MAX_SHAPED {
            self.shaped.clear();
        }
        let shaped = Arc::new(self.shape_uncached(text));
        self.shaped.insert(key, shaped.clone());
        shaped
    }

    fn layout(&mut self, text: &TextNode) -> Layout<()> {
        let style = &text.style;
        let mut builder = self
            .layout
            .ranged_builder(&mut self.fonts, &text.content, 1.0, false);
        builder.push_default(StyleProperty::FontFamily(FontFamily::Source(
            Cow::Borrowed(&style.font_family),
        )));
        builder.push_default(StyleProperty::FontSize(style.font_size as f32));
        builder.push_default(StyleProperty::FontWeight(FontWeight::new(f32::from(
            style.font_weight,
        ))));
        if style.italic {
            builder.push_default(StyleProperty::FontStyle(FontStyle::Italic));
        }
        builder.push_default(StyleProperty::LineHeight(LineHeight::FontSizeRelative(
            style.line_height,
        )));
        if style.letter_spacing != 0.0 {
            builder.push_default(StyleProperty::LetterSpacing(style.letter_spacing));
        }
        let mut layout: Layout<()> = builder.build(&text.content);
        layout.break_all_lines(text.width.map(|w| w as f32));
        let alignment = match text.align {
            TextAlign::Start => Alignment::Start,
            TextAlign::Center => Alignment::Center,
            TextAlign::End => Alignment::End,
            TextAlign::Justify => Alignment::Justify,
        };
        layout.align(alignment, AlignmentOptions::default());
        layout
    }

    /// Appends the outlines of `text` to `items`. One fill is emitted per glyph run.
    pub(crate) fn layout_into(
        &mut self,
        text: &TextNode,
        items: &mut Vec<DisplayItem>,
        transform: Affine,
    ) {
        let shaped = self.shape(text);
        let paint = Paint::solid(text.style.color);
        // In a box of fixed height, the text block moves to the middle or the bottom.
        let free = text
            .height
            .filter(|_| text.width.is_some())
            .map_or(0.0, |h| h - shaped.size.height);
        let offset = match text.vertical_align {
            TextVerticalAlign::Top => 0.0,
            TextVerticalAlign::Middle => free / 2.0,
            TextVerticalAlign::Bottom => free,
        };
        let transform = transform * Affine::translate((0.0, offset));
        for run in &shaped.runs {
            if let Some(width) = run.embolden {
                items.push(DisplayItem::Stroke {
                    path: run.path.clone(),
                    transform,
                    stroke: Stroke::new(paint.clone(), width),
                });
            }
            items.push(DisplayItem::Fill {
                path: run.path.clone(),
                transform,
                paint: paint.clone(),
                rule: FillRule::NonZero,
            });
        }
    }

    /// Lays out `text` with Parley and converts its glyphs to outlines.
    fn shape_uncached(&mut self, text: &TextNode) -> Shaped {
        let layout = self.layout(text);
        let size = Size::new(f64::from(layout.width()), f64::from(layout.height()));
        let mut runs = Vec::new();
        for line in layout.lines() {
            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                    continue;
                };
                let run = glyph_run.run();
                let font = run.font();
                let Ok(font_ref) = FontRef::from_index(font.data.data(), font.index) else {
                    continue;
                };
                let units_per_em = font_ref.head().map(|h| h.units_per_em()).unwrap_or(1000);
                let scale = f64::from(run.font_size()) / f64::from(units_per_em);
                let coords: Vec<i16> = run
                    .normalized_coords()
                    .iter()
                    .map(|c| c.to_bits())
                    .collect();
                let synthesis = run.synthesis();
                // Font units are y-up; the page is y-down.
                let mut glyph_transform = Affine::scale_non_uniform(scale, -scale);
                if let Some(angle) = synthesis.skew() {
                    glyph_transform =
                        Affine::skew(f64::from(angle).to_radians().tan(), 0.0) * glyph_transform;
                }

                let mut path = BezPath::new();
                for glyph in glyph_run.positioned_glyphs() {
                    let key = GlyphKey {
                        font: font.data.id(),
                        index: font.index,
                        glyph: glyph.id,
                        coords: coords.clone(),
                    };
                    let outline = self
                        .outlines
                        .entry(key)
                        .or_insert_with(|| glyph_outline(&font_ref, glyph.id, &coords));
                    if let Some(outline) = outline {
                        let place = Affine::translate((f64::from(glyph.x), f64::from(glyph.y)))
                            * glyph_transform;
                        path.extend(place * outline.clone());
                    }
                }
                if path.is_empty() {
                    continue;
                }
                runs.push(ShapedRun {
                    path,
                    embolden: synthesis
                        .embolden()
                        .then(|| f64::from(run.font_size()) / 30.0),
                });
            }
        }
        Shaped { size, runs }
    }
}

fn glyph_outline(font: &FontRef<'_>, glyph: u32, coords: &[i16]) -> Option<BezPath> {
    let outline = font.outline_glyphs().get(GlyphId::new(glyph))?;
    let coords: Vec<NormalizedCoord> = coords
        .iter()
        .map(|c| NormalizedCoord::from_bits(*c))
        .collect();
    let mut pen = PathPen(BezPath::new());
    outline
        .draw(
            DrawSettings::unhinted(
                skrifa::instance::Size::unscaled(),
                LocationRef::new(&coords),
            ),
            &mut pen,
        )
        .ok()?;
    Some(pen.0)
}

struct PathPen(BezPath);

impl OutlinePen for PathPen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to((f64::from(x), f64::from(y)));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to((f64::from(x), f64::from(y)));
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.0.quad_to(
            (f64::from(cx0), f64::from(cy0)),
            (f64::from(x), f64::from(y)),
        );
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.0.curve_to(
            (f64::from(cx0), f64::from(cy0)),
            (f64::from(cx1), f64::from(cy1)),
            (f64::from(x), f64::from(y)),
        );
    }

    fn close(&mut self) {
        self.0.close_path();
    }
}
