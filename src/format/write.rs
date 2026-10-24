use std::io::{Seek, Write};

use jiff::Timestamp;
use serde_json::Map;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::error::Result;
use crate::model::Document;

use super::manifest::{AssetEntry, Manifest, PageEntry};
use super::{FORMAT_VERSION, MIME_TYPE};

/// zlib level for the JSON entries: a good balance of speed and size.
const DEFLATE_LEVEL: i64 = 6;

/// Options for [`write`](fn@write).
#[derive(Clone, Debug, Default)]
pub struct WriteOptions {
    /// When the document was created. `None` uses the current time; pass the value read
    /// from the original file to keep it across saves.
    pub created: Option<Timestamp>,
    /// PNG preview of the first page, stored as `thumbnail.png`.
    pub thumbnail: Option<Vec<u8>>,
}

/// Writes a document as a `.forma` archive.
///
/// Only assets referenced by the document are written.
pub fn write<W: Write + Seek>(
    document: &Document,
    writer: W,
    options: &WriteOptions,
) -> Result<()> {
    let now = Timestamp::now();
    let used = document.used_assets();
    let assets: Vec<_> = document
        .assets
        .iter()
        .filter(|(id, _)| used.contains(id))
        .collect();

    let manifest = Manifest {
        format_version: FORMAT_VERSION,
        generator: concat!("libforma ", env!("CARGO_PKG_VERSION")).into(),
        created: options.created.unwrap_or(now),
        modified: now,
        title: document.title.clone(),
        pages: document
            .pages
            .iter()
            .map(|p| PageEntry {
                id: p.id,
                name: p.name.clone(),
                frames: p.children.iter().filter(|n| n.is_frame()).count(),
            })
            .collect(),
        assets: assets
            .iter()
            .map(|(id, asset)| AssetEntry {
                id: *id,
                media_type: asset.media_type.clone(),
                path: format!("assets/{}.{}", id.to_hex(), asset.extension()),
            })
            .collect(),
        extra: Map::new(),
    };

    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    // An explicit level keeps the fast zlib encoder: the `deflate` feature also brings
    // zopfli, which the default level may pick and which is far too slow for autosave.
    let deflated = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .compression_level(Some(DEFLATE_LEVEL));

    let mut zip = ZipWriter::new(writer);
    zip.start_file("mimetype", stored)?;
    zip.write_all(MIME_TYPE.as_bytes())?;

    // Serialized to memory first: many small writes into the compressor are slow.
    zip.start_file("manifest.json", deflated)?;
    zip.write_all(&serde_json::to_vec_pretty(&manifest)?)?;

    zip.start_file("document.json", deflated)?;
    zip.write_all(&serde_json::to_vec_pretty(document)?)?;

    for ((_, asset), entry) in assets.iter().zip(&manifest.assets) {
        // Images are already compressed.
        zip.start_file(&entry.path, stored)?;
        zip.write_all(&asset.data)?;
    }

    if let Some(thumbnail) = &options.thumbnail {
        zip.start_file("thumbnail.png", stored)?;
        zip.write_all(thumbnail)?;
    }

    zip.finish()?;
    Ok(())
}
