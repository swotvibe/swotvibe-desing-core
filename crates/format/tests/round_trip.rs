//! Round-trip and integrity tests for the persisted schema.

use std::collections::BTreeMap;

use serde_json::json;
use swotvibe_core::{Command, Document, NodeKind, NodePlacement, Position, apply_batch_at_current};
use swotvibe_format::{
    DtoDocument, ImportError, SCHEMA_VERSION, export, from_json, import, import_migrated,
    migrate_to_current, to_json,
};

/// Builds a document containing a page with a frame holding two shapes.
fn sample_document() -> (Document, Vec<String>) {
    let mut doc = Document::new();
    let frame = swotvibe_core::NodeId::new();
    let a = swotvibe_core::NodeId::new();
    let b = swotvibe_core::NodeId::new();
    let page = swotvibe_core::PageId::new();

    apply_batch_at_current(
        &mut doc,
        &[
            Command::CreatePage {
                id: page,
                name: "Page 1".into(),
            },
            Command::CreateNode {
                id: frame,
                kind: NodeKind::Frame,
                name: Some("frame".into()),
                parent: NodePlacement::PageRoot {
                    page,
                    position: Position::Last,
                },
            },
            Command::CreateNode {
                id: a,
                kind: NodeKind::Shape,
                name: Some("a".into()),
                parent: NodePlacement::Child {
                    parent: frame,
                    position: Position::Last,
                },
            },
            Command::CreateNode {
                id: b,
                kind: NodeKind::Shape,
                name: None,
                parent: NodePlacement::Child {
                    parent: frame,
                    position: Position::Last,
                },
            },
        ],
    )
    .expect("sample document builds");

    (doc, vec![a.to_string(), b.to_string()])
}

#[test]
fn export_then_import_preserves_structure() {
    let (doc, children) = sample_document();
    let dto = export(&doc);
    let restored = import(&dto).expect("a document this crate exported imports cleanly");

    assert_eq!(restored.id(), doc.id());
    assert_eq!(restored.pages().len(), doc.pages().len());
    assert_eq!(restored.node_count(), doc.node_count());

    let frame = doc
        .pages()
        .first()
        .and_then(|p| p.roots().first().copied())
        .expect("the sample has a frame root");
    let restored_frame = restored
        .pages()
        .first()
        .and_then(|p| p.roots().first().copied())
        .expect("the restored document has a frame root");
    assert_eq!(restored_frame, frame);
    assert_eq!(restored.node(frame).unwrap().children().len(), 2);
    assert_eq!(
        restored
            .node(frame)
            .unwrap()
            .children()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        children
    );
}

#[test]
fn child_order_is_preserved_exactly() {
    let (doc, children) = sample_document();
    let restored = import(&export(&doc)).expect("round trip succeeds");

    let frame = restored.pages()[0].roots()[0];
    let restored_order: Vec<String> = restored
        .node(frame)
        .unwrap()
        .children()
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(restored_order, children);
}

#[test]
fn page_root_order_is_preserved() {
    let mut doc = Document::new();
    let page = swotvibe_core::PageId::new();
    let first = swotvibe_core::NodeId::new();
    let second = swotvibe_core::NodeId::new();
    apply_batch_at_current(
        &mut doc,
        &[
            Command::CreatePage {
                id: page,
                name: "P".into(),
            },
            Command::CreateNode {
                id: first,
                kind: NodeKind::Shape,
                name: None,
                parent: NodePlacement::PageRoot {
                    page,
                    position: Position::Last,
                },
            },
            Command::CreateNode {
                id: second,
                kind: NodeKind::Shape,
                name: None,
                parent: NodePlacement::PageRoot {
                    page,
                    position: Position::Last,
                },
            },
        ],
    )
    .unwrap();

    let restored = import(&export(&doc)).unwrap();
    assert_eq!(restored.pages()[0].roots(), &[first, second]);
}

#[test]
fn assets_round_trip() {
    let mut doc = Document::new();
    let asset = swotvibe_core::AssetId::new();
    apply_batch_at_current(
        &mut doc,
        &[Command::CreateAsset {
            id: asset,
            name: "logo".into(),
        }],
    )
    .unwrap();

    let restored = import(&export(&doc)).unwrap();
    assert_eq!(restored.assets().len(), 1);
    assert_eq!(restored.asset(asset).unwrap().name, "logo");
}

#[test]
fn a_fresh_document_json_round_trips() {
    let (doc, _) = sample_document();
    let bytes = to_json(&export(&doc)).expect("serialization succeeds");
    let parsed = from_json(&bytes).expect("parsing succeeds");
    let restored = import(&parsed).expect("import succeeds");

    assert_eq!(restored.id(), doc.id());
    assert_eq!(restored.node_count(), doc.node_count());
}

#[test]
fn json_is_stable_across_a_double_round_trip() {
    let (doc, _) = sample_document();
    let first = to_json(&export(&doc)).unwrap();
    let parsed = from_json(&first).unwrap();
    let second = to_json(&export(&import(&parsed).unwrap())).unwrap();
    assert_eq!(first, second, "save/load/save must be byte-stable");
}

#[test]
fn an_empty_document_round_trips() {
    let doc = Document::new();
    let restored = import(&export(&doc)).unwrap();
    assert_eq!(restored.id(), doc.id());
    assert!(restored.pages().is_empty());
    assert_eq!(restored.node_count(), 0);
}

#[test]
fn a_newer_schema_is_refused_with_its_version() {
    let (doc, _) = sample_document();
    let mut dto = export(&doc);
    dto.schema_version = SCHEMA_VERSION + 5;

    match import(&dto) {
        Err(ImportError::UnsupportedFutureVersion { found, supported }) => {
            assert_eq!(found, SCHEMA_VERSION + 5);
            assert_eq!(supported, SCHEMA_VERSION);
        }
        other => panic!("expected a future-version error, got {other:?}"),
    }
}

#[test]
fn a_newer_schema_is_refused_by_the_migration_entry_point() {
    let (doc, _) = sample_document();
    let mut dto = export(&doc);
    dto.schema_version = SCHEMA_VERSION + 1;
    assert!(matches!(
        migrate_to_current(dto),
        Err(ImportError::UnsupportedFutureVersion { .. })
    ));
}

#[test]
fn the_current_schema_needs_no_migration() {
    let (doc, _) = sample_document();
    let dto = export(&doc);
    assert!(!swotvibe_format::needs_migration(&dto));
    assert_eq!(migrate_to_current(dto.clone()).unwrap(), dto);
}

#[test]
fn migrate_and_import_reaches_the_runtime_model() {
    let (doc, _) = sample_document();
    let restored = import_migrated(export(&doc)).unwrap();
    assert_eq!(restored.id(), doc.id());
}

#[test]
fn unknown_node_kind_is_reported_not_defaulted() {
    let (doc, _) = sample_document();
    let mut dto = export(&doc);
    dto.nodes[0].kind = "hypercube".into();

    assert!(matches!(
        import(&dto),
        Err(ImportError::UnknownNodeKind { found }) if found == "hypercube"
    ));
}

#[test]
fn a_dangling_child_reference_is_reported() {
    let (doc, _) = sample_document();
    let mut dto = export(&doc);
    let frame = dto
        .nodes
        .iter_mut()
        .find(|n| n.kind == "frame")
        .expect("the sample has a frame");
    frame
        .children
        .push(swotvibe_core::NodeId::new().to_string());

    assert!(matches!(
        import(&dto),
        Err(ImportError::DanglingReference { .. })
    ));
}

#[test]
fn a_malformed_identity_is_reported() {
    let (doc, _) = sample_document();
    let mut dto = export(&doc);
    dto.nodes[0].id = "not-a-uuid".into();

    assert!(matches!(import(&dto), Err(ImportError::InvalidId { .. })));
}

#[test]
fn a_duplicate_node_identity_is_reported() {
    let (doc, _) = sample_document();
    let mut dto = export(&doc);
    let duplicated = dto.nodes[0].id.clone();
    dto.nodes.push(swotvibe_format::DtoNode {
        id: duplicated,
        kind: "shape".into(),
        name: None,
        children: Vec::new(),
        props: None,
        extensions: Default::default(),
    });
    assert!(matches!(import(&dto), Err(ImportError::DuplicateId { .. })));
}

#[test]
fn a_cycle_is_reported() {
    // inner -> outer -> inner: `outer`'s own child is its ancestor. Frames may
    // own children, so the structural check that fires is the cycle check.
    let mut doc = Document::new();
    let page = swotvibe_core::PageId::new();
    let outer = swotvibe_core::NodeId::new();
    let inner = swotvibe_core::NodeId::new();
    apply_batch_at_current(
        &mut doc,
        &[
            Command::CreatePage {
                id: page,
                name: "P".into(),
            },
            Command::CreateNode {
                id: outer,
                kind: NodeKind::Frame,
                name: Some("outer".into()),
                parent: NodePlacement::PageRoot {
                    page,
                    position: Position::Last,
                },
            },
            Command::CreateNode {
                id: inner,
                kind: NodeKind::Frame,
                name: Some("inner".into()),
                parent: NodePlacement::Child {
                    parent: outer,
                    position: Position::Last,
                },
            },
        ],
    )
    .unwrap();

    let mut dto = export(&doc);
    let outer_id = outer.to_string();
    let inner = dto
        .nodes
        .iter_mut()
        .find(|n| n.id == inner.to_string())
        .unwrap();
    inner.children.push(outer_id);

    let result = import(&dto);
    assert!(
        matches!(result, Err(ImportError::Cycle { .. })),
        "expected a cycle error, got {result:?}"
    );
}

#[test]
fn an_orphan_node_is_reported() {
    let (doc, _) = sample_document();
    let mut dto = export(&doc);
    dto.nodes.push(swotvibe_format::DtoNode {
        id: swotvibe_core::NodeId::new().to_string(),
        kind: "shape".into(),
        name: None,
        children: Vec::new(),
        props: None,
        extensions: Default::default(),
    });

    assert!(matches!(
        import(&dto),
        Err(ImportError::DanglingReference { .. })
    ));
}

#[test]
fn a_leaf_with_children_is_rejected() {
    let (doc, _) = sample_document();
    let mut dto = export(&doc);
    let (a_id, b_id) = {
        let shapes: Vec<String> = dto
            .nodes
            .iter()
            .filter(|n| n.kind == "shape")
            .map(|n| n.id.clone())
            .collect();
        (shapes[0].clone(), shapes[1].clone())
    };
    let a = dto.nodes.iter_mut().find(|n| n.id == a_id).unwrap();
    a.children.push(b_id);

    assert!(matches!(
        import(&dto),
        Err(ImportError::InvalidDocument { .. })
    ));
}

#[test]
fn unknown_top_level_fields_are_preserved() {
    let raw = json!({
        "schema_version": SCHEMA_VERSION,
        "id": swotvibe_core::DocumentId::new().to_string(),
        "pages": [],
        "assets": [],
        "nodes": [],
        "future_field": { "kept": true }
    });

    let dto = from_json(&serde_json::to_vec(&raw).unwrap()).unwrap();
    assert_eq!(
        dto.extensions.get("future_field"),
        Some(&json!({ "kept": true }))
    );

    // The preserved field survives a re-serialization.
    let bytes = to_json(&dto).unwrap();
    let reparsed: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        reparsed["extensions"]["future_field"],
        json!({ "kept": true })
    );
}

#[test]
fn extension_data_parsed_from_a_file_survives_export() {
    let mut dto = DtoDocument {
        schema_version: SCHEMA_VERSION,
        id: swotvibe_core::DocumentId::new().to_string(),
        pages: Vec::new(),
        assets: Vec::new(),
        nodes: Vec::new(),
        extensions: BTreeMap::new(),
    };
    dto.extensions
        .insert("vendor".into(), json!({ "note": "hello" }));

    let bytes = to_json(&dto).unwrap();
    let reparsed = from_json(&bytes).unwrap();
    assert_eq!(
        reparsed.extensions.get("vendor"),
        Some(&json!({ "note": "hello" }))
    );
}

#[test]
fn non_object_json_is_rejected() {
    let bytes = serde_json::to_vec(&json!([1, 2, 3])).unwrap();
    assert!(matches!(
        from_json(&bytes),
        Err(swotvibe_format::JsonError::NotAnObject)
    ));
}

#[test]
fn malformed_json_is_rejected() {
    assert!(matches!(
        from_json(b"{ not json"),
        Err(swotvibe_format::JsonError::Parse(_))
    ));
}

#[test]
fn schema_version_is_written_into_the_json() {
    let (doc, _) = sample_document();
    let bytes = to_json(&export(&doc)).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["schema_version"], json!(SCHEMA_VERSION));
}
