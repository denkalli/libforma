//! Geometry types (re-exported from [`kurbo`]) and helpers for primitive shapes.

use std::f64::consts::{FRAC_PI_2, TAU};

pub use kurbo::{
    Affine, BezPath, Circle, Ellipse, Line, PathEl, Point, Rect, RoundedRect, Shape, Size, Vec2,
};

/// Tolerance used when converting curves to Bézier paths.
pub const TOLERANCE: f64 = 0.1;

/// Builds a regular polygon inscribed in the box `(0, 0) - size`, with the first vertex at
/// the top.
pub fn regular_polygon(size: Size, sides: u32) -> BezPath {
    let sides = sides.max(3);
    let points = (0..sides).map(|i| {
        let angle = -FRAC_PI_2 + TAU * f64::from(i) / f64::from(sides);
        unit_to_box(size, 1.0, angle)
    });
    closed_polyline(points)
}

/// Builds a star inscribed in the box `(0, 0) - size`. `inner_ratio` is the radius of the
/// inner vertices relative to the outer ones.
pub fn star(size: Size, points: u32, inner_ratio: f64) -> BezPath {
    let points = points.max(3);
    let inner_ratio = inner_ratio.clamp(0.0, 1.0);
    let count = points * 2;
    let vertices = (0..count).map(|i| {
        let angle = -FRAC_PI_2 + TAU * f64::from(i) / f64::from(count);
        let radius = if i % 2 == 0 { 1.0 } else { inner_ratio };
        unit_to_box(size, radius, angle)
    });
    closed_polyline(vertices)
}

fn unit_to_box(size: Size, radius: f64, angle: f64) -> Point {
    let (sin, cos) = angle.sin_cos();
    Point::new(
        size.width * 0.5 * (1.0 + radius * cos),
        size.height * 0.5 * (1.0 + radius * sin),
    )
}

fn closed_polyline(mut points: impl Iterator<Item = Point>) -> BezPath {
    let mut path = BezPath::new();
    if let Some(first) = points.next() {
        path.move_to(first);
        for p in points {
            path.line_to(p);
        }
        path.close_path();
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polygon_fits_its_box() {
        let size = Size::new(100.0, 50.0);
        let bbox = regular_polygon(size, 6).bounding_box();
        assert!(bbox.x0 >= -1e-9 && bbox.y0 >= -1e-9);
        assert!(bbox.x1 <= size.width + 1e-9 && bbox.y1 <= size.height + 1e-9);
    }

    #[test]
    fn star_has_two_vertices_per_point() {
        let path = star(Size::new(10.0, 10.0), 5, 0.5);
        let lines = path
            .elements()
            .iter()
            .filter(|el| matches!(el, PathEl::LineTo(_)))
            .count();
        assert_eq!(lines, 9);
    }
}
