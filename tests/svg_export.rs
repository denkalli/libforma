use libforma::export::svg;
use libforma::geom::{Point, Rect};
use libforma::model::Geometry;
use libforma::paint::{BlendMode, Color, GradientStop, LineCap, Paint, Stroke};
use libforma::scene::SceneBuilder;
use libforma::{Document, Node, NodeId, Page};

/// Exports a frame holding `children` (frame at `(10, 20)`, 100 × 80).
fn export_frame(children: Vec<Node>) -> String {
    let mut frame = Node::frame("Quadro", 100.0, 80.0).at(10.0, 20.0);
    for child in children {
        frame = frame.with_child(child);
    }
    export_node(Document::new("t"), frame)
}

fn export_node(doc: Document, node: Node) -> String {
    let id = node.id;
    let page = Page::new("p").with_child(node);
    let page_id = page.id;
    let doc = doc.with_page(page);
    svg::export_nodes(&doc, page_id, &[id], &mut SceneBuilder::new())
        .unwrap()
        .remove(0)
}

fn parse(svg: &str) -> usvg::Tree {
    usvg::Tree::from_str(svg, &usvg::Options::default()).expect("exported SVG must parse")
}

#[test]
fn frame_sets_size_viewbox_fill_and_clip() {
    let svg = export_frame(vec![]);
    assert!(
        svg.contains(r#"width="100" height="80" viewBox="10 20 100 80""#),
        "{svg}"
    );
    assert!(svg.contains(r##"fill="#ffffff""##), "{svg}");
    assert!(svg.contains(r#"<g clip-path="url(#clip1)">"#), "{svg}");
    let tree = parse(&svg);
    assert_eq!(tree.size().width(), 100.0);
}

#[test]
fn shapes_fills_and_strokes() {
    let mut stroke = Stroke::new(Color::rgba(255, 0, 0, 128), 3.0);
    stroke.cap = LineCap::Round;
    stroke.dash = vec![4.0, 2.0];
    let svg = export_frame(vec![
        Node::shape(Geometry::rect(10.0, 20.0))
            .at(5.0, 5.0)
            .with_fill(Color::rgb(0, 128, 255)),
        Node::shape(Geometry::ellipse(30.0, 30.0)).with_stroke(stroke),
        // Lines are never filled.
        Node::shape(Geometry::Line {
            end: Point::new(50.0, 50.0),
        })
        .with_fill(Color::BLACK),
    ]);
    // Children are placed in page space: frame (10, 20) + child (5, 5).
    assert!(
        svg.contains(r##"transform="translate(15 25)" fill="#0080ff""##),
        "{svg}"
    );
    assert!(svg.contains(r##"stroke="#ff0000" stroke-opacity="0.502" stroke-width="3" stroke-linecap="round" stroke-dasharray="4 2""##), "{svg}");
    // Frame fill, rectangle, ellipse stroke; the clip path is in <defs>.
    assert_eq!(svg.matches("<path").count(), 4, "{svg}");
    parse(&svg);
}

#[test]
fn gradients_go_to_defs() {
    let paint = Paint::LinearGradient {
        start: Point::new(0.0, 0.0),
        end: Point::new(10.0, 0.0),
        stops: vec![
            GradientStop::new(0.0, Color::BLACK),
            GradientStop::new(1.0, Color::WHITE.with_alpha(0)),
        ],
    };
    let svg = export_node(
        Document::new("t"),
        Node::shape(Geometry::rect(10.0, 10.0)).with_fill(paint),
    );
    assert!(svg.contains("<linearGradient id=\"gradient1\""), "{svg}");
    assert!(svg.contains(r#"fill="url(#gradient1)""#), "{svg}");
    assert!(svg.contains(r#"stop-opacity="0""#), "{svg}");
    parse(&svg);
}

#[test]
fn layers_and_hidden_nodes() {
    let mut hidden = Node::shape(Geometry::rect(1.0, 1.0)).with_fill(Color::BLACK);
    hidden.visible = false;
    let group = Node::group(vec![
        Node::shape(Geometry::rect(5.0, 5.0)).with_fill(Color::BLACK),
        hidden,
    ])
    .with_opacity(0.5)
    .with_blend(BlendMode::Multiply);
    let svg = export_node(Document::new("t"), group);
    assert!(
        svg.contains(r#"<g opacity="0.5" style="mix-blend-mode:multiply">"#),
        "{svg}"
    );
    assert_eq!(svg.matches("<path").count(), 1, "{svg}");
    parse(&svg);
}

#[test]
fn images_are_embedded() {
    const PNG: &[u8] = include_bytes!("data/pixel.png");
    let mut doc = Document::new("t");
    let asset = doc.assets.insert_image(PNG.to_vec()).unwrap();
    let svg = export_node(doc, Node::image(asset, (10.0, 10.0)));
    assert!(
        svg.contains(r#"href="data:image/png;base64,iVBORw0KGgo"#),
        "{svg}"
    );
    parse(&svg);
}

#[test]
fn area_export_covers_everything_in_it() {
    let a = Node::shape(Geometry::rect(10.0, 10.0)).with_fill(Color::BLACK);
    let b = Node::shape(Geometry::rect(10.0, 10.0))
        .at(100.0, 0.0)
        .with_fill(Color::BLACK);
    let page = Page::new("p").with_child(a).with_child(b);
    let page_id = page.id;
    let doc = Document::new("t").with_page(page);
    let svg = svg::export_area(
        &doc,
        page_id,
        Rect::new(-5.0, -5.0, 115.0, 15.0),
        &mut SceneBuilder::new(),
    )
    .unwrap();
    assert!(svg.contains(r#"viewBox="-5 -5 120 20""#), "{svg}");
    assert_eq!(svg.matches("<path").count(), 2, "{svg}");
    let _: NodeId = doc.pages[0].children[0].id;
}
