//! Reading and writing `.forma` files.
//!
//! A `.forma` file is a ZIP archive:
//!
//! ```text
//! mimetype            "application/vnd.forma+zip", first entry, stored uncompressed
//! manifest.json       format version, generator, timestamps, pages and assets
//! document.json       the document tree
//! assets/<hash>.<ext> binary assets, named by the BLAKE3 hash of their content
//! thumbnail.png       optional preview of the first page
//! ```
//!
//! See `docs/format.md` for the full specification.

mod manifest;
mod migrate;
mod read;
mod write;

use std::fs::File;
use std::io::{BufReader, BufWriter, Write as _};
use std::path::Path;

pub use manifest::{AssetEntry, Manifest, PageEntry};
pub use read::{Preview, read, read_preview, read_with_manifest};
pub use write::{WriteOptions, write};

use crate::error::Result;
use crate::model::Document;

/// Media type stored in the `mimetype` entry.
pub const MIME_TYPE: &str = "application/vnd.forma+zip";

/// File extension, without the dot.
pub const EXTENSION: &str = "forma";

/// Newest format version this library reads and the one it writes.
pub const FORMAT_VERSION: u32 = 2;

/// Opens a `.forma` file.
pub fn open(path: impl AsRef<Path>) -> Result<Document> {
    read(BufReader::new(File::open(path)?))
}

/// Reads a file's manifest and thumbnail without loading the document, for file browsers.
pub fn open_preview(path: impl AsRef<Path>) -> Result<Preview> {
    read_preview(BufReader::new(File::open(path)?))
}

/// Saves a document to a `.forma` file.
///
/// The file is written next to the destination first and then renamed over it, so an
/// interrupted save never leaves a half-written document behind.
pub fn save(document: &Document, path: impl AsRef<Path>, options: &WriteOptions) -> Result<()> {
    let path = path.as_ref();
    let mut tmp_name = path.file_name().unwrap_or_default().to_os_string();
    tmp_name.push(".tmp");
    let tmp_path = path.with_file_name(tmp_name);

    let result = (|| {
        let mut file = BufWriter::new(File::create(&tmp_path)?);
        write(document, &mut file, options)?;
        file.flush()?;
        file.into_inner().map_err(|e| e.into_error())?.sync_all()?;
        std::fs::rename(&tmp_path, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp_path);
    }
    result
}
