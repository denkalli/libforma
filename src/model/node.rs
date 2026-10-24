use kurbo::{Affine, Size, Vec2};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::id::{AssetId, NodeId};
use crate::paint::{BlendMode, Paint, Stroke};
use crate::serde_util;

use super::frame::FrameNode;
use super::shape::{Geometry, ShapeNode};
use super::text::{TextNode, TextStyle};

/// An element of the document tree.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(
        default = "serde_util::yes",
        skip_serializing_if = "serde_util::is_true"
    )]
    pub visible: bool,
    #[serde(default, skip_serializing_if = "serde_util::is_false")]
    pub locked: bool,
    /// Maps the node's local space into its parent's space.
    #[serde(
        default = "identity",
        with = "serde_util::affine",
        skip_serializing_if = "serde_util::is_identity"
    )]
    pub transform: Affine,
    #[serde(
        default = "serde_util::one",
        skip_serializing_if = "serde_util::is_one"
    )]
    pub opacity: f32,
    #[serde(default, skip_serializing_if = "serde_util::is_default")]
    pub blend: BlendMode,
    pub kind: NodeKind,
    /// Fields this version of libforma does not know about, kept so they survive a save.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

fn identity() -> Affine {
    Affine::IDENTITY
}

/// What a node is.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NodeKind {
    Group { children: Vec<Node> },
    Frame(FrameNode),
    Shape(ShapeNode),
    Text(TextNode),
    Image(ImageNode),
}

/// A raster image placed in the document.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct ImageNode {
    pub asset: AssetId,
    /// Display size in local units; the image is stretched to fill it.
    #[serde(with = "serde_util::size")]
    pub size: Size,
}

impl Node {
    /// Creates a visible node with a fresh id and default properties.
    pub fn new(kind: NodeKind) -> Self {
        Self {
            id: NodeId::new(),
            name: String::new(),
            visible: true,
            locked: false,
            transform: Affine::IDENTITY,
            opacity: 1.0,
            blend: BlendMode::Normal,
            kind,
            extra: Map::new(),
        }
    }

    pub fn group(children: Vec<Node>) -> Self {
        Self::new(NodeKind::Group { children })
    }

    /// An empty white frame of the given size.
    pub fn frame(name: impl Into<String>, width: f64, height: f64) -> Self {
        Self::new(NodeKind::Frame(FrameNode::new(Size::new(width, height)))).with_name(name)
    }

    /// Adds a child to a group or frame. Has no effect on other kinds of node.
    pub fn with_child(mut self, child: Node) -> Self {
        if let Some(children) = self.children_mut() {
            children.push(child);
        }
        self
    }

    /// A shape with no fill and no stroke.
    pub fn shape(geometry: Geometry) -> Self {
        Self::new(NodeKind::Shape(ShapeNode::new(geometry)))
    }

    pub fn text(content: impl Into<String>, style: TextStyle) -> Self {
        Self::new(NodeKind::Text(TextNode::new(content, style)))
    }

    pub fn image(asset: AssetId, size: impl Into<Size>) -> Self {
        Self::new(NodeKind::Image(ImageNode {
            asset,
            size: size.into(),
        }))
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    pub fn with_transform(mut self, transform: Affine) -> Self {
        self.transform = transform;
        self
    }

    /// Moves the node by `(x, y)` in its parent's space.
    pub fn at(mut self, x: f64, y: f64) -> Self {
        self.transform = Affine::translate(Vec2::new(x, y)) * self.transform;
        self
    }

    pub fn with_opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity;
        self
    }

    pub fn with_blend(mut self, blend: BlendMode) -> Self {
        self.blend = blend;
        self
    }

    /// Sets the fill of a shape. Has no effect on other kinds of node.
    pub fn with_fill(mut self, paint: impl Into<Paint>) -> Self {
        if let NodeKind::Shape(shape) = &mut self.kind {
            shape.fill = Some(paint.into());
        }
        self
    }

    /// Sets the stroke of a shape. Has no effect on other kinds of node.
    pub fn with_stroke(mut self, stroke: Stroke) -> Self {
        if let NodeKind::Shape(shape) = &mut self.kind {
            shape.stroke = Some(stroke);
        }
        self
    }

    /// A deep copy with new ids for the node and all its descendants, for duplicating and
    /// pasting.
    pub fn duplicate(&self) -> Self {
        let mut copy = self.clone();
        copy.reassign_ids();
        copy
    }

    fn reassign_ids(&mut self) {
        self.id = NodeId::new();
        if let Some(children) = self.children_mut() {
            for child in children {
                child.reassign_ids();
            }
        }
    }

    pub fn is_group(&self) -> bool {
        matches!(self.kind, NodeKind::Group { .. })
    }

    pub fn is_frame(&self) -> bool {
        matches!(self.kind, NodeKind::Frame(_))
    }

    /// Whether the node holds children (a group or a frame).
    pub fn is_container(&self) -> bool {
        self.is_group() || self.is_frame()
    }

    /// Children of a group or frame; empty for other kinds of node.
    pub fn children(&self) -> &[Node] {
        match &self.kind {
            NodeKind::Group { children } => children,
            NodeKind::Frame(frame) => &frame.children,
            _ => &[],
        }
    }

    /// Children of a group or frame, or `None` for other kinds of node.
    pub fn children_mut(&mut self) -> Option<&mut Vec<Node>> {
        match &mut self.kind {
            NodeKind::Group { children } => Some(children),
            NodeKind::Frame(frame) => Some(&mut frame.children),
            _ => None,
        }
    }

    /// Calls `f` on this node and all its descendants, parents first.
    pub fn walk<'a>(&'a self, f: &mut impl FnMut(&'a Node)) {
        f(self);
        for child in self.children() {
            child.walk(f);
        }
    }

    /// Finds this node or a descendant by id.
    pub fn find(&self, id: NodeId) -> Option<&Node> {
        if self.id == id {
            return Some(self);
        }
        self.children().iter().find_map(|c| c.find(id))
    }

    pub fn find_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        if self.id == id {
            return Some(self);
        }
        self.children_mut()?.iter_mut().find_map(|c| c.find_mut(id))
    }
}
