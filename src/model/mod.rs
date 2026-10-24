//! The document model: a [`Document`] holds [`Page`]s, each page holds a tree of [`Node`]s.
//!
//! Coordinates are in pixels (1 px = 1/96 in = 0.75 pt). Every node has a transform that
//! places its local coordinate space inside its parent's; shapes are defined in local space
//! with their bounding box starting at `(0, 0)`.

mod asset;
mod document;
mod frame;
mod node;
mod page;
mod shape;
mod text;

pub use asset::{Asset, AssetStore};
pub use document::{Document, NodeLocation};
pub use frame::FrameNode;
pub use node::{ImageNode, Node, NodeKind};
pub use page::Page;
pub use shape::{Geometry, ShapeNode};
pub use text::{TextAlign, TextNode, TextSizing, TextStyle, TextVerticalAlign};
