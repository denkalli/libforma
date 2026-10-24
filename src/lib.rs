//! Core library of Forma: document model, vector engine, rendering and the `.forma` file
//! format.
//!
//! The crate is organised in layers:
//!
//! - [`model`] — the document tree: pages, nodes, shapes, text and images.
//! - [`history`] — every edit goes through a [`Command`](history::Command), which makes
//!   undo and redo possible.
//! - [`scene`] — flattens a page into a [`DisplayList`](scene::DisplayList), the only
//!   input the renderer and the exporters ever see.
//! - [`render`] — GPU rendering (feature `render`).
//! - [`query`] — bounds and hit testing, for editors.
//! - [`export`] — SVG, PNG (feature `render`) and PDF (feature `pdf`).
//! - [`format`](mod@format) — reading and writing `.forma` files.
//!
//! Geometry types come from [`kurbo`] and are re-exported in [`geom`].

pub mod boolean;
pub mod error;
pub mod export;
pub mod format;
pub mod geom;
pub mod history;
pub mod id;
pub mod model;
pub mod paint;
pub mod query;
#[cfg(feature = "render")]
pub mod render;
pub mod scene;
pub mod text;

mod serde_util;

pub use error::{Error, Result};
pub use id::{AssetId, NodeId, PageId};
pub use model::{Document, Node, NodeKind, Page};
