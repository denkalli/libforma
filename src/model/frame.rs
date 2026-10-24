use kurbo::Size;
use serde::{Deserialize, Serialize};

use crate::paint::Paint;
use crate::serde_util;

use super::node::Node;

/// A frame (artboard): a fixed-size area on the infinite page that holds other nodes.
///
/// Frames give a design its format (a 1080 × 1080 post, an A4 sheet, a slide) and are what
/// gets exported. They are drawn with an optional fill and, by default, hide whatever of
/// their children extends past their edges.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct FrameNode {
    /// The frame covers the box `(0, 0) - size` in its local space.
    #[serde(with = "serde_util::size")]
    pub size: Size,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<Paint>,
    /// Hide children outside the frame.
    #[serde(
        default = "serde_util::yes",
        skip_serializing_if = "serde_util::is_true"
    )]
    pub clip: bool,
    /// Child nodes, back to front.
    #[serde(default)]
    pub children: Vec<Node>,
}

impl FrameNode {
    /// An empty frame with a white fill that clips its content.
    pub fn new(size: Size) -> Self {
        Self {
            size,
            fill: Some(Paint::solid(crate::paint::Color::WHITE)),
            clip: true,
            children: Vec::new(),
        }
    }
}
