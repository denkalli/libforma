use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::id::{AssetId, PageId};

/// Contents of `manifest.json`.
///
/// It describes the file without requiring `document.json` to be parsed, so file browsers
/// and template galleries can list pages cheaply.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub format_version: u32,
    /// Application that wrote the file, e.g. `libforma 0.1.0`.
    pub generator: String,
    /// When the document was first saved (RFC 3339).
    pub created: Timestamp,
    /// When the document was last saved (RFC 3339).
    pub modified: Timestamp,
    pub title: String,
    pub pages: Vec<PageEntry>,
    pub assets: Vec<AssetEntry>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct PageEntry {
    pub id: PageId,
    pub name: String,
    /// Number of top-level frames on the page.
    #[serde(default)]
    pub frames: usize,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct AssetEntry {
    pub id: AssetId,
    pub media_type: String,
    /// Path of the asset inside the archive.
    pub path: String,
}
