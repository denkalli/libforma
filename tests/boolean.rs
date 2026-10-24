use libforma::boolean::{BooleanOp, combine_nodes};
use libforma::geom::Shape;
use libforma::history::History;
use libforma::model::{Geometry, NodeLocation};
use libforma::paint::Color;
use libforma::{Document, Node, NodeKind, Page};

fn shape_area(node: &Node) -> f64 {
    let NodeKind::Shape(shape) = &node.kind else {
        panic!("not a shape")
    };
    shape.geometry.to_path().area().abs()
}

#[test]
fn combining_nodes_replaces_them_and_can_be_undone() {
    let back = Node::shape(Geometry::rect(10.0, 10.0))
        .with_fill(Color::rgb(255, 0, 0))
        .with_name("back");
    let front = Node::shape(Geometry::ellipse(10.0, 10.0))
        .at(5.0, 0.0)
        .with_fill(Color::BLACK);
    let other = Node::shape(Geometry::rect(1.0, 1.0)).at(50.0, 50.0);
    let ids = [front.id, back.id];
    let page = Page::new("p")
        .with_child(other)
        .with_child(back)
        .with_child(front);
    let page_id = page.id;
    let mut doc = Document::new("t").with_page(page);
    let original = doc.clone();

    let command = combine_nodes(&doc, &ids, BooleanOp::Subtract).unwrap();
    let mut history = History::new();
    history.execute(&mut doc, command).unwrap();

    let children = &doc.pages[0].children;
    assert_eq!(children.len(), 2);
    let result = &children[1];
    assert_eq!(result.name, "back");
    assert_eq!(
        doc.locate(result.id),
        Some(NodeLocation::top_level(page_id, 1))
    );
    // Square minus the left half of a circle of radius 5 placed at x = 5.
    let expected = 100.0 - std::f64::consts::PI * 25.0 / 2.0;
    assert!(
        (shape_area(result) - expected).abs() < 0.1,
        "{}",
        shape_area(result)
    );
    // The path is stored with its bounding box at the origin.
    let NodeKind::Shape(shape) = &result.kind else {
        unreachable!()
    };
    let bbox = shape.geometry.bounds();
    assert!(bbox.x0.abs() < 1e-6 && bbox.y0.abs() < 1e-6, "{bbox:?}");

    history.undo(&mut doc).unwrap();
    assert_eq!(doc, original);
}

#[test]
fn rejects_mixed_parents_and_non_shapes() {
    let a = Node::shape(Geometry::rect(1.0, 1.0));
    let b = Node::shape(Geometry::rect(1.0, 1.0));
    let ids = [a.id, b.id];
    let group = Node::group(vec![b]);
    let group_id = group.id;
    let doc = Document::new("t").with_page(Page::new("p").with_child(a).with_child(group));
    assert!(combine_nodes(&doc, &ids, BooleanOp::Union).is_err());
    assert!(combine_nodes(&doc, &[ids[0], group_id], BooleanOp::Union).is_err());
    assert!(combine_nodes(&doc, &[ids[0]], BooleanOp::Union).is_err());
}
