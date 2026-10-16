use swotvibe_core::{
    Command, Document, DocumentEngine, NodeId, NodeKind, NodePlacement, PageId, Position,
};

fn seed() -> (DocumentEngine, PageId, NodeId, NodeId) {
    let mut e = DocumentEngine::new();
    let p = PageId::new();
    let a = NodeId::new();
    let b = NodeId::new();
    e.apply(&[
        Command::CreatePage {
            id: p,
            name: "Page".into(),
        },
        Command::CreateNode {
            id: a,
            kind: NodeKind::Frame,
            name: Some("A".into()),
            parent: root(p),
        },
        Command::CreateNode {
            id: b,
            kind: NodeKind::Shape,
            name: Some("B".into()),
            parent: child(a),
        },
    ])
    .unwrap();
    (e, p, a, b)
}
fn root(page: PageId) -> NodePlacement {
    NodePlacement::PageRoot {
        page,
        position: Position::Last,
    }
}
fn child(parent: NodeId) -> NodePlacement {
    NodePlacement::Child {
        parent,
        position: Position::Last,
    }
}
fn same_content(a: &Document, b: &Document) {
    assert_eq!(a.pages(), b.pages());
    assert_eq!(a.assets(), b.assets());
    assert_eq!(a.node_count(), b.node_count());
    for n in a.nodes() {
        assert_eq!(Some(n), b.node(n.id));
        assert_eq!(a.node_extensions(n.id), b.node_extensions(n.id));
        assert_eq!(a.parent_of(n.id), b.parent_of(n.id));
        assert_eq!(a.page_of(n.id), b.page_of(n.id));
    }
    for p in a.pages() {
        assert_eq!(a.page_extensions(p.id), b.page_extensions(p.id));
    }
}
fn round_trip(mut e: DocumentEngine, commands: &[Command]) {
    let before = e.document().clone();
    e.apply(commands).unwrap();
    e.validate().unwrap();
    let after = e.document().clone();
    e.undo().expect("every accepted batch must be reversible");
    e.validate().unwrap();
    same_content(&before, e.document());
    e.redo().expect("every undo must be redoable");
    e.validate().unwrap();
    same_content(&after, e.document());
}
#[test]
fn moving_a_child_out_before_deleting_its_parent_round_trips() {
    let (e, p, a, b) = seed();
    round_trip(
        e,
        &[
            Command::MoveNode {
                id: b,
                parent: root(p),
            },
            Command::DeleteNode { id: a },
        ],
    );
}
#[test]
fn deleting_a_child_and_then_its_parent_round_trips() {
    let (e, _, a, b) = seed();
    round_trip(
        e,
        &[Command::DeleteNode { id: b }, Command::DeleteNode { id: a }],
    );
}
#[test]
fn creating_and_deleting_a_subtree_in_one_batch_round_trips() {
    let (e, _, a, _) = seed();
    let c = NodeId::new();
    round_trip(
        e,
        &[
            Command::CreateNode {
                id: c,
                kind: NodeKind::Text,
                name: Some("New".into()),
                parent: child(a),
            },
            Command::DeleteNode { id: a },
        ],
    );
}
#[test]
fn creating_and_deleting_the_same_identity_in_one_batch_round_trips() {
    let (e, p, _, _) = seed();
    let c = NodeId::new();
    round_trip(
        e,
        &[
            Command::CreateNode {
                id: c,
                kind: NodeKind::Shape,
                name: None,
                parent: root(p),
            },
            Command::DeleteNode { id: c },
        ],
    );
}
#[test]
fn removing_a_page_restores_its_name_and_order() {
    let (mut e, _, _, _) = seed();
    let p = PageId::new();
    let q = PageId::new();
    e.apply(&[
        Command::CreatePage {
            id: p,
            name: "Middle".into(),
        },
        Command::CreatePage {
            id: q,
            name: "Last".into(),
        },
    ])
    .unwrap();
    round_trip(e, &[Command::RemovePage { id: p }]);
}
#[test]
fn registering_an_asset_is_reversible() {
    let (e, _, _, _) = seed();
    round_trip(
        e,
        &[Command::CreateAsset {
            id: swotvibe_core::AssetId::new(),
            name: "Image".into(),
        }],
    );
}

#[test]
fn removal_preserves_extensions_and_reusing_an_identity_does_not_inherit_them() {
    let (e, _, a, b) = seed();
    let mut doc = e.into_document();
    doc.set_node_extensions(
        a,
        [("extra".into(), serde_json::json!({"data": ["original", 4]}))].into(),
    );
    doc.set_node_extensions(
        b,
        [("child-extra".into(), serde_json::json!("child"))].into(),
    );
    round_trip(
        DocumentEngine::with_document(doc.clone()),
        &[Command::DeleteNode { id: a }],
    );
    let mut e = DocumentEngine::with_document(doc);
    e.apply(&[
        Command::DeleteNode { id: b },
        Command::CreateNode {
            id: b,
            kind: NodeKind::Text,
            name: None,
            parent: child(a),
        },
    ])
    .unwrap();
    assert!(e.document().node_extensions(b).is_empty());
    e.undo().unwrap();
    assert_eq!(e.document().node(b).unwrap().kind, NodeKind::Shape);
    assert_eq!(e.document().node_extensions(b)["child-extra"], "child");
    e.redo().unwrap();
    assert!(e.document().node_extensions(b).is_empty());
}

#[test]
fn page_metadata_survives_removal_and_history() {
    let (e, _, _, _) = seed();
    let mut doc = e.into_document();
    let p = PageId::new();
    swotvibe_core::apply_batch_at_current(
        &mut doc,
        &[Command::CreatePage {
            id: p,
            name: "Metadata".into(),
        }],
    )
    .unwrap();
    doc.set_page_extensions(
        p,
        [("future".into(), serde_json::json!({"name":"kept"}))].into(),
    );
    round_trip(
        DocumentEngine::with_document(doc),
        &[Command::RemovePage { id: p }],
    );
}

#[test]
fn referenced_assets_cannot_be_removed_and_failed_batches_leave_history_unchanged() {
    let (mut e, p, _, _) = seed();
    let asset = swotvibe_core::AssetId::new();
    let image = NodeId::new();
    let mut props = swotvibe_core::NodeProps::default_for(NodeKind::Image);
    if let swotvibe_core::Content::Image(value) = &mut props.content {
        value.asset = Some(asset);
    }
    e.apply(&[
        Command::CreateAsset {
            id: asset,
            name: "Asset".into(),
        },
        Command::CreateNode {
            id: image,
            kind: NodeKind::Image,
            name: None,
            parent: root(p),
        },
        Command::SetNodeProps {
            id: image,
            props: Box::new(props),
        },
    ])
    .unwrap();
    let before = e.document().clone();
    let status = e.history().status();
    assert!(e.apply(&[Command::RemoveAsset { id: asset }]).is_err());
    same_content(&before, e.document());
    assert_eq!(e.history().status(), status);
    round_trip(
        e,
        &[
            Command::DeleteNode { id: image },
            Command::RemoveAsset { id: asset },
        ],
    );
}

#[test]
fn a_poisoned_command_plan_cannot_publish_its_valid_prefix() {
    let (e, _, _, b) = seed();
    let before = e.document().clone();
    let mut plan = swotvibe_core::CommandPlan::new(e.document());
    plan.push(Command::RenameNode {
        id: b,
        name: Some("staged".into()),
    })
    .unwrap();
    assert_eq!(plan.document().node(b).unwrap().name(), "staged");
    assert!(
        plan.push(Command::DeleteNode { id: NodeId::new() })
            .is_err()
    );
    assert!(plan.into_commands().is_err());
    same_content(&before, e.document());
}

#[test]
fn zero_byte_retention_commits_the_edit_and_reports_the_lost_undo() {
    let (mut e, _, _, b) = seed();
    e.history_mut().clear();
    e.history_mut().set_byte_capacity(Some(0));
    e.apply(&[Command::RenameNode {
        id: b,
        name: Some("Committed".into()),
    }])
    .unwrap();
    assert_eq!(e.document().node(b).unwrap().name(), "Committed");
    let status = e.history().status();
    assert_eq!(status.estimated_bytes, 0);
    assert_eq!(status.last_commit_retained, Some(false));
    assert_eq!(status.evicted_entries, 1);
    assert!(!e.can_undo());
}

#[test]
fn lowering_the_step_limit_counts_redo_and_preserves_the_nearest_future() {
    let (mut e, _, _, b) = seed();
    e.history_mut().clear();
    for name in ["One", "Two", "Three"] {
        e.apply(&[Command::RenameNode {
            id: b,
            name: Some(name.into()),
        }])
        .unwrap();
    }
    for _ in 0..3 {
        e.undo().unwrap();
    }
    e.history_mut().set_capacity(Some(1));
    assert_eq!(e.redo_depth(), 1);
    e.redo().unwrap();
    assert_eq!(e.document().node(b).unwrap().name(), "One");
    assert!(!e.can_redo());
}

#[test]
fn inverse_size_changes_during_undo_cannot_exceed_the_retention_budget() {
    let (mut e, _, _, b) = seed();
    e.history_mut().clear();
    e.apply(&[Command::RenameNode {
        id: b,
        name: Some("large".repeat(5000)),
    }])
    .unwrap();
    let bytes = e.history().estimated_bytes();
    e.history_mut().set_byte_capacity(Some(bytes));
    assert!(e.can_undo());
    e.undo().unwrap();
    assert_eq!(e.document().node(b).unwrap().name(), "B");
    assert!(e.history().estimated_bytes() <= bytes);
    assert!(!e.can_redo());
    assert!(e.history().status().evicted_entries > 0);
}

#[test]
fn a_large_metadata_subtree_is_charged_and_an_oversized_step_is_not_split() {
    let (e, _, a, _) = seed();
    let mut doc = e.into_document();
    doc.set_node_extensions(
        a,
        [(
            "payload".into(),
            serde_json::json!({"nested":["x".repeat(10000)]}),
        )]
        .into(),
    );
    let mut e = DocumentEngine::with_document(doc);
    e.history_mut().set_byte_capacity(Some(2048));
    e.apply(&[Command::DeleteNode { id: a }]).unwrap();
    assert!(e.document().node(a).is_none());
    assert!(!e.can_undo());
    assert_eq!(e.history().status().last_commit_retained, Some(false));
    assert_eq!(e.history().estimated_bytes(), 0);
}

#[test]
fn changesets_report_leaf_moves_and_metadata_replacement() {
    let (mut e, p, a, b) = seed();
    let commit = e
        .apply(&[Command::MoveNode {
            id: b,
            parent: root(p),
        }])
        .unwrap();
    assert!(commit.change_set().moved_nodes().any(|id| id == b));
    assert!(commit.change_set().changed_nodes().any(|id| id == a));
    let mut doc = e.into_document();
    doc.set_node_extensions(b, [("previous".into(), serde_json::json!(true))].into());
    let mut e = DocumentEngine::with_document(doc);
    let value = e.document().node(b).unwrap().clone();
    let commit = e
        .apply(&[
            Command::DeleteNode { id: b },
            Command::RestoreNode {
                node: Box::new(value),
                parent: root(p),
                extensions: Default::default(),
            },
        ])
        .unwrap();
    assert!(commit.change_set().changed_nodes().any(|id| id == b));
    assert!(!commit.change_set().is_empty());
}

#[test]
fn failed_batches_preserve_both_history_stacks_and_retention_status() {
    let (mut e, _, _, b) = seed();
    e.apply(&[Command::RenameNode {
        id: b,
        name: Some("Second".into()),
    }])
    .unwrap();
    e.undo().unwrap();
    let status = e.history().status();
    let revision = e.revision();
    assert!(
        e.apply(&[
            Command::RenameNode {
                id: b,
                name: Some("prefix".into())
            },
            Command::DeleteNode { id: NodeId::new() }
        ])
        .is_err()
    );
    assert_eq!(e.history().status(), status);
    assert_eq!(e.revision(), revision);
    e.redo().unwrap();
    assert_eq!(e.document().node(b).unwrap().name(), "Second");
}

#[test]
fn zero_step_retention_clears_redo_as_well_as_undo() {
    let (mut e, _, _, b) = seed();
    e.apply(&[Command::RenameNode {
        id: b,
        name: Some("Second".into()),
    }])
    .unwrap();
    e.undo().unwrap();
    e.history_mut().set_capacity(Some(0));
    assert!(!e.can_undo());
    assert!(!e.can_redo());
    assert_eq!(e.history().estimated_bytes(), 0);
}

#[test]
fn compound_command_sequences_round_trip_under_a_fixed_state_machine() {
    let mut random = 0x71c4d19u64;
    for case in 0..128 {
        let (e, initial_page, _, _) = seed();
        let mut plan = swotvibe_core::CommandPlan::new(e.document());
        for step in 0..24 {
            random = random
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let mut ids: Vec<_> = plan.document().nodes().map(|n| n.id).collect();
            ids.sort();
            let index = (random as usize) % ids.len().max(1);
            let command = if ids.is_empty() || random.is_multiple_of(6) {
                let kind = if random.is_multiple_of(2) {
                    NodeKind::Frame
                } else {
                    NodeKind::Shape
                };
                Command::CreateNode {
                    id: NodeId::new(),
                    kind,
                    name: Some(format!("{case}/{step}")),
                    parent: root(initial_page),
                }
            } else {
                let id = ids[index];
                match random % 6 {
                    1 => Command::DeleteNode { id },
                    2 => Command::MoveNode {
                        id,
                        parent: root(initial_page),
                    },
                    3 => Command::RenameNode {
                        id,
                        name: Some(format!("rename {case}/{step}")),
                    },
                    4 => {
                        let mut props = plan.document().node(id).unwrap().props.clone();
                        props.size = swotvibe_core::Size::new((step + 1) as f64, 17.0).unwrap();
                        Command::SetNodeProps {
                            id,
                            props: Box::new(props),
                        }
                    }
                    _ => {
                        let containers: Vec<_> = ids
                            .iter()
                            .copied()
                            .filter(|candidate| {
                                *candidate != id
                                    && plan
                                        .document()
                                        .node(*candidate)
                                        .unwrap()
                                        .kind
                                        .can_own_children()
                                    && !plan.document().is_descendant_of(*candidate, id)
                            })
                            .collect();
                        if containers.is_empty() {
                            Command::RenameNode { id, name: None }
                        } else {
                            Command::MoveNode {
                                id,
                                parent: child(containers[(random as usize) % containers.len()]),
                            }
                        }
                    }
                }
            };
            plan.push(command).unwrap();
        }
        round_trip(e, &plan.into_commands().unwrap());
    }
}
