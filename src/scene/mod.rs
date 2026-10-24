//! The display list: a page flattened into simple drawing operations.
//!
//! This is the boundary between the document model and everything that draws it. The GPU
//! renderer and the SVG and PDF exporters only ever see a [`DisplayList`], so none of them
//! needs to understand the document tree, and replacing a backend touches nothing else.

mod build;

use kurbo::{Affine, BezPath, Rect, Size};

use crate::id::AssetId;
use crate::paint::{BlendMode, Color, FillRule, Paint, Stroke};

pub use build::SceneBuilder;

/// Drawing operations, back to front, in page space.
#[derive(Clone, PartialEq, Debug)]
pub struct DisplayList {
    /// The region of interest in page space. Exporters produce an image of exactly this
    /// region; the canvas ignores it and shows whatever the view covers.
    pub area: Rect,
    /// Color under everything, or `None` for transparent.
    pub background: Option<Color>,
    pub items: Vec<DisplayItem>,
}

impl DisplayList {
    /// Size of the exported image, in page units.
    pub fn size(&self) -> Size {
        self.area.size()
    }

    /// Maps page space to the exported image (the area's corner becomes the origin).
    pub fn area_transform(&self) -> Affine {
        Affine::translate(-self.area.origin().to_vec2())
    }
}

/// One drawing operation. Paths are in local space and `transform` maps them to page space;
/// gradient coordinates are in the same local space as the path.
#[derive(Clone, PartialEq, Debug)]
pub enum DisplayItem {
    Fill {
        path: BezPath,
        transform: Affine,
        paint: Paint,
        rule: FillRule,
    },
    Stroke {
        path: BezPath,
        transform: Affine,
        stroke: Stroke,
    },
    /// A raster image stretched to the box `(0, 0) - size`.
    Image {
        asset: AssetId,
        size: Size,
        transform: Affine,
    },
    /// Starts a group composited as a whole with the given opacity and blend mode.
    PushLayer {
        opacity: f32,
        blend: BlendMode,
    },
    /// Ends the most recent [`DisplayItem::PushLayer`].
    PopLayer,
    /// Hides everything outside `path` (non-zero fill) until the matching
    /// [`DisplayItem::PopClip`].
    PushClip {
        path: BezPath,
        transform: Affine,
    },
    PopClip,
}
