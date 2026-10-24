use libforma::model::{TextAlign, TextNode, TextStyle};
use libforma::scene::{DisplayItem, SceneBuilder};
use libforma::text::TextEngine;
use libforma::{Document, Node, Page};

fn style(size: f64) -> TextStyle {
    TextStyle {
        font_size: size,
        ..TextStyle::default()
    }
}

/// System fonts differ between machines; skip when none are installed.
fn has_fonts(engine: &mut TextEngine) -> bool {
    let size = engine.measure(&TextNode::new("x", style(16.0)));
    if size.width == 0.0 {
        eprintln!("skipping: no system fonts found");
        return false;
    }
    true
}

#[test]
fn text_becomes_filled_outlines() {
    let mut builder = SceneBuilder::new();
    if !has_fonts(builder.text_engine()) {
        return;
    }
    let page = Page::new("p").with_child(Node::text("Forma", style(32.0)).at(10.0, 10.0));
    let doc = Document::new("t").with_page(page);
    let list = builder.build_page(&doc, &doc.pages[0]);
    let fills: Vec<_> = list
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Fill {
                path, transform, ..
            } => Some((*transform * path.clone()).control_box()),
            _ => None,
        })
        .collect();
    assert!(!fills.is_empty());
    // Glyphs sit inside the line box starting at the node origin.
    for bbox in fills {
        assert!(bbox.x0 >= 10.0 - 1.0 && bbox.y0 >= 10.0 - 1.0, "{bbox:?}");
        assert!(bbox.y1 <= 10.0 + 32.0 * 1.2 + 1.0, "{bbox:?}");
    }
}

#[test]
fn wrapping_and_alignment() {
    let mut engine = TextEngine::new();
    if !has_fonts(&mut engine) {
        return;
    }
    let content = "one two three four five six seven eight";
    let single = engine.measure(&TextNode::new(content, style(16.0)));
    let mut wrapped = TextNode::new(content, style(16.0));
    wrapped.width = Some(single.width / 3.0);
    let wrapped_size = engine.measure(&wrapped);
    assert!(
        wrapped_size.height >= single.height * 2.5,
        "{single:?} vs {wrapped_size:?}"
    );
    assert!(wrapped_size.width <= single.width / 3.0 + 1.0);

    // Centered text in a wide box starts further right than start-aligned text.
    let left_edge = |align: TextAlign| {
        let mut text = TextNode::new("hi", style(16.0));
        text.width = Some(400.0);
        text.align = align;
        let page = Page::new("p").with_child(Node::new(libforma::NodeKind::Text(text)));
        let doc = Document::new("t").with_page(page);
        let list = SceneBuilder::new().build_page(&doc, &doc.pages[0]);
        list.items
            .iter()
            .find_map(|i| match i {
                DisplayItem::Fill { path, .. } => Some(path.control_box().x0),
                _ => None,
            })
            .unwrap()
    };
    assert!(left_edge(TextAlign::Center) > left_edge(TextAlign::Start) + 100.0);
}

#[test]
fn changed_text_is_laid_out_again() {
    let mut engine = TextEngine::new();
    if !has_fonts(&mut engine) {
        return;
    }
    let short = TextNode::new("Forma", style(20.0));
    let a = engine.measure(&short);
    // Same text again: same size, from the cache.
    assert_eq!(engine.measure(&short), a);
    let long = TextNode::new("Forma Forma", style(20.0));
    assert!(engine.measure(&long).width > a.width);
    let bigger = TextNode::new("Forma", style(40.0));
    assert!(engine.measure(&bigger).height > a.height);
    let mut wrapped = TextNode::new("Forma Forma", style(20.0));
    wrapped.width = Some(a.width * 1.2);
    assert!(engine.measure(&wrapped).height > a.height * 1.5);
    // The color does not change the layout, only the paint of the outlines.
    let mut red = short.clone();
    red.style.color = libforma::paint::Color::rgb(255, 0, 0);
    assert_eq!(engine.measure(&red), a);
}

#[test]
fn families_weights_and_registered_fonts() {
    let mut engine = TextEngine::new();
    let inter = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../forma-desktop/assets/fonts/Inter-SemiBold.ttf"
    ));
    let Ok(inter) = inter else {
        eprintln!("skipping: Inter not found next to libforma");
        return;
    };
    // A made-up file registers nothing.
    assert!(engine.register_font(b"not a font".to_vec()).is_empty());
    assert_eq!(engine.register_font(inter), ["Inter"]);
    assert!(engine.families().iter().any(|f| f == "Inter"));
    assert!(engine.weights("Inter").contains(&600));
    assert!(engine.weights("No Such Family").is_empty());
    let (data, index) = engine.font_data("Inter").unwrap();
    assert!(data.len() > 1000);
    assert_eq!(index, 0);

    // The family can be used right away.
    let mut style = style(20.0);
    style.font_family = "Inter".into();
    style.font_weight = 600;
    assert!(engine.measure(&TextNode::new("Forma", style)).width > 0.0);
}

#[test]
fn a_fixed_text_box_sets_bounds_and_vertical_position() {
    use libforma::model::TextVerticalAlign;
    use libforma::query;

    let mut builder = SceneBuilder::new();
    if !has_fonts(builder.text_engine()) {
        return;
    }
    let mut text = TextNode::new("Forma", style(20.0));
    let measured = builder.text_engine().measure(&text);
    text.width = Some(300.0);
    text.height = Some(200.0);
    let top_of_ink = |text: &TextNode, builder: &mut SceneBuilder| {
        let node = Node::new(libforma::NodeKind::Text(text.clone()));
        let page = Page::new("p").with_child(node.clone());
        let doc = Document::new("t").with_page(page);
        let bounds = query::local_bounds(&node, builder.text_engine());
        let list = builder.build_page(&doc, &doc.pages[0]);
        let top = list
            .items
            .iter()
            .filter_map(|item| match item {
                DisplayItem::Fill {
                    path, transform, ..
                } => Some((*transform * path.clone()).control_box().y0),
                _ => None,
            })
            .fold(f64::INFINITY, f64::min);
        (bounds, top)
    };
    let (bounds, top) = top_of_ink(&text, &mut builder);
    assert_eq!(bounds.size(), (300.0, 200.0).into());
    text.vertical_align = TextVerticalAlign::Bottom;
    let (_, bottom) = top_of_ink(&text, &mut builder);
    assert!((bottom - top - (200.0 - measured.height)).abs() < 0.5);
    text.vertical_align = TextVerticalAlign::Middle;
    let (_, middle) = top_of_ink(&text, &mut builder);
    assert!((middle - top - (200.0 - measured.height) / 2.0).abs() < 0.5);
    // Without a width, the height is ignored and the box fits the text.
    text.width = None;
    let (bounds, _) = top_of_ink(&text, &mut builder);
    assert_eq!(bounds.size(), measured);
}
