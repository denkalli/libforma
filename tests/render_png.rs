#![cfg(feature = "render")]

use libforma::export::png;
use libforma::geom::Rect;
use libforma::model::Geometry;
use libforma::paint::{BlendMode, Color};
use libforma::render::{OffscreenRenderer, RgbaImage};
use libforma::scene::SceneBuilder;
use libforma::{Document, Node, Page};

/// Returns `None` (and the test passes) on machines without a usable GPU, such as CI.
fn renderer() -> Option<OffscreenRenderer> {
    match OffscreenRenderer::new() {
        Ok(r) => Some(r),
        Err(err) => {
            eprintln!("skipping GPU test: {err}");
            None
        }
    }
}

/// Renders the first node of the first page.
fn render(doc: &Document, scale: f64, renderer: &mut OffscreenRenderer) -> RgbaImage {
    let page = &doc.pages[0];
    let list = SceneBuilder::new()
        .build_node(doc, page, page.children[0].id)
        .unwrap();
    renderer.render_image(&list, &doc.assets, scale).unwrap()
}

fn assert_close(actual: [u8; 4], expected: [u8; 4]) {
    let ok = actual.iter().zip(expected).all(|(a, e)| a.abs_diff(e) <= 2);
    assert!(ok, "expected {expected:?}, got {actual:?}");
}

#[test]
fn renders_a_frame_with_shapes_layers_and_images() {
    let Some(mut renderer) = renderer() else {
        return;
    };

    let mut doc = Document::new("t");
    let pixel = doc
        .assets
        .insert_image(include_bytes!("data/pixel.png").to_vec())
        .unwrap();
    let frame = Node::frame("Quadro", 100.0, 100.0)
        .at(500.0, -300.0)
        .with_child(Node::shape(Geometry::rect(40.0, 40.0)).with_fill(Color::rgb(255, 0, 0)))
        .with_child(
            Node::shape(Geometry::rect(40.0, 40.0))
                .at(50.0, 0.0)
                .with_fill(Color::rgb(0, 0, 255))
                .with_opacity(0.5),
        )
        .with_child(
            Node::shape(Geometry::rect(40.0, 40.0))
                .at(0.0, 50.0)
                .with_fill(Color::rgb(128, 128, 128))
                .with_blend(BlendMode::Multiply),
        )
        .with_child(Node::image(pixel, (40.0, 40.0)).at(50.0, 50.0))
        // Mostly outside the frame: clipped.
        .with_child(
            Node::shape(Geometry::rect(40.0, 40.0))
                .at(95.0, 95.0)
                .with_fill(Color::BLACK),
        );
    let doc = doc.with_page(Page::new("p").with_child(frame));

    let image = render(&doc, 1.0, &mut renderer);
    assert_eq!((image.width, image.height), (100, 100));
    assert_close(image.pixel(20, 20), [255, 0, 0, 255]);
    assert_close(image.pixel(70, 20), [127, 127, 255, 255]);
    assert_close(image.pixel(20, 70), [128, 128, 128, 255]);
    // tests/data/pixel.png is an opaque orange pixel.
    assert_close(image.pixel(70, 70), [255, 128, 0, 255]);
    // Frame fill, and the clipped square's visible corner.
    assert_close(image.pixel(92, 45), [255, 255, 255, 255]);
    assert_close(image.pixel(98, 98), [0, 0, 0, 255]);

    let doubled = render(&doc, 2.0, &mut renderer);
    assert_eq!((doubled.width, doubled.height), (200, 200));
    assert_close(doubled.pixel(150, 40), [127, 127, 255, 255]);
}

#[test]
fn free_content_is_transparent_around_it() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let page = Page::new("p")
        .with_child(Node::shape(Geometry::rect(5.0, 10.0)).with_fill(Color::rgba(0, 255, 0, 128)));
    let page_id = page.id;
    let doc = Document::new("t").with_page(page);
    let bytes = png::export_area(
        &doc,
        page_id,
        Rect::new(0.0, 0.0, 10.0, 10.0),
        1.0,
        &mut SceneBuilder::new(),
        &mut renderer,
    )
    .unwrap();
    let decoder = ::png::Decoder::new(std::io::Cursor::new(bytes));
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    reader.next_frame(&mut buf).unwrap();
    let px = |x: usize, y: usize| -> [u8; 4] { buf[(y * 10 + x) * 4..][..4].try_into().unwrap() };
    assert_close(px(8, 5), [0, 0, 0, 0]);
    assert_close(px(2, 5), [0, 255, 0, 128]);
}

#[test]
fn png_export_produces_one_file_per_frame() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let a = Node::frame("A", 30.0, 20.0);
    let b = Node::frame("B", 10.0, 10.0).at(100.0, 0.0);
    let ids = [a.id, b.id];
    let page = Page::new("p").with_child(a).with_child(b);
    let page_id = page.id;
    let doc = Document::new("t").with_page(page);
    let files = png::export_nodes(
        &doc,
        page_id,
        &ids,
        1.0,
        &mut SceneBuilder::new(),
        &mut renderer,
    )
    .unwrap();
    assert_eq!(files.len(), 2);
    let size = |bytes: &[u8]| {
        let reader = ::png::Decoder::new(std::io::Cursor::new(bytes.to_vec()))
            .read_info()
            .unwrap();
        (reader.info().width, reader.info().height)
    };
    assert_eq!(size(&files[0]), (30, 20));
    assert_eq!(size(&files[1]), (10, 10));
}
