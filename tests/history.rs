use libforma::geom::Affine;
use libforma::history::{Command, History};
use libforma::model::{Geometry, NodeLocation};
use libforma::{Document, Node, NodeId, Page, PageId};

fn setup() -> (Document, PageId, NodeId, NodeId) {
    let a = Node::shape(Geometry::rect(10.0, 10.0)).with_name("a");
    let group = Node::group(vec![a.clone()]).with_name("g");
    let page = Page::new("p").with_child(group.clone());
    let page_id = page.id;
    (
        Document::new("doc").with_page(page),
        page_id,
        group.id,
        a.id,
    )
}

#[test]
fn every_command_undoes_and_redoes() {
    let (mut doc, page, group, a) = setup();
    let original = doc.clone();
    let mut updated = doc.node(a).unwrap().clone();
    updated.name = "renamed".into();
    let mut updated_group = doc.node(group).unwrap().clone();
    updated_group.opacity = 0.5;
    if let Some(children) = updated_group.children_mut() {
        children.clear();
    }
    let mut updated_page = doc.pages[0].clone();
    updated_page.name = "renamed".into();
    updated_page.children.clear();

    let commands = vec![
        Command::InsertNode {
            location: NodeLocation::top_level(page, 1),
            node: Node::shape(Geometry::ellipse(5.0, 5.0)),
        },
        Command::RemoveNode { id: a },
        Command::MoveNode {
            id: a,
            to: NodeLocation::top_level(page, 0),
        },
        Command::SetTransform {
            id: a,
            transform: Affine::translate((5.0, 5.0)),
        },
        Command::UpdateNode { node: updated },
        Command::UpdateNode {
            node: updated_group,
        },
        Command::InsertPage {
            index: 1,
            page: Page::new("p2"),
        },
        Command::RemovePage { id: page },
        Command::UpdatePage { page: updated_page },
    ];

    for command in commands {
        let mut history = History::new();
        history.execute(&mut doc, command.clone()).unwrap();
        let changed = doc.clone();
        assert_ne!(changed, original, "{command:?} changed nothing");
        assert!(history.undo(&mut doc).unwrap());
        assert_eq!(doc, original, "undo of {command:?}");
        assert!(history.redo(&mut doc).unwrap());
        assert_eq!(doc, changed, "redo of {command:?}");
        history.undo(&mut doc).unwrap();
    }
}

#[test]
fn update_group_keeps_children() {
    let (mut doc, _, group, a) = setup();
    let mut node = doc.node(group).unwrap().clone();
    node.children_mut().unwrap().clear();
    node.name = "new".into();
    History::new()
        .execute(&mut doc, Command::UpdateNode { node })
        .unwrap();
    assert_eq!(doc.node(group).unwrap().name, "new");
    assert!(doc.node(a).is_some());
}

#[test]
fn failed_move_leaves_document_unchanged() {
    let (mut doc, page, group, a) = setup();
    let original = doc.clone();
    let mut history = History::new();
    // Out of range.
    assert!(
        history
            .execute(
                &mut doc,
                Command::MoveNode {
                    id: a,
                    to: NodeLocation::top_level(page, 9)
                }
            )
            .is_err()
    );
    // Into itself.
    assert!(
        history
            .execute(
                &mut doc,
                Command::MoveNode {
                    id: group,
                    to: NodeLocation::in_group(page, group, 0)
                }
            )
            .is_err()
    );
    assert_eq!(doc, original);
    assert!(!history.can_undo());
}

#[test]
fn failed_batch_is_rolled_back() {
    let (mut doc, _, _, a) = setup();
    let original = doc.clone();
    let batch = Command::Batch(vec![
        Command::SetTransform {
            id: a,
            transform: Affine::scale(2.0),
        },
        Command::RemoveNode { id: a },
        Command::RemoveNode { id: a }, // fails: already removed
    ]);
    assert!(History::new().execute(&mut doc, batch).is_err());
    assert_eq!(doc, original);
}

#[test]
fn batch_undoes_in_one_step() {
    let (mut doc, _, _, a) = setup();
    let original = doc.clone();
    let mut history = History::new();
    history
        .execute(
            &mut doc,
            Command::Batch(vec![
                Command::SetTransform {
                    id: a,
                    transform: Affine::scale(2.0),
                },
                Command::RemoveNode { id: a },
            ]),
        )
        .unwrap();
    history.undo(&mut doc).unwrap();
    assert_eq!(doc, original);
    assert!(!history.can_undo());
}

#[test]
fn new_command_clears_redo_and_limit_is_respected() {
    let (mut doc, _, _, a) = setup();
    let mut history = History::with_limit(2);
    for i in 0..3 {
        history
            .execute(
                &mut doc,
                Command::SetTransform {
                    id: a,
                    transform: Affine::translate((i as f64, 0.0)),
                },
            )
            .unwrap();
    }
    assert!(history.undo(&mut doc).unwrap());
    assert!(history.undo(&mut doc).unwrap());
    assert!(!history.undo(&mut doc).unwrap());
    assert!(history.can_redo());
    history
        .execute(
            &mut doc,
            Command::SetTransform {
                id: a,
                transform: Affine::IDENTITY,
            },
        )
        .unwrap();
    assert!(!history.can_redo());
}

#[test]
fn merged_commands_undo_in_one_step() {
    let (mut doc, _, _, a) = setup();
    let original = doc.clone();
    let mut history = History::new();
    for i in 1..=5 {
        let command = Command::SetTransform {
            id: a,
            transform: Affine::translate((i as f64, 0.0)),
        };
        history.execute_merged(&mut doc, command, 7).unwrap();
    }
    history.end_merge();
    history
        .execute_merged(
            &mut doc,
            Command::SetTransform {
                id: a,
                transform: Affine::scale(2.0),
            },
            7,
        )
        .unwrap();
    assert!(history.undo(&mut doc).unwrap());
    assert_eq!(
        doc.node(a).unwrap().transform,
        Affine::translate((5.0, 0.0))
    );
    assert!(history.undo(&mut doc).unwrap());
    assert_eq!(doc, original);
    assert!(!history.can_undo());
}

#[test]
fn modified_flag_follows_saves_undo_and_merges() {
    let (mut doc, _, _, a) = setup();
    let mut history = History::new();
    assert!(!history.is_modified());
    let move_to = |x: f64| Command::SetTransform {
        id: a,
        transform: Affine::translate((x, 0.0)),
    };

    history.execute(&mut doc, move_to(1.0)).unwrap();
    assert!(history.is_modified());
    history.undo(&mut doc).unwrap();
    assert!(!history.is_modified());
    history.redo(&mut doc).unwrap();
    history.mark_saved();
    assert!(!history.is_modified());

    // Merging into the saved step changes the document, so it counts as modified.
    history.execute_merged(&mut doc, move_to(2.0), 1).unwrap();
    history.mark_saved();
    history.execute_merged(&mut doc, move_to(3.0), 1).unwrap();
    assert!(history.is_modified());
    history.undo(&mut doc).unwrap();
    assert!(history.is_modified());
}

#[test]
fn discard_reverts_without_redo() {
    let (mut doc, page, _, _) = setup();
    let original = doc.clone();
    let mut history = History::new();
    let node = Node::shape(Geometry::rect(1.0, 1.0));
    history
        .execute_merged(
            &mut doc,
            Command::InsertNode {
                location: NodeLocation::top_level(page, 0),
                node,
            },
            3,
        )
        .unwrap();
    assert!(history.discard(&mut doc).unwrap());
    assert_eq!(doc, original);
    assert!(!history.can_redo());
    assert!(!history.is_modified());
}

#[test]
fn merging_keeps_structural_changes_in_the_step() {
    let (mut doc, page, group, a) = setup();
    let original = doc.clone();
    let mut history = History::new();
    // A drag moves the node, then drops it out of its group: one undo step.
    for x in [1.0, 2.0, 3.0] {
        let command = Command::SetTransform {
            id: a,
            transform: Affine::translate((x, 0.0)),
        };
        history.execute_merged(&mut doc, command, 4).unwrap();
    }
    let drop = Command::Batch(vec![
        Command::MoveNode {
            id: a,
            to: NodeLocation::top_level(page, 1),
        },
        Command::SetTransform {
            id: a,
            transform: Affine::translate((30.0, 0.0)),
        },
    ]);
    history.execute_merged(&mut doc, drop, 4).unwrap();
    history.end_merge();
    assert_eq!(doc.locate(a).unwrap().parent, None);

    assert!(history.undo(&mut doc).unwrap());
    assert_eq!(doc, original);
    assert_eq!(doc.locate(a).unwrap().parent, Some(group));
    assert!(!history.can_undo());
    assert!(history.redo(&mut doc).unwrap());
    assert_eq!(doc.locate(a).unwrap().parent, None);
    assert_eq!(
        doc.node(a).unwrap().transform,
        Affine::translate((30.0, 0.0))
    );
}
