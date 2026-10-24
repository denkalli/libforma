use std::io::{Cursor, Read, Write};

use libforma::format::{self, WriteOptions};
use libforma::geom::{Affine, BezPath, Point};
use libforma::model::{Geometry, TextStyle};
use libforma::paint::{BlendMode, Color, GradientStop, Paint, Stroke};
use libforma::{Document, Error, Node, Page};
use zip::ZipArchive;
use zip::write::SimpleFileOptions;

/// 1x1 transparent PNG.
const PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae,
    0x42, 0x60, 0x82,
];

fn sample_document() -> Document {
    let mut doc = Document::new("Sample");
    let image = doc.assets.insert_image(PNG.to_vec()).unwrap();
    // Unused assets are not saved.
    doc.assets.insert("application/octet-stream", vec![1, 2, 3]);

    let mut path = BezPath::new();
    path.move_to((0.0, 0.0));
    path.curve_to((10.0, 0.5), (20.25, 30.0), (40.0, 40.0));
    path.close_path();

    let gradient = Paint::LinearGradient {
        start: Point::new(0.0, 0.0),
        end: Point::new(100.0, 0.0),
        stops: vec![
            GradientStop::new(0.0, Color::rgb(255, 0, 0)),
            GradientStop::new(1.0, Color::rgba(0, 0, 255, 128)),
        ],
    };

    let page = Page::new("Cover")
        .with_child(
            Node::shape(Geometry::rounded_rect(200.0, 100.0, 12.0))
                .with_name("Card")
                .at(40.0, 40.0)
                .with_fill(gradient)
                .with_stroke(Stroke::new(Color::BLACK, 2.0)),
        )
        .with_child(Node::group(vec![
            Node::shape(Geometry::Path { path }).with_fill(Color::rgb(10, 20, 30)),
            Node::shape(Geometry::Star {
                size: (50.0, 50.0).into(),
                points: 5,
                inner_ratio: 0.4,
            })
            // Rotation coefficients need every bit of precision to survive a round trip.
            .with_transform(Affine::rotate(0.2))
            .with_opacity(0.5)
            .with_blend(BlendMode::Multiply),
        ]))
        .with_child(Node::text("Hello, Forma!", TextStyle::default()).at(10.0, 500.0))
        .with_child(Node::image(image, (64.0, 64.0)));

    doc.with_page(page).with_page(Page::new("Back"))
}

fn save_to_bytes(doc: &Document) -> Vec<u8> {
    let mut buf = Cursor::new(Vec::new());
    format::write(doc, &mut buf, &WriteOptions::default()).unwrap();
    buf.into_inner()
}

fn replace_entry(archive: &[u8], name: &str, f: impl Fn(&str) -> String) -> Vec<u8> {
    // `f` receives the entry as (lossy) UTF-8; binary entries are simply replaced.
    let mut zip = ZipArchive::new(Cursor::new(archive)).unwrap();
    let mut out = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).unwrap();
        let entry_name = entry.name().unwrap().to_string();
        let mut data = Vec::new();
        entry.read_to_end(&mut data).unwrap();
        if entry_name == name {
            data = f(&String::from_utf8_lossy(&data)).into_bytes();
        }
        out.start_file(entry_name, SimpleFileOptions::default())
            .unwrap();
        out.write_all(&data).unwrap();
    }
    out.finish().unwrap().into_inner()
}

#[test]
fn round_trip_preserves_document() {
    let original = sample_document();
    let loaded = format::read(Cursor::new(save_to_bytes(&original))).unwrap();

    assert_eq!(loaded.title, original.title);
    assert_eq!(loaded.pages, original.pages);
    // Only the referenced image survives.
    assert_eq!(loaded.assets.len(), 1);
    assert_eq!(loaded.used_assets(), original.used_assets());
}

#[test]
fn archive_layout() {
    let bytes = save_to_bytes(&sample_document());
    let mut zip = ZipArchive::new(Cursor::new(bytes)).unwrap();
    let first = zip.by_index(0).unwrap();
    assert_eq!(first.name().unwrap(), "mimetype");
    assert_eq!(first.compression(), zip::CompressionMethod::Stored);
    drop(first);
    let names: Vec<String> = zip.file_names().map(|n| n.unwrap().to_string()).collect();
    assert!(names.contains(&"manifest.json".to_string()));
    assert!(names.contains(&"document.json".to_string()));
    assert_eq!(names.iter().filter(|n| n.starts_with("assets/")).count(), 1);
}

#[test]
fn unknown_fields_survive_a_save() {
    let bytes = replace_entry(
        &save_to_bytes(&sample_document()),
        "document.json",
        |json| {
            let mut value: serde_json::Value = serde_json::from_str(json).unwrap();
            value["future_doc_field"] = "doc".into();
            value["pages"][0]["future_page_field"] = 1.into();
            value["pages"][0]["children"][0]["future_node_field"] = true.into();
            value.to_string()
        },
    );
    let doc = format::read(Cursor::new(bytes)).unwrap();
    let resaved = format::read(Cursor::new(save_to_bytes(&doc))).unwrap();
    assert_eq!(resaved.extra["future_doc_field"], "doc");
    assert_eq!(resaved.pages[0].extra["future_page_field"], 1);
    assert_eq!(
        resaved.pages[0].children[0].extra["future_node_field"],
        true
    );
}

#[test]
fn newer_version_is_rejected() {
    let bytes = replace_entry(
        &save_to_bytes(&sample_document()),
        "manifest.json",
        |json| {
            let current = format!("\"format_version\": {}", format::FORMAT_VERSION);
            json.replace(&current, "\"format_version\": 99")
        },
    );
    let err = format::read(Cursor::new(bytes)).unwrap_err();
    assert!(
        matches!(err, Error::UnsupportedVersion { found: 99, .. }),
        "{err}"
    );
}

#[test]
fn wrong_mimetype_is_rejected() {
    let bytes = replace_entry(&save_to_bytes(&sample_document()), "mimetype", |_| {
        "application/zip".into()
    });
    assert!(matches!(
        format::read(Cursor::new(bytes)),
        Err(Error::InvalidFile(_))
    ));
}

#[test]
fn corrupted_asset_is_rejected() {
    let doc = sample_document();
    let image = *doc.used_assets().iter().next().unwrap();
    let path = format!("assets/{}.png", image.to_hex());
    let bytes = replace_entry(&save_to_bytes(&doc), &path, |_| "garbage".into());
    assert!(matches!(
        format::read(Cursor::new(bytes)),
        Err(Error::InvalidFile(_))
    ));
}

#[test]
fn save_and_open_file() {
    let dir = std::env::temp_dir().join(format!("libforma-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("doc.forma");
    let doc = sample_document();
    format::save(&doc, &path, &WriteOptions::default()).unwrap();
    let loaded = format::open(&path).unwrap();
    assert_eq!(loaded.pages, doc.pages);
    assert!(!dir.join("doc.forma.tmp").exists());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn preview_reads_manifest_and_thumbnail() {
    let doc = sample_document();
    let mut buf = Cursor::new(Vec::new());
    let options = WriteOptions {
        thumbnail: Some(PNG.to_vec()),
        ..Default::default()
    };
    format::write(&doc, &mut buf, &options).unwrap();
    let preview = format::read_preview(Cursor::new(buf.into_inner())).unwrap();
    assert_eq!(preview.manifest.title, "Sample");
    assert_eq!(preview.manifest.pages.len(), 2);
    assert_eq!(preview.thumbnail.as_deref(), Some(PNG));

    let without = format::read_preview(Cursor::new(save_to_bytes(&doc))).unwrap();
    assert!(without.thumbnail.is_none());
}

#[test]
fn duplicate_gives_new_ids() {
    let doc = sample_document();
    let group = doc.pages[0].children[1].clone();
    let copy = group.duplicate();
    assert_ne!(copy.id, group.id);
    assert_ne!(copy.children()[0].id, group.children()[0].id);
    assert_eq!(copy.children()[0].kind, group.children()[0].kind);
}

#[test]
fn version_1_files_open_with_their_pages_as_frames() {
    let doc = format::open("tests/data/showcase-v1.forma").unwrap();
    assert_eq!(doc.pages.len(), 1);
    let page = &doc.pages[0];
    assert_eq!(page.children.len(), 1);
    let frame = &page.children[0];
    assert_eq!(frame.name, "Cover");
    let libforma::NodeKind::Frame(f) = &frame.kind else {
        panic!("expected a frame")
    };
    assert_eq!((f.size.width, f.size.height), (1080.0, 1080.0));
    assert_eq!(
        f.children.len(),
        9,
        "the old page content moved into the frame"
    );
    // Saving writes version 2, and it reads back unchanged.
    let again = format::read(Cursor::new(save_to_bytes(&doc))).unwrap();
    assert_eq!(again.pages, doc.pages);
}
