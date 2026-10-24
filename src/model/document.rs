use std::collections::{BTreeSet, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::error::{Error, Result};
use crate::id::{AssetId, NodeId, PageId};

use super::asset::AssetStore;
use super::node::{Node, NodeKind};
use super::page::Page;

/// A Forma document.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct Document {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    pub pages: Vec<Page>,
    /// Images and other binary content. Stored next to `document.json` in a `.forma` file,
    /// not inside it.
    #[serde(skip)]
    pub assets: AssetStore,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Where a node sits in the tree: inside `parent` (or at the top level of `page` when
/// `parent` is `None`), at position `index` among its siblings.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct NodeLocation {
    pub page: PageId,
    pub parent: Option<NodeId>,
    pub index: usize,
}

impl NodeLocation {
    /// Top level of a page.
    pub fn top_level(page: PageId, index: usize) -> Self {
        Self {
            page,
            parent: None,
            index,
        }
    }

    /// Inside a group.
    pub fn in_group(page: PageId, group: NodeId, index: usize) -> Self {
        Self {
            page,
            parent: Some(group),
            index,
        }
    }
}

impl Document {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            ..Self::default()
        }
    }

    pub fn with_page(mut self, page: Page) -> Self {
        self.pages.push(page);
        self
    }

    pub fn page(&self, id: PageId) -> Option<&Page> {
        self.pages.iter().find(|p| p.id == id)
    }

    pub fn page_mut(&mut self, id: PageId) -> Option<&mut Page> {
        self.pages.iter_mut().find(|p| p.id == id)
    }

    pub fn page_index(&self, id: PageId) -> Option<usize> {
        self.pages.iter().position(|p| p.id == id)
    }

    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.pages.iter().find_map(|p| p.find(id))
    }

    pub fn node_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.pages.iter_mut().find_map(|p| p.find_mut(id))
    }

    /// Finds where a node is.
    pub fn locate(&self, id: NodeId) -> Option<NodeLocation> {
        fn search(
            siblings: &[Node],
            page: PageId,
            parent: Option<NodeId>,
            id: NodeId,
        ) -> Option<NodeLocation> {
            for (index, node) in siblings.iter().enumerate() {
                if node.id == id {
                    return Some(NodeLocation {
                        page,
                        parent,
                        index,
                    });
                }
                if let Some(found) = search(node.children(), page, Some(node.id), id) {
                    return Some(found);
                }
            }
            None
        }
        self.pages
            .iter()
            .find_map(|p| search(&p.children, p.id, None, id))
    }

    /// The sibling list a location points into.
    pub fn siblings_mut(&mut self, page: PageId, parent: Option<NodeId>) -> Result<&mut Vec<Node>> {
        let page = self.page_mut(page).ok_or(Error::PageNotFound(page))?;
        match parent {
            None => Ok(&mut page.children),
            Some(parent) => {
                let node = page.find_mut(parent).ok_or(Error::NodeNotFound(parent))?;
                node.children_mut().ok_or_else(|| {
                    Error::InvalidLocation(format!("node {parent} is not a group or frame"))
                })
            }
        }
    }

    /// The sibling list a location points into.
    pub fn siblings(&self, page: PageId, parent: Option<NodeId>) -> Result<&[Node]> {
        let page = self.page(page).ok_or(Error::PageNotFound(page))?;
        match parent {
            None => Ok(&page.children),
            Some(parent) => {
                let node = page.find(parent).ok_or(Error::NodeNotFound(parent))?;
                if node.is_container() {
                    Ok(node.children())
                } else {
                    Err(Error::InvalidLocation(format!(
                        "node {parent} is not a group or frame"
                    )))
                }
            }
        }
    }

    /// Checks that [`insert_node`](Self::insert_node) would succeed, without changing anything.
    pub fn check_insert(&self, location: NodeLocation, node: &Node) -> Result<()> {
        let siblings = self.siblings(location.page, location.parent)?;
        if location.index > siblings.len() {
            return Err(Error::InvalidLocation(format!(
                "index {} out of range (len {})",
                location.index,
                siblings.len()
            )));
        }
        let mut seen = HashSet::new();
        let mut duplicate = None;
        node.walk(&mut |n| {
            if duplicate.is_none() && (!seen.insert(n.id) || self.node(n.id).is_some()) {
                duplicate = Some(n.id);
            }
        });
        match duplicate {
            Some(id) => Err(Error::InvalidLocation(format!(
                "node id {id} is already in use"
            ))),
            None => Ok(()),
        }
    }

    /// Inserts a node (and its descendants) at a location.
    ///
    /// Fails if the location does not exist or if any id in the subtree is already used.
    pub fn insert_node(&mut self, location: NodeLocation, node: Node) -> Result<()> {
        self.check_insert(location, &node)?;
        self.siblings_mut(location.page, location.parent)?
            .insert(location.index, node);
        Ok(())
    }

    /// Removes a node and returns it with the location it had.
    pub fn remove_node(&mut self, id: NodeId) -> Result<(Node, NodeLocation)> {
        let location = self.locate(id).ok_or(Error::NodeNotFound(id))?;
        let siblings = self.siblings_mut(location.page, location.parent)?;
        Ok((siblings.remove(location.index), location))
    }

    /// Calls `f` on every node of every page.
    pub fn walk<'a>(&'a self, f: &mut impl FnMut(&'a Node)) {
        for page in &self.pages {
            page.walk(f);
        }
    }

    /// Assets referenced by at least one node.
    pub fn used_assets(&self) -> BTreeSet<AssetId> {
        let mut used = BTreeSet::new();
        self.walk(&mut |node| {
            if let NodeKind::Image(image) = &node.kind {
                used.insert(image.asset);
            }
        });
        used
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Geometry;

    fn sample() -> (Document, PageId, NodeId, NodeId) {
        let rect = Node::shape(Geometry::rect(10.0, 10.0));
        let rect_id = rect.id;
        let group = Node::group(vec![rect]);
        let group_id = group.id;
        let page = Page::new("p").with_child(group);
        let page_id = page.id;
        (
            Document::new("t").with_page(page),
            page_id,
            group_id,
            rect_id,
        )
    }

    #[test]
    fn locate_nested_node() {
        let (doc, page, group, rect) = sample();
        assert_eq!(doc.locate(group), Some(NodeLocation::top_level(page, 0)));
        assert_eq!(
            doc.locate(rect),
            Some(NodeLocation::in_group(page, group, 0))
        );
    }

    #[test]
    fn remove_then_insert_restores_document() {
        let (mut doc, _, _, rect) = sample();
        let original = doc.clone();
        let (node, location) = doc.remove_node(rect).unwrap();
        assert!(doc.node(rect).is_none());
        doc.insert_node(location, node).unwrap();
        assert_eq!(doc, original);
    }

    #[test]
    fn duplicate_ids_are_rejected() {
        let (mut doc, page, _, rect) = sample();
        let copy = doc.node(rect).unwrap().clone();
        assert!(
            doc.insert_node(NodeLocation::top_level(page, 0), copy)
                .is_err()
        );
    }

    #[test]
    fn cannot_insert_into_non_group() {
        let (mut doc, page, _, rect) = sample();
        let node = Node::shape(Geometry::rect(1.0, 1.0));
        let err = doc.insert_node(NodeLocation::in_group(page, rect, 0), node);
        assert!(matches!(err, Err(Error::InvalidLocation(_))));
    }
}
