//! Geometric queries on a page: bounds, hit testing and area selection.
//!
//! An editor uses these to know what is under the pointer and what a selection rectangle
//! covers.

use kurbo::{Affine, ParamCurveNearest, Point, Rect, Shape};

use crate::id::NodeId;
use crate::model::{Node, NodeKind, Page};
use crate::text::TextEngine;

/// Bounding box of a node in its own local space, ignoring strokes.
///
/// Groups use the union of their visible children; an empty group has an empty box.
pub fn local_bounds(node: &Node, text: &mut TextEngine) -> Rect {
    match &node.kind {
        NodeKind::Shape(shape) => shape.geometry.bounds(),
        NodeKind::Text(t) => text.box_size(t).to_rect(),
        NodeKind::Image(image) => image.size.to_rect(),
        NodeKind::Frame(frame) => frame.size.to_rect(),
        NodeKind::Group { children } => {
            let mut bounds: Option<Rect> = None;
            for child in children.iter().filter(|c| c.visible) {
                let child_bounds =
                    (child.transform * local_bounds(child, text).to_path(0.1)).bounding_box();
                bounds = Some(bounds.map_or(child_bounds, |b| b.union(child_bounds)));
            }
            bounds.unwrap_or(Rect::ZERO)
        }
    }
}

/// Transform from a node's local space to page space (the product of its ancestors'
/// transforms and its own). `None` if the node is not on the page.
pub fn page_transform(page: &Page, id: NodeId) -> Option<Affine> {
    fn search(nodes: &[Node], parent: Affine, id: NodeId) -> Option<Affine> {
        nodes.iter().find_map(|node| {
            let transform = parent * node.transform;
            if node.id == id {
                Some(transform)
            } else {
                search(node.children(), transform, id)
            }
        })
    }
    search(&page.children, Affine::IDENTITY, id)
}

/// Axis-aligned bounding box of a node in page space.
pub fn page_bounds(page: &Page, id: NodeId, text: &mut TextEngine) -> Option<Rect> {
    let node = page.find(id)?;
    let transform = page_transform(page, id)?;
    Some((transform * local_bounds(node, text).to_path(0.1)).bounding_box())
}

/// Finds what is under `point` (page space), front to back.
///
/// Returns the chain of ids from the top-level node down to the leaf that was hit, so the
/// caller can choose the depth to select (a click usually selects the top-level node; a
/// double click enters groups). Hidden and locked nodes are skipped. `tolerance` widens thin
/// targets such as lines, in page units.
pub fn hit_test(
    page: &Page,
    point: Point,
    tolerance: f64,
    text: &mut TextEngine,
) -> Option<Vec<NodeId>> {
    fn search(
        nodes: &[Node],
        parent: Affine,
        point: Point,
        tolerance: f64,
        text: &mut TextEngine,
    ) -> Option<Vec<NodeId>> {
        for node in nodes.iter().rev() {
            if !node.visible || node.locked {
                continue;
            }
            let transform = parent * node.transform;
            let hit = match &node.kind {
                NodeKind::Frame(frame) => {
                    let local = (transform.determinant().abs() > 1e-12)
                        .then(|| transform.inverse() * point);
                    let inside = local.is_some_and(|p| frame.size.to_rect().contains(p));
                    // Clipped children are only hit where they are visible.
                    let children = if frame.clip && !inside {
                        None
                    } else {
                        search(&frame.children, transform, point, tolerance, text)
                    };
                    match children {
                        Some(mut chain) => {
                            chain.insert(0, node.id);
                            Some(chain)
                        }
                        None => inside.then(|| vec![node.id]),
                    }
                }
                NodeKind::Group { children } => search(children, transform, point, tolerance, text)
                    .map(|mut chain| {
                        chain.insert(0, node.id);
                        chain
                    }),
                _ => node_contains(node, transform, point, tolerance, text).then(|| vec![node.id]),
            };
            if hit.is_some() {
                return hit;
            }
        }
        None
    }
    search(&page.children, Affine::IDENTITY, point, tolerance, text)
}

fn node_contains(
    node: &Node,
    transform: Affine,
    point: Point,
    tolerance: f64,
    text: &mut TextEngine,
) -> bool {
    if transform.determinant().abs() < 1e-12 {
        return false;
    }
    let local = transform.inverse() * point;
    // Tolerance in local units, assuming roughly uniform scaling.
    let scale = transform.determinant().abs().sqrt();
    let tol = tolerance / scale;
    match &node.kind {
        NodeKind::Shape(shape) => {
            let path = shape.geometry.to_path();
            let filled = shape.fill.is_some() && !shape.geometry.is_open();
            if filled {
                let winding = path.winding(local);
                let inside = match shape.fill_rule {
                    crate::paint::FillRule::NonZero => winding != 0,
                    crate::paint::FillRule::EvenOdd => winding % 2 != 0,
                };
                if inside {
                    return true;
                }
            }
            let half_width = shape.stroke.as_ref().map_or(0.0, |s| s.width / 2.0);
            let reach = half_width + tol;
            let near_outline = path
                .segments()
                .any(|seg| seg.nearest(local, 1e-3).distance_sq <= reach * reach);
            // Shapes without fill or stroke are still selectable by their outline.
            near_outline
        }
        NodeKind::Text(_) | NodeKind::Image(_) => {
            local_bounds(node, text).inflate(tol, tol).contains(local)
        }
        NodeKind::Group { .. } | NodeKind::Frame(_) => false,
    }
}

/// Union of the page-space bounds of the visible top-level nodes, or `None` for an empty page.
pub fn content_bounds(page: &Page, text: &mut TextEngine) -> Option<Rect> {
    page.children
        .iter()
        .filter(|n| n.visible)
        .map(|n| (n.transform * local_bounds(n, text).to_path(0.1)).bounding_box())
        .reduce(|a, b| a.union(b))
}

/// Ids of the top-level frames of a page, back to front.
pub fn frames(page: &Page) -> Vec<NodeId> {
    page.children
        .iter()
        .filter(|n| n.is_frame())
        .map(|n| n.id)
        .collect()
}

/// Top-level nodes whose bounds intersect `rect` (page space), back to front. Hidden and
/// locked nodes are skipped.
pub fn nodes_in_rect(page: &Page, rect: Rect, text: &mut TextEngine) -> Vec<NodeId> {
    page.children
        .iter()
        .filter(|n| n.visible && !n.locked)
        .filter(|n| {
            let bounds = (n.transform * local_bounds(n, text).to_path(0.1)).bounding_box();
            // Closed-interval overlap, so lines (zero-height boxes) can be selected too.
            bounds.x0 <= rect.x1
                && bounds.x1 >= rect.x0
                && bounds.y0 <= rect.y1
                && bounds.y1 >= rect.y0
        })
        .map(|n| n.id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Geometry;
    use crate::paint::{Color, Stroke};

    fn page() -> (Page, NodeId, NodeId, NodeId, NodeId) {
        let back = Node::shape(Geometry::rect(100.0, 100.0)).with_fill(Color::BLACK);
        let inner = Node::shape(Geometry::ellipse(20.0, 20.0))
            .at(10.0, 10.0)
            .with_fill(Color::WHITE);
        let group = Node::group(vec![inner.clone()]).at(50.0, 50.0);
        let line = Node::shape(Geometry::Line {
            end: Point::new(100.0, 0.0),
        })
        .at(0.0, 200.0)
        .with_stroke(Stroke::new(Color::BLACK, 2.0));
        let ids = (back.id, group.id, inner.id, line.id);
        let page = Page::new("p")
            .with_child(back)
            .with_child(group)
            .with_child(line);
        (page, ids.0, ids.1, ids.2, ids.3)
    }

    #[test]
    fn hit_test_returns_front_most_chain() {
        let (page, back, group, inner, line) = page();
        let mut text = TextEngine::new();
        assert_eq!(
            hit_test(&page, Point::new(70.0, 70.0), 0.0, &mut text),
            Some(vec![group, inner])
        );
        assert_eq!(
            hit_test(&page, Point::new(5.0, 5.0), 0.0, &mut text),
            Some(vec![back])
        );
        assert_eq!(
            hit_test(&page, Point::new(50.0, 201.5), 1.0, &mut text),
            Some(vec![line])
        );
        assert_eq!(
            hit_test(&page, Point::new(50.0, 210.0), 1.0, &mut text),
            None
        );
    }

    #[test]
    fn locked_and_hidden_nodes_are_skipped() {
        let (mut page, back, group, ..) = page();
        page.find_mut(group).unwrap().visible = false;
        assert_eq!(
            hit_test(&page, Point::new(70.0, 70.0), 0.0, &mut TextEngine::new()),
            Some(vec![back])
        );
        page.find_mut(back).unwrap().locked = true;
        assert_eq!(
            hit_test(&page, Point::new(70.0, 70.0), 0.0, &mut TextEngine::new()),
            None
        );
    }

    #[test]
    fn bounds_and_transforms_compose() {
        let (page, _, group, inner, _) = page();
        let mut text = TextEngine::new();
        assert_eq!(
            page_transform(&page, inner),
            Some(Affine::translate((60.0, 60.0)))
        );
        assert_eq!(
            page_bounds(&page, inner, &mut text),
            Some(Rect::new(60.0, 60.0, 80.0, 80.0))
        );
        assert_eq!(
            page_bounds(&page, group, &mut text),
            Some(Rect::new(60.0, 60.0, 80.0, 80.0))
        );
    }

    #[test]
    fn rect_selection_picks_top_level_nodes() {
        let (page, back, group, _, line) = page();
        let mut text = TextEngine::new();
        assert_eq!(
            nodes_in_rect(&page, Rect::new(55.0, 55.0, 65.0, 65.0), &mut text),
            vec![back, group]
        );
        assert_eq!(
            nodes_in_rect(&page, Rect::new(0.0, 190.0, 10.0, 210.0), &mut text),
            vec![line]
        );
    }

    #[test]
    fn frames_clip_hits_and_report_their_area() {
        let inside = Node::shape(Geometry::rect(10.0, 10.0)).with_fill(Color::BLACK);
        let overflowing = Node::shape(Geometry::rect(50.0, 10.0))
            .at(80.0, 50.0)
            .with_fill(Color::BLACK);
        let frame = Node::frame("f", 100.0, 100.0)
            .at(200.0, 0.0)
            .with_child(inside.clone())
            .with_child(overflowing.clone());
        let fid = frame.id;
        let page = Page::new("p").with_child(frame);
        let mut text = TextEngine::new();
        let hit = |p: (f64, f64), text: &mut TextEngine| hit_test(&page, Point::from(p), 0.0, text);
        assert_eq!(hit((205.0, 5.0), &mut text), Some(vec![fid, inside.id]));
        assert_eq!(hit((250.0, 80.0), &mut text), Some(vec![fid]));
        assert_eq!(
            hit((290.0, 55.0), &mut text),
            Some(vec![fid, overflowing.id])
        );
        // The part of the child outside the frame is hidden, so it cannot be clicked.
        assert_eq!(hit((320.0, 55.0), &mut text), None);
        assert_eq!(
            page_bounds(&page, fid, &mut text),
            Some(Rect::new(200.0, 0.0, 300.0, 100.0))
        );
        assert_eq!(
            content_bounds(&page, &mut text),
            Some(Rect::new(200.0, 0.0, 300.0, 100.0))
        );
        assert_eq!(frames(&page), vec![fid]);
    }

    #[test]
    fn content_bounds_skip_hidden_nodes_and_frames_skip_other_nodes() {
        let mut text = TextEngine::new();
        assert_eq!(content_bounds(&Page::new("vazia"), &mut text), None);

        let shape = Node::shape(Geometry::rect(10.0, 10.0)).at(-20.0, 0.0);
        let mut hidden = Node::shape(Geometry::rect(10.0, 10.0)).at(500.0, 500.0);
        hidden.visible = false;
        let frame = Node::frame("f", 100.0, 50.0).at(0.0, 10.0);
        let fid = frame.id;
        let page = Page::new("p")
            .with_child(shape)
            .with_child(frame)
            .with_child(hidden);
        assert_eq!(
            content_bounds(&page, &mut text),
            Some(Rect::new(-20.0, 0.0, 100.0, 60.0))
        );
        assert_eq!(frames(&page), vec![fid]);
    }
}
