//! Edits as reversible commands, and the undo/redo history built on them.
//!
//! Every change to a [`Document`] should go through [`History::execute`]. Applying a
//! [`Command`] returns its inverse, which is what gets stored for undo.

use kurbo::Affine;

use crate::error::{Error, Result};
use crate::id::{NodeId, PageId};
use crate::model::{Document, Node, NodeLocation, Page};

/// A reversible change to a document.
#[derive(Clone, PartialEq, Debug)]
pub enum Command {
    /// Inserts a node (with its descendants) at a location.
    InsertNode {
        location: NodeLocation,
        node: Node,
    },
    /// Removes a node and its descendants.
    RemoveNode {
        id: NodeId,
    },
    /// Moves a node to a new location. `to.index` is the index after the node has been
    /// removed from its current place.
    MoveNode {
        id: NodeId,
        to: NodeLocation,
    },
    /// Replaces the node's transform.
    SetTransform {
        id: NodeId,
        transform: Affine,
    },
    /// Replaces a node's properties (everything but its id and, for groups, its children).
    UpdateNode {
        node: Node,
    },
    InsertPage {
        index: usize,
        page: Page,
    },
    RemovePage {
        id: PageId,
    },
    /// Replaces a page's properties (everything but its id and its nodes).
    UpdatePage {
        page: Page,
    },
    /// Several commands applied in order and undone as one step.
    Batch(Vec<Command>),
}

impl Command {
    /// Applies the command and returns the command that undoes it.
    ///
    /// On error the document is left unchanged.
    pub fn apply(self, doc: &mut Document) -> Result<Command> {
        match self {
            Command::InsertNode { location, node } => {
                let id = node.id;
                doc.insert_node(location, node)?;
                Ok(Command::RemoveNode { id })
            }
            Command::RemoveNode { id } => {
                let (node, location) = doc.remove_node(id)?;
                Ok(Command::InsertNode { location, node })
            }
            Command::MoveNode { id, to } => {
                if to.parent == Some(id) || to.parent.is_some_and(|p| is_descendant(doc, p, id)) {
                    return Err(Error::InvalidLocation(
                        "cannot move a node into itself".into(),
                    ));
                }
                let (node, from) = doc.remove_node(id)?;
                if let Err(err) = doc.check_insert(to, &node) {
                    doc.insert_node(from, node)
                        .expect("restoring a node to where it was must succeed");
                    return Err(err);
                }
                doc.insert_node(to, node)?;
                Ok(Command::MoveNode { id, to: from })
            }
            Command::SetTransform { id, transform } => {
                let node = doc.node_mut(id).ok_or(Error::NodeNotFound(id))?;
                let old = std::mem::replace(&mut node.transform, transform);
                Ok(Command::SetTransform { id, transform: old })
            }
            Command::UpdateNode { mut node } => {
                let target = doc.node_mut(node.id).ok_or(Error::NodeNotFound(node.id))?;
                if let (Some(new_children), Some(old_children)) =
                    (node.children_mut(), target.children_mut())
                {
                    std::mem::swap(new_children, old_children);
                } else if target.is_container() != node.is_container() {
                    return Err(Error::InvalidLocation(
                        "UpdateNode cannot turn a container into another kind of node".into(),
                    ));
                }
                std::mem::swap(target, &mut node);
                // `node` now holds the old properties; give it back an empty child list so
                // the inverse does not carry a copy of the subtree.
                if let Some(children) = node.children_mut() {
                    children.clear();
                }
                Ok(Command::UpdateNode { node })
            }
            Command::InsertPage { index, page } => {
                if index > doc.pages.len() {
                    return Err(Error::InvalidLocation(format!(
                        "page index {index} out of range"
                    )));
                }
                if doc.page(page.id).is_some() {
                    return Err(Error::InvalidLocation(format!(
                        "page id {} is already in use",
                        page.id
                    )));
                }
                let id = page.id;
                doc.pages.insert(index, page);
                Ok(Command::RemovePage { id })
            }
            Command::RemovePage { id } => {
                let index = doc.page_index(id).ok_or(Error::PageNotFound(id))?;
                let page = doc.pages.remove(index);
                Ok(Command::InsertPage { index, page })
            }
            Command::UpdatePage { mut page } => {
                let target = doc.page_mut(page.id).ok_or(Error::PageNotFound(page.id))?;
                std::mem::swap(&mut target.children, &mut page.children);
                std::mem::swap(target, &mut page);
                page.children.clear();
                Ok(Command::UpdatePage { page })
            }
            Command::Batch(commands) => {
                let mut inverses = Vec::with_capacity(commands.len());
                for command in commands {
                    match command.apply(doc) {
                        Ok(inverse) => inverses.push(inverse),
                        Err(err) => {
                            // Roll back what was already applied.
                            for inverse in inverses.into_iter().rev() {
                                inverse
                                    .apply(doc)
                                    .expect("rolling back a batch must succeed");
                            }
                            return Err(err);
                        }
                    }
                }
                inverses.reverse();
                Ok(Command::Batch(inverses))
            }
        }
    }
}

fn is_descendant(doc: &Document, candidate: NodeId, ancestor: NodeId) -> bool {
    doc.node(ancestor)
        .is_some_and(|a| a.children().iter().any(|c| c.find(candidate).is_some()))
}

/// Undo and redo stacks.
///
/// Continuous edits (dragging a shape, moving a slider) can be merged into a single undo step
/// with [`History::execute_merged`]. The history also remembers which state was last saved,
/// so an editor can tell whether the document has unsaved changes.
#[derive(Clone, Debug, Default)]
pub struct History {
    undo: Vec<Entry>,
    redo: Vec<Entry>,
    /// Maximum number of undo steps kept. `None` means unlimited.
    limit: Option<usize>,
    /// Source of entry ids.
    next_id: u64,
    /// Id of the top undo entry when the document was saved; `None` for the initial state.
    saved: Option<Option<u64>>,
    /// Merge key of the top undo entry while merging is still open.
    merging: Option<u64>,
}

#[derive(Clone, Debug)]
struct Entry {
    id: u64,
    command: Command,
}

impl History {
    pub fn new() -> Self {
        Self {
            saved: Some(None),
            ..Self::default()
        }
    }

    /// A history that keeps at most `limit` undo steps.
    pub fn with_limit(limit: usize) -> Self {
        Self {
            limit: Some(limit),
            ..Self::new()
        }
    }

    /// Applies a command and records it for undo. Clears the redo stack.
    pub fn execute(&mut self, doc: &mut Document, command: Command) -> Result<()> {
        let inverse = command.apply(doc)?;
        self.merging = None;
        self.push(inverse);
        Ok(())
    }

    /// Applies a command, merging it with the previous one when both use the same `key` and
    /// no other command came in between. Undo then reverts all merged commands at once.
    ///
    /// Merging is cheap for continuous edits: when the stored inverse already restores
    /// everything the new command changes (the transform of the same nodes during a drag),
    /// nothing more is stored. Other commands, such as moving a node to another parent at the
    /// end of a drag, are added to the step.
    ///
    /// Call [`end_merge`](Self::end_merge) when the continuous edit is over (for example
    /// when the mouse button is released).
    pub fn execute_merged(&mut self, doc: &mut Document, command: Command, key: u64) -> Result<()> {
        let inverse = command.apply(doc)?;
        if self.merging == Some(key) && !self.undo.is_empty() {
            // The entry now describes a different state, so it gets a new id.
            let id = self.fresh_id();
            if let Some(top) = self.undo.last_mut() {
                top.id = id;
                if !restores(&top.command, &inverse) {
                    // Undo the newer command first, then the older ones.
                    let older = std::mem::replace(&mut top.command, Command::Batch(Vec::new()));
                    top.command = Command::Batch(vec![inverse, older]);
                }
            }
            self.redo.clear();
        } else {
            self.push(inverse);
            self.merging = Some(key);
        }
        Ok(())
    }

    /// Stops merging: the next command starts a new undo step.
    pub fn end_merge(&mut self) {
        self.merging = None;
    }

    fn push(&mut self, command: Command) {
        let id = self.fresh_id();
        self.undo.push(Entry { id, command });
        if let Some(limit) = self.limit
            && self.undo.len() > limit
        {
            let dropped = self.undo.remove(0);
            if self.saved == Some(Some(dropped.id)) {
                // The saved state can no longer be reached by undoing.
                self.saved = None;
            }
        }
        self.redo.clear();
        if let Some(Some(saved)) = self.saved
            && self.redo.iter().any(|e| e.id == saved)
        {
            self.saved = None;
        }
    }

    fn fresh_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    /// Undoes the last command. Returns `false` if there was nothing to undo.
    pub fn undo(&mut self, doc: &mut Document) -> Result<bool> {
        self.merging = None;
        let Some(entry) = self.undo.pop() else {
            return Ok(false);
        };
        let command = entry.command.apply(doc)?;
        self.redo.push(Entry {
            id: entry.id,
            command,
        });
        Ok(true)
    }

    /// Reverts the last undo step without making it redoable, for abandoning an edit in
    /// progress (such as a path the user cancelled while drawing it).
    pub fn discard(&mut self, doc: &mut Document) -> Result<bool> {
        self.merging = None;
        let Some(entry) = self.undo.pop() else {
            return Ok(false);
        };
        entry.command.apply(doc)?;
        Ok(true)
    }

    /// Redoes the last undone command. Returns `false` if there was nothing to redo.
    pub fn redo(&mut self, doc: &mut Document) -> Result<bool> {
        self.merging = None;
        let Some(entry) = self.redo.pop() else {
            return Ok(false);
        };
        let command = entry.command.apply(doc)?;
        self.undo.push(Entry {
            id: entry.id,
            command,
        });
        Ok(true)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Records the current state as saved.
    ///
    /// An open merge stays open, so saving in the middle of a drag (autosave) does not split
    /// its undo step.
    pub fn mark_saved(&mut self) {
        self.saved = Some(self.undo.last().map(|e| e.id));
    }

    /// Whether the document differs from the state recorded by [`mark_saved`](Self::mark_saved)
    /// (or from the initial state, if it was never saved).
    pub fn is_modified(&self) -> bool {
        self.saved != Some(self.undo.last().map(|e| e.id))
    }

    pub fn clear(&mut self) {
        *self = Self {
            limit: self.limit,
            ..Self::new()
        };
    }
}
/// What an inverse command puts back, as far as merging is concerned.
#[derive(Clone, Copy, PartialEq)]
enum Restored {
    Transform(NodeId),
    /// The whole node but its children, transform included.
    Node(NodeId),
    Page(PageId),
    /// The node is removed, so any change to it is undone too.
    Removed(NodeId),
    PageRemoved(PageId),
}

/// Collects what `command` puts back. Returns `false` if it changes the structure of the
/// document (inserts, removes or moves nodes or pages).
fn restored(command: &Command, out: &mut Vec<Restored>) -> bool {
    match command {
        Command::SetTransform { id, .. } => out.push(Restored::Transform(*id)),
        Command::UpdateNode { node } => out.push(Restored::Node(node.id)),
        Command::UpdatePage { page } => out.push(Restored::Page(page.id)),
        Command::RemoveNode { id } => {
            out.push(Restored::Removed(*id));
            return false;
        }
        Command::RemovePage { id } => {
            out.push(Restored::PageRemoved(*id));
            return false;
        }
        Command::Batch(commands) => {
            let mut only_setters = true;
            for command in commands {
                only_setters &= restored(command, out);
            }
            return only_setters;
        }
        Command::InsertNode { .. } | Command::MoveNode { .. } | Command::InsertPage { .. } => {
            return false;
        }
    }
    true
}

/// Whether undoing with `older` (an inverse that runs after `newer`) already restores
/// everything `newer` would, so `newer` can be dropped from a merged step.
fn restores(older: &Command, newer: &Command) -> bool {
    let mut new = Vec::new();
    if !restored(newer, &mut new) {
        return false;
    }
    let mut old = Vec::new();
    restored(older, &mut old);
    new.iter().all(|r| {
        old.contains(r)
            || match r {
                Restored::Transform(id) => {
                    old.contains(&Restored::Node(*id)) || old.contains(&Restored::Removed(*id))
                }
                Restored::Node(id) => old.contains(&Restored::Removed(*id)),
                Restored::Page(id) => old.contains(&Restored::PageRemoved(*id)),
                Restored::Removed(_) | Restored::PageRemoved(_) => false,
            }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Geometry;

    #[test]
    fn drawing_stays_one_small_undo_entry() {
        let page = Page::new("p");
        let page_id = page.id;
        let mut doc = Document::new("doc").with_page(page);
        let original = doc.clone();
        let mut history = History::new();
        let node = Node::shape(Geometry::rect(1.0, 1.0));
        let id = node.id;
        let location = NodeLocation::top_level(page_id, 0);
        history
            .execute_merged(&mut doc, Command::InsertNode { location, node }, 9)
            .unwrap();
        for i in 1..=50 {
            let mut node = doc.node(id).unwrap().clone();
            node.transform = Affine::translate((f64::from(i), 0.0));
            history
                .execute_merged(&mut doc, Command::UpdateNode { node }, 9)
                .unwrap();
        }
        history.end_merge();
        // The removal that undoes the insertion covers every later change to the node.
        assert_eq!(history.undo.len(), 1);
        assert_eq!(history.undo[0].command, Command::RemoveNode { id });
        assert!(history.undo(&mut doc).unwrap());
        assert_eq!(doc, original);
    }
}
