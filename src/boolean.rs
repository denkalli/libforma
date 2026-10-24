//! Boolean operations on shapes: union, subtraction, intersection and exclusion.
//!
//! Curves stay curves: the result is an exact Bézier path, not a polygon approximation.
//! linesweeper is used internally; none of its types are part of this API.

use kurbo::{Affine, BezPath, PathEl, Shape};

use crate::error::{Error, Result};
use crate::history::Command;
use crate::id::NodeId;
use crate::model::{Document, Geometry, Node, NodeKind, NodeLocation};
use crate::paint::FillRule;

/// A boolean operation between shapes.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum BooleanOp {
    /// Area covered by any shape.
    Union,
    /// Area of the first shape not covered by the others.
    Subtract,
    /// Area covered by all shapes.
    Intersect,
    /// Area covered by an odd number of shapes.
    Exclude,
}

/// Combines two closed paths.
///
/// Open subpaths are closed first. The result uses the non-zero fill rule (it fills the same
/// with either rule).
pub fn combine_paths(a: &BezPath, b: &BezPath, rule: FillRule, op: BooleanOp) -> Result<BezPath> {
    let op = match op {
        BooleanOp::Union => linesweeper::BinaryOp::Union,
        BooleanOp::Subtract => linesweeper::BinaryOp::Difference,
        BooleanOp::Intersect => linesweeper::BinaryOp::Intersection,
        BooleanOp::Exclude => linesweeper::BinaryOp::Xor,
    };
    let rule = match rule {
        FillRule::NonZero => linesweeper::FillRule::NonZero,
        FillRule::EvenOdd => linesweeper::FillRule::EvenOdd,
    };
    let contours = linesweeper::binary_op(&closed(a), &closed(b), rule, op)
        .map_err(|e| Error::InvalidLocation(format!("boolean operation failed: {e}")))?;
    let mut out = BezPath::new();
    for contour in contours.contours() {
        out.extend(contour.path.iter());
    }
    Ok(out)
}

/// Closes every subpath, so a path can take part in a boolean operation.
fn closed(path: &BezPath) -> BezPath {
    let mut out = BezPath::new();
    let mut open = false;
    for el in path.elements() {
        match el {
            PathEl::MoveTo(_) => {
                if open {
                    out.close_path();
                }
                open = true;
            }
            PathEl::ClosePath => open = false,
            _ => {}
        }
        out.push(*el);
    }
    if open {
        out.close_path();
    }
    out
}

/// Builds the command that replaces shapes with the result of a boolean operation.
///
/// All nodes must be shapes with the same parent. They are combined back to front (for
/// [`BooleanOp::Subtract`], the backmost shape is the one cut). The result takes the style,
/// name and position in the tree of the backmost shape. Execute the command through
/// [`History`](crate::history::History) to make it undoable.
pub fn combine_nodes(doc: &Document, ids: &[NodeId], op: BooleanOp) -> Result<Command> {
    if ids.len() < 2 {
        return Err(Error::InvalidLocation(
            "a boolean operation needs at least two shapes".into(),
        ));
    }
    let mut located: Vec<(NodeLocation, &Node)> = Vec::with_capacity(ids.len());
    for &id in ids {
        let location = doc.locate(id).ok_or(Error::NodeNotFound(id))?;
        let node = doc.node(id).ok_or(Error::NodeNotFound(id))?;
        if !matches!(node.kind, NodeKind::Shape(_)) {
            return Err(Error::InvalidLocation(format!("node {id} is not a shape")));
        }
        if let Some((first, _)) = located.first()
            && (first.page, first.parent) != (location.page, location.parent)
        {
            return Err(Error::InvalidLocation(
                "all shapes must have the same parent".into(),
            ));
        }
        located.push((location, node));
    }
    located.sort_by_key(|(location, _)| location.index);

    // Work in the parent's space.
    let parent_path = |node: &Node| {
        let NodeKind::Shape(shape) = &node.kind else {
            unreachable!()
        };
        (node.transform * shape.geometry.to_path(), shape.fill_rule)
    };
    let (first, first_rule) = parent_path(located[0].1);
    // Each operand is normalised with its own fill rule, then they are combined under
    // non-zero.
    let mut result = normalize(&first, first_rule)?;
    for (_, node) in &located[1..] {
        let (path, rule) = parent_path(node);
        result = combine_paths(&result, &normalize(&path, rule)?, FillRule::NonZero, op)?;
    }

    // Store the path with its bounding box at the local origin, like other shapes.
    let origin = result.bounding_box().origin().to_vec2();
    let (back_location, back) = located[0];
    let mut combined = back.clone();
    combined.id = NodeId::new();
    combined.transform = Affine::translate(origin);
    if let NodeKind::Shape(shape) = &mut combined.kind {
        shape.geometry = Geometry::Path {
            path: Affine::translate(-origin) * result,
        };
        shape.fill_rule = FillRule::NonZero;
    }

    let mut commands: Vec<Command> = located
        .iter()
        .rev()
        .map(|(_, node)| Command::RemoveNode { id: node.id })
        .collect();
    commands.push(Command::InsertNode {
        location: back_location,
        node: combined,
    });
    Ok(Command::Batch(commands))
}

/// Rewrites a path so it fills the same area under the non-zero rule.
fn normalize(path: &BezPath, rule: FillRule) -> Result<BezPath> {
    combine_paths(path, &BezPath::new(), rule, BooleanOp::Union)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::Rect;

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> BezPath {
        Rect::new(x0, y0, x1, y1).to_path(0.1)
    }

    #[test]
    fn areas_of_two_overlapping_squares() {
        let a = rect(0.0, 0.0, 10.0, 10.0);
        let b = rect(5.0, 5.0, 15.0, 15.0);
        let area = |op| {
            combine_paths(&a, &b, FillRule::NonZero, op)
                .unwrap()
                .area()
                .abs()
        };
        assert!((area(BooleanOp::Union) - 175.0).abs() < 1e-6);
        assert!((area(BooleanOp::Subtract) - 75.0).abs() < 1e-6);
        assert!((area(BooleanOp::Intersect) - 25.0).abs() < 1e-6);
        assert!((area(BooleanOp::Exclude) - 150.0).abs() < 1e-6);
    }

    #[test]
    fn disjoint_intersection_is_empty() {
        let a = rect(0.0, 0.0, 1.0, 1.0);
        let b = rect(5.0, 5.0, 6.0, 6.0);
        let out = combine_paths(&a, &b, FillRule::NonZero, BooleanOp::Intersect).unwrap();
        assert!(out.is_empty());
    }
}
