//! Error type shared by the whole crate.

use crate::id::{AssetId, NodeId, PageId};

/// Result alias using [`Error`].
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Errors produced by libforma.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("invalid .forma file: {0}")]
    InvalidFile(String),

    #[error(
        "this file uses format version {found}, but the newest supported version is {supported}"
    )]
    UnsupportedVersion { found: u32, supported: u32 },

    #[error("page not found: {0}")]
    PageNotFound(PageId),

    #[error("node not found: {0}")]
    NodeNotFound(NodeId),

    #[error("asset not found: {0}")]
    AssetNotFound(AssetId),

    #[error("invalid location: {0}")]
    InvalidLocation(String),

    #[error("image error: {0}")]
    Image(String),

    #[error("rendering error: {0}")]
    Render(String),

    #[error("PDF export error: {0}")]
    Pdf(String),
}

impl From<zip::result::ZipError> for Error {
    fn from(err: zip::result::ZipError) -> Self {
        match err {
            zip::result::ZipError::Io(err) => Error::Io(err),
            other => Error::InvalidFile(other.to_string()),
        }
    }
}

impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Error::InvalidFile(err.to_string())
    }
}
