//! Timings on a large document: 20 frames holding 5 000 shapes (some in groups) and 300
//! texts. Ignored by default; run in release mode to see the numbers:
//!
//! ```text
//! cargo test --release --test perf -- --ignored --nocapture
//! ```
//!
//! The limits are loose on purpose: they catch gross regressions, not small ones.

use std::io::Cursor;
use std::time::{Duration, Instant};

use libforma::format::{self, WriteOptions};
use libforma::geom::{Point, Rect};
use libforma::model::{Geometry, NodeKind, TextStyle};
use libforma::paint::Color;
use libforma::query;
use libforma::render::OffscreenRenderer;
use libforma::scene::SceneBuilder;
use libforma::{Document, Node, Page};

const FRAMES: usize = 20;
const SHAPES_PER_FRAME: usize = 250;
const TEXTS_PER_FRAME: usize = 15;
const FRAME_SIZE: f64 = 1080.0;
const FRAME_GAP: f64 = 200.0;

/// A small deterministic random generator, so every run measures the same document.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

fn big_document() -> Document {
    let mut rng = Lcg(7);
    let mut page = Page::new("Grande");
    for f in 0..FRAMES {
        let (col, row) = ((f % 5) as f64, (f / 5) as f64);
        let mut frame = Node::frame(format!("Quadro {f}"), FRAME_SIZE, FRAME_SIZE).at(
            col * (FRAME_SIZE + FRAME_GAP),
            row * (FRAME_SIZE + FRAME_GAP),
        );
        let mut group = Vec::new();
        for i in 0..SHAPES_PER_FRAME {
            let (w, h) = (10.0 + rng.next() * 80.0, 10.0 + rng.next() * 80.0);
            let geometry = match i % 4 {
                0 => Geometry::rect(w, h),
                1 => Geometry::ellipse(w, h),
                2 => Geometry::Star {
                    size: (w, h).into(),
                    points: 5,
                    inner_ratio: 0.4,
                },
                _ => Geometry::Polygon {
                    size: (w, h).into(),
                    sides: 6,
                },
            };
            let shape = Node::shape(geometry)
                .with_fill(Color::rgb((i * 7) as u8, (i * 13) as u8, 120))
                .at(rng.next() * (FRAME_SIZE - w), rng.next() * (FRAME_SIZE - h));
            // Every fifth shape goes into a group of five.
            if i % 5 == 0 {
                group.push(shape);
                if group.len() == 5 {
                    frame = frame.with_child(Node::group(std::mem::take(&mut group)));
                }
            } else {
                frame = frame.with_child(shape);
            }
        }
        for g in group {
            frame = frame.with_child(g);
        }
        for i in 0..TEXTS_PER_FRAME {
            let style = TextStyle {
                font_size: 14.0 + (i % 5) as f64 * 8.0,
                ..TextStyle::default()
            };
            let text = Node::text(format!("Texto número {i} do quadro {f}"), style)
                .at(rng.next() * 700.0, rng.next() * 1000.0);
            frame = frame.with_child(text);
        }
        page = page.with_child(frame);
    }
    Document::new("Desempenho").with_page(page)
}

/// Runs `f` `runs` times and returns the average duration.
fn time<T>(runs: u32, mut f: impl FnMut() -> T) -> Duration {
    let start = Instant::now();
    for _ in 0..runs {
        std::hint::black_box(f());
    }
    start.elapsed() / runs
}

fn report(name: &str, d: Duration) {
    println!("{name:<44} {:>9.3} ms", d.as_secs_f64() * 1000.0);
}

#[test]
#[ignore = "slow; run with --release --ignored --nocapture"]
fn large_document_timings() {
    let doc = big_document();
    let page = &doc.pages[0];
    let mut nodes = 0;
    page.walk(&mut |_| nodes += 1);
    println!("\n{nodes} nodes on one page\n");

    let start = Instant::now();
    let mut builder = SceneBuilder::new();
    let first = builder.build_page(&doc, page);
    report("build_page (first, loads fonts)", start.elapsed());
    println!("{:<44} {:>9}", "display items", first.items.len());
    let build = time(10, || builder.build_page(&doc, page));
    report("build_page", build);

    let mut texts = Vec::new();
    page.walk(&mut |n| {
        if let NodeKind::Text(t) = &n.kind {
            texts.push(t.clone());
        }
    });
    let measure = time(10, || {
        texts
            .iter()
            .map(|t| builder.text_engine().measure(t).width)
            .sum::<f64>()
    });
    report(&format!("measure all {} texts", texts.len()), measure);

    let bounds = time(10, || query::content_bounds(page, builder.text_engine()));
    report("content_bounds", bounds);

    let area = query::content_bounds(page, builder.text_engine()).unwrap();
    let mut rng = Lcg(3);
    let points: Vec<Point> = (0..1000)
        .map(|_| {
            Point::new(
                area.x0 + rng.next() * area.width(),
                area.y0 + rng.next() * area.height(),
            )
        })
        .collect();
    let hits = time(1, || {
        points
            .iter()
            .filter(|p| query::hit_test(page, **p, 1.0, builder.text_engine()).is_some())
            .count()
    });
    report("hit_test, per point", hits / 1000);

    let rects = time(1, || {
        (0..100)
            .map(|i| {
                let x = f64::from(i) * 50.0;
                let r = Rect::new(x, x, x + 800.0, x + 800.0);
                query::nodes_in_rect(page, r, builder.text_engine()).len()
            })
            .sum::<usize>()
    });
    report("nodes_in_rect, per rectangle", rects / 100);

    let frame = query::frames(page)[0];
    let frame_bounds = time(100, || {
        query::page_bounds(page, frame, builder.text_engine())
    });
    report("page_bounds of a frame", frame_bounds);

    let mut file = Vec::new();
    let save = time(1, || {
        file.clear();
        format::write(&doc, Cursor::new(&mut file), &WriteOptions::default()).unwrap();
    });
    report("save (write to memory)", save);
    println!("{:<44} {:>9.1} KB", "file size", file.len() as f64 / 1024.0);
    let open = time(3, || format::read(Cursor::new(&file)).unwrap());
    report("open (read from memory)", open);

    let mut render = None;
    if let Ok(mut renderer) = OffscreenRenderer::new() {
        let list = builder.build_node(&doc, page, frame).unwrap();
        renderer.render_image(&list, &doc.assets, 1.0).unwrap();
        let d = time(5, || {
            renderer.render_image(&list, &doc.assets, 1.0).unwrap()
        });
        report("render_image of a frame (1080², GPU)", d);
        render = Some(d);
    } else {
        println!("no GPU: rendering skipped");
    }

    if cfg!(debug_assertions) {
        return;
    }
    // Loose limits for a release build on an ordinary desktop (measured: build 3 ms, hit
    // test 0.1 ms, save 35 ms, open 31 ms, render 3 ms).
    assert!(
        build < Duration::from_millis(30),
        "build_page took {build:?}"
    );
    assert!(
        bounds < Duration::from_millis(10),
        "content_bounds took {bounds:?}"
    );
    let hit = hits / 1000;
    assert!(hit < Duration::from_millis(1), "hit_test took {hit:?}");
    assert!(save < Duration::from_millis(300), "save took {save:?}");
    assert!(open < Duration::from_millis(300), "open took {open:?}");
    if let Some(d) = render {
        assert!(d < Duration::from_millis(100), "render took {d:?}");
    }
}
