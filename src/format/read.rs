use std::io::{Read, Seek};

use serde_json::Value;
use zip::ZipArchive;

use crate::error::{Error, Result};
use crate::id::AssetId;
use crate::model::Document;

use super::MIME_TYPE;
use super::manifest::Manifest;
use super::migrate::migrate;

/// Refuse entries larger than this when decompressed, to protect against ZIP bombs.
const MAX_ENTRY_SIZE: u64 = 1 << 30;

/// Reads a `.forma` archive.
///
/// Older format versions are migrated; newer ones are rejected with
/// [`Error::UnsupportedVersion`]. Assets listed in the manifest are loaded and checked
/// against their hash.
pub fn read<R: Read + Seek>(reader: R) -> Result<Document> {
    read_with_manifest(reader).map(|(document, _)| document)
}

/// Like [`read`], also returning the manifest (for example to keep its `created` time).
pub fn read_with_manifest<R: Read + Seek>(reader: R) -> Result<(Document, Manifest)> {
    let mut zip = ZipArchive::new(reader)?;

    let first = zip.by_index(0)?;
    if first.name()? != "mimetype" {
        return Err(Error::InvalidFile("first entry is not `mimetype`".into()));
    }
    drop(first);
    let mimetype = read_entry(&mut zip, "mimetype")?;
    if mimetype != MIME_TYPE.as_bytes() {
        return Err(Error::InvalidFile("not a Forma document".into()));
    }

    let manifest: Value = serde_json::from_slice(&read_entry(&mut zip, "manifest.json")?)?;
    let version = manifest
        .get("format_version")
        .and_then(Value::as_u64)
        .ok_or_else(|| Error::InvalidFile("manifest has no format_version".into()))?;
    let version =
        u32::try_from(version).map_err(|_| Error::InvalidFile("bad format_version".into()))?;

    // `migrate` rejects newer versions before the document is interpreted.
    let document: Value = serde_json::from_slice(&read_entry(&mut zip, "document.json")?)?;
    let document = migrate(document, version)?;
    let mut document: Document = serde_json::from_value(document)?;
    let manifest: Manifest = serde_json::from_value(manifest)?;

    for entry in &manifest.assets {
        let data = read_entry(&mut zip, &entry.path)?;
        if AssetId::for_content(&data) != entry.id {
            return Err(Error::InvalidFile(format!(
                "asset {} is corrupted",
                entry.path
            )));
        }
        document.assets.insert(entry.media_type.clone(), data);
    }

    Ok((document, manifest))
}

/// Manifest and thumbnail of a file, read without loading the document.
#[derive(Clone, Debug)]
pub struct Preview {
    pub manifest: Manifest,
    /// PNG data of `thumbnail.png`, if the file has one.
    pub thumbnail: Option<Vec<u8>>,
}

/// Reads only the manifest and the thumbnail.
///
/// Files with a newer format version still return their manifest, so they can be listed.
pub fn read_preview<R: Read + Seek>(reader: R) -> Result<Preview> {
    let mut zip = ZipArchive::new(reader)?;
    if read_entry(&mut zip, "mimetype")? != MIME_TYPE.as_bytes() {
        return Err(Error::InvalidFile("not a Forma document".into()));
    }
    let manifest: Manifest = serde_json::from_slice(&read_entry(&mut zip, "manifest.json")?)?;
    let has_thumbnail = zip
        .file_names()
        .any(|n| n.is_ok_and(|n| n == "thumbnail.png"));
    let thumbnail = if has_thumbnail {
        Some(read_entry(&mut zip, "thumbnail.png")?)
    } else {
        None
    };
    Ok(Preview {
        manifest,
        thumbnail,
    })
}

fn read_entry<R: Read + Seek>(zip: &mut ZipArchive<R>, name: &str) -> Result<Vec<u8>> {
    let entry = zip.by_name(name).map_err(|err| match err {
        zip::result::ZipError::FileNotFound => Error::InvalidFile(format!("missing `{name}`")),
        other => other.into(),
    })?;
    if entry.size() > MAX_ENTRY_SIZE {
        return Err(Error::InvalidFile(format!("`{name}` is too large")));
    }
    let mut data = Vec::with_capacity(entry.size() as usize);
    entry.take(MAX_ENTRY_SIZE).read_to_end(&mut data)?;
    Ok(data)
}
