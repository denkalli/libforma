//! Builds a sample document and writes it as .forma, .svg, .png and .pdf.
//!
//! ```sh
//! cargo run --example showcase -- <output directory>
//! ```

use std::path::PathBuf;

use libforma::boolean::{BooleanOp, combine_nodes};
use libforma::export::{pdf, png, svg};
use libforma::format::{self, WriteOptions};
use libforma::geom::{Affine, Point};
use libforma::history::History;
use libforma::model::{Geometry, TextAlign, TextNode, TextStyle};
use libforma::paint::{BlendMode, Color, GradientStop, LineCap, Paint, Stroke};
use libforma::render::OffscreenRenderer;
use libforma::scene::SceneBuilder;
use libforma::{Document, Node, NodeKind, Page};

fn main() -> libforma::Result<()> {
    let out = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "showcase".into()));
    std::fs::create_dir_all(&out)?;

    let mut doc = sample();
    let page = doc.pages[0].id;
    let frame = doc.pages[0].children[0].id;

    // A boolean operation through the history, like the editor will do it.
    let ring: Vec<_> = doc.pages[0].children[0].children()[3..5]
        .iter()
        .map(|n| n.id)
        .collect();
    let command = combine_nodes(&doc, &ring, BooleanOp::Subtract)?;
    History::new().execute(&mut doc, command)?;

    let mut builder = SceneBuilder::new();
    let mut renderer = OffscreenRenderer::new()?;
    let frames = [frame];
    let thumbnail =
        png::export_nodes(&doc, page, &frames, 0.25, &mut builder, &mut renderer)?.remove(0);

    std::fs::write(
        out.join("showcase.svg"),
        svg::export_nodes(&doc, page, &frames, &mut builder)?.remove(0),
    )?;
    std::fs::write(
        out.join("showcase.png"),
        png::export_nodes(&doc, page, &frames, 1.0, &mut builder, &mut renderer)?.remove(0),
    )?;
    std::fs::write(
        out.join("showcase.pdf"),
        pdf::export_nodes(&doc, page, &frames, &mut builder)?,
    )?;
    format::save(
        &doc,
        out.join("showcase.forma"),
        &WriteOptions {
            thumbnail: Some(thumbnail),
            ..Default::default()
        },
    )?;

    let reopened = format::open(out.join("showcase.forma"))?;
    assert_eq!(reopened.pages, doc.pages);
    println!("wrote showcase.{{forma,svg,png,pdf}} to {}", out.display());
    Ok(())
}

fn sample() -> Document {
    let mut doc = Document::new("Showcase");
    let image = doc
        .assets
        .insert_image(include_bytes!("../tests/data/pixel.png").to_vec())
        .unwrap();

    let sky = Paint::LinearGradient {
        start: Point::new(0.0, 0.0),
        end: Point::new(0.0, 540.0),
        stops: vec![
            GradientStop::new(0.0, Color::rgb(0x1e, 0x2a, 0x78)),
            GradientStop::new(1.0, Color::rgb(0xff, 0x7e, 0x5f)),
        ],
    };
    let sun = Paint::RadialGradient {
        center: Point::new(90.0, 90.0),
        radius: 90.0,
        stops: vec![
            GradientStop::new(0.0, Color::rgb(0xff, 0xf3, 0xb0)),
            GradientStop::new(1.0, Color::rgb(0xff, 0xb3, 0x47)),
        ],
    };

    let mut dashed = Stroke::new(Color::WHITE.with_alpha(200), 4.0);
    dashed.dash = vec![12.0, 8.0];
    dashed.cap = LineCap::Round;

    let mut title = TextNode::new(
        "Forma",
        TextStyle {
            font_size: 96.0,
            font_weight: 700,
            color: Color::WHITE,
            ..TextStyle::default()
        },
    );
    title.width = Some(960.0);
    title.align = TextAlign::Center;

    let mut subtitle = TextNode::new(
        "Templates and vector design, free for everyone.",
        TextStyle {
            font_size: 32.0,
            color: Color::WHITE.with_alpha(220),
            ..TextStyle::default()
        },
    );
    subtitle.width = Some(960.0);
    subtitle.align = TextAlign::Center;

    let frame = Node::frame("Cover", 1080.0, 1080.0)
        .with_child(
            Node::shape(Geometry::rect(1080.0, 1080.0))
                .with_name("Sky")
                .with_fill(sky),
        )
        .with_child(
            Node::shape(Geometry::ellipse(180.0, 180.0))
                .with_name("Sun")
                .at(450.0, 300.0)
                .with_fill(sun),
        )
        .with_child(
            Node::shape(Geometry::Star {
                size: (120.0, 120.0).into(),
                points: 5,
                inner_ratio: 0.45,
            })
            .with_name("Star")
            .with_transform(Affine::translate((820.0, 140.0)) * Affine::rotate(0.2))
            .with_fill(Color::rgb(0xff, 0xe0, 0x66))
            .with_blend(BlendMode::Screen),
        )
        // These two become a ring through a boolean subtraction.
        .with_child(
            Node::shape(Geometry::ellipse(200.0, 200.0))
                .with_name("Ring")
                .at(120.0, 120.0)
                .with_fill(Color::WHITE.with_alpha(160)),
        )
        .with_child(
            Node::shape(Geometry::ellipse(120.0, 120.0))
                .at(160.0, 160.0)
                .with_fill(Color::BLACK),
        )
        .with_child(
            Node::shape(Geometry::Polygon {
                size: (1080.0, 400.0).into(),
                sides: 3,
            })
            .with_name("Mountain")
            .at(0.0, 680.0)
            .with_fill(Color::rgb(0x2b, 0x1d, 0x4e))
            .with_opacity(0.9),
        )
        .with_child(
            Node::shape(Geometry::rounded_rect(1000.0, 1000.0, 40.0))
                .at(40.0, 40.0)
                .with_stroke(dashed),
        )
        .with_child(
            Node::new(NodeKind::Text(title))
                .with_name("Title")
                .at(60.0, 560.0),
        )
        .with_child(
            Node::new(NodeKind::Text(subtitle))
                .with_name("Subtitle")
                .at(60.0, 690.0),
        )
        .with_child(
            Node::image(image, (48.0, 48.0))
                .with_name("Logo")
                .at(996.0, 996.0),
        );

    doc.with_page(Page::new("Page 1").with_child(frame))
}
