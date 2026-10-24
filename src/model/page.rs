use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::id::{NodeId, PageId};

use super::node::Node;

/// A page: an infinite canvas holding a tree of nodes.
///
/// Fixed formats (a post, an A4 sheet) are [frames](super::FrameNode) placed on the page.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Page {
    pub id: PageId,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// Top-level nodes, from back to front.
    #[serde(default)]
    pub children: Vec<Node>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Page {
    /// An empty page.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: PageId::new(),
            name: name.into(),
            children: Vec::new(),
            extra: Map::new(),
        }
    }

    pub fn with_child(mut self, node: Node) -> Self {
        self.children.push(node);
        self
    }

    pub fn find(&self, id: NodeId) -> Option<&Node> {
        self.children.iter().find_map(|n| n.find(id))
    }

    pub fn find_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.children.iter_mut().find_map(|n| n.find_mut(id))
    }

    /// Calls `f` on every node of the page, parents first, back to front.
    pub fn walk<'a>(&'a self, f: &mut impl FnMut(&'a Node)) {
        for node in &self.children {
            node.walk(f);
        }
    }
}
