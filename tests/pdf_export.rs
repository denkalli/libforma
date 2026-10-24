#![cfg(feature = "pdf")]

use libforma::export::pdf;
use libforma::geom::Point;
use libforma::model::Geometry;
use libforma::paint::{BlendMode, Color, GradientStop, Paint, Stroke};
use libforma::scene::SceneBuilder;
use libforma::{Document, Node, Page};

#[test]
fn one_pdf_page_per_frame() {
    let mut doc = Document::new("t");
    let pixel = doc
        .assets
        .insert_image(include_bytes!("data/pixel.png").to_vec())
        .unwrap();
    let gradient = Paint::RadialGradient {
        center: Point::new(5.0, 5.0),
        radius: 5.0,
        stops: vec![
            GradientStop::new(0.0, Color::WHITE),
            GradientStop::new(1.0, Color::BLACK.with_alpha(0)),
        ],
    };
    let first = Node::frame("A", 200.0, 100.0)
        .with_child(
            Node::shape(Geometry::rect(50.0, 50.0))
                .with_fill(gradient)
                .with_stroke(Stroke::new(Color::BLACK, 1.0)),
        )
        .with_child(
            Node::group(vec![
                Node::shape(Geometry::ellipse(10.0, 10.0)).with_fill(Color::rgba(0, 0, 255, 100)),
            ])
            .with_opacity(0.5)
            .with_blend(BlendMode::Screen),
        )
        .with_child(Node::image(pixel, (20.0, 20.0)).at(100.0, 10.0));
    let second = Node::frame("B", 400.0, 300.0).at(300.0, 0.0);
    let ids = [first.id, second.id];
    let page = Page::new("p").with_child(first).with_child(second);
    let page_id = page.id;
    let doc = doc.with_page(page);

    let bytes = pdf::export_nodes(&doc, page_id, &ids, &mut SceneBuilder::new()).unwrap();
    assert!(bytes.starts_with(b"%PDF-"));
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("/Type/Pages/Count 2"), "page count");
    // 200×100 px = 150×75 pt.
    assert!(text.contains("/MediaBox[0 0 150 75]"), "media box");
    assert!(text.contains("/MediaBox[0 0 300 225]"), "media box");

    let only_second =
        pdf::export_nodes(&doc, page_id, &ids[1..], &mut SceneBuilder::new()).unwrap();
    assert!(String::from_utf8_lossy(&only_second).contains("/Type/Pages/Count 1"));
}
