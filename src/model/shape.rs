use kurbo::{BezPath, Ellipse, Line, Point, Rect, RoundedRect, Shape, Size};
use serde::{Deserialize, Serialize};

use crate::geom::{self, TOLERANCE};
use crate::paint::{FillRule, Paint, Stroke};
use crate::serde_util;

/// A vector shape with optional fill and stroke.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct ShapeNode {
    pub geometry: Geometry,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<Paint>,
    #[serde(default, skip_serializing_if = "serde_util::is_default")]
    pub fill_rule: FillRule,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke: Option<Stroke>,
}

impl ShapeNode {
    pub fn new(geometry: Geometry) -> Self {
        Self {
            geometry,
            fill: None,
            fill_rule: FillRule::NonZero,
            stroke: None,
        }
    }
}

/// The outline of a shape, in the node's local space.
///
/// Parametric shapes stay editable (a rectangle keeps its corner radius, a star its number
/// of points); [`Geometry::Path`] holds arbitrary Bézier paths.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Geometry {
    Rect {
        #[serde(with = "serde_util::size")]
        size: Size,
        #[serde(default, skip_serializing_if = "serde_util::is_zero")]
        corner_radius: f64,
    },
    Ellipse {
        #[serde(with = "serde_util::size")]
        size: Size,
    },
    Polygon {
        #[serde(with = "serde_util::size")]
        size: Size,
        sides: u32,
    },
    Star {
        #[serde(with = "serde_util::size")]
        size: Size,
        points: u32,
        inner_ratio: f64,
    },
    /// A straight line from the local origin to `end`.
    Line {
        #[serde(with = "serde_util::point")]
        end: Point,
    },
    Path {
        #[serde(with = "serde_util::bez_path")]
        path: BezPath,
    },
}

impl Geometry {
    pub fn rect(width: f64, height: f64) -> Self {
        Geometry::Rect {
            size: Size::new(width, height),
            corner_radius: 0.0,
        }
    }

    pub fn rounded_rect(width: f64, height: f64, corner_radius: f64) -> Self {
        Geometry::Rect {
            size: Size::new(width, height),
            corner_radius,
        }
    }

    pub fn ellipse(width: f64, height: f64) -> Self {
        Geometry::Ellipse {
            size: Size::new(width, height),
        }
    }

    /// Converts the geometry to a Bézier path in local space.
    pub fn to_path(&self) -> BezPath {
        match self {
            Geometry::Rect {
                size,
                corner_radius,
            } => {
                let rect = size.to_rect();
                if *corner_radius > 0.0 {
                    RoundedRect::from_rect(rect, *corner_radius).to_path(TOLERANCE)
                } else {
                    rect.to_path(TOLERANCE)
                }
            }
            Geometry::Ellipse { size } => Ellipse::from_rect(size.to_rect()).to_path(TOLERANCE),
            Geometry::Polygon { size, sides } => geom::regular_polygon(*size, *sides),
            Geometry::Star {
                size,
                points,
                inner_ratio,
            } => geom::star(*size, *points, *inner_ratio),
            Geometry::Line { end } => Line::new(Point::ORIGIN, *end).to_path(TOLERANCE),
            Geometry::Path { path } => path.clone(),
        }
    }

    /// Bounding box in local space, ignoring strokes.
    pub fn bounds(&self) -> Rect {
        match self {
            Geometry::Rect { size, .. }
            | Geometry::Ellipse { size }
            | Geometry::Polygon { size, .. }
            | Geometry::Star { size, .. } => size.to_rect(),
            Geometry::Line { end } => Rect::from_points(Point::ORIGIN, *end),
            Geometry::Path { path } => path.bounding_box(),
        }
    }

    /// Whether the geometry is an open line that cannot be filled.
    pub fn is_open(&self) -> bool {
        matches!(self, Geometry::Line { .. })
    }
}
