//! Regressions for the M0 code-review findings.
//!
//! Each test names the finding it pins down, so a future change that reopens
//! one of them fails here rather than in a bug report:
//!
//! 1. extension/unknown-field preservation across the whole pipeline,
//! 2. resource limits on the untrusted import path,
//! 3. past schema versions refused instead of read as current,
//! 4. bounded edit history,
//! 5. error-contract details (collision code, monotonic revisions).

use std::collections::BTreeMap;

use serde_json::json;
use swotvibe_core::{
    Command, CommandErrorCode, DEFAULT_HISTORY_CAPACITY, Document, DocumentEngine, NodeKind,
    NodePlacement, Position, Revision, apply_batch_at_current,
};
use swotvibe_format::{
    DtoDocument, ImportError, ImportLimits, JsonError, ReadLimits, SCHEMA_VERSION, export,
    from_json, from_json_with_limits, import, import_with_limits, migrate_to_current, to_json,
};

/// A page with one frame containing one shape, plus one asset.
fn sample_document() -> Document {
    let mut doc = Document::new();
    let page = swotvibe_core::PageId::new();
    let frame = swotvibe_core::NodeId::new();
    let shape = swotvibe_core::NodeId::new();
    let asset = swotvibe_core::AssetId::new();

    apply_batch_at_current(
        &mut doc,
        &[
            Command::CreatePage {
                id: page,
                name: "Page".into(),
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
                id: shape,
                kind: NodeKind::Shape,
                name: None,
                parent: NodePlacement::Child {
                    parent: frame,
                    position: Position::Last,
                },
            },
            Command::CreateAsset {
                id: asset,
                name: "asset".into(),
            },
        ],
    )
    .expect("sample document builds");

    doc
}

// ---------------------------------------------------------------------------
// Finding 1: extension and unknown-field preservation
// ---------------------------------------------------------------------------

#[test]
fn review_1_page_node_and_asset_extensions_survive_a_full_round_trip() {
    let mut doc = sample_document();
    let page = doc.pages()[0].id;
    let node_id = doc.pages()[0].roots()[0];
    let asset = doc.assets()[0].id;

    let page_ext = BTreeMap::from([("page.note".to_owned(), json!("page-level"))]);
    let node_ext = BTreeMap::from([("node.lockedBy".to_owned(), json!(["alice", "bob"]))]);
    let asset_ext = BTreeMap::from([("asset.license".to_owned(), json!({ "spdx": "MIT" }))]);
    doc.set_page_extensions(page, page_ext.clone());
    doc.set_node_extensions(node_id, node_ext.clone());
    doc.set_asset_extensions(asset, asset_ext.clone());

    // Export writes the maps into the DTO at the entity's own level.
    let dto = export(&doc);
    assert_eq!(dto.pages[0].extensions, page_ext);
    assert_eq!(
        dto.nodes
            .iter()
            .find(|n| n.id == node_id.to_string())
            .expect("root node is exported")
            .extensions,
        node_ext
    );
    assert_eq!(dto.assets[0].extensions, asset_ext);

    // ...and import restores them on the runtime entities, not the document.
    let reopened = import(&dto).expect("export output imports");
    assert_eq!(reopened.page_extensions(page), &page_ext);
    assert_eq!(reopened.node_extensions(node_id), &node_ext);
    assert_eq!(reopened.asset_extensions(asset), &asset_ext);
    assert!(
        reopened.extensions().is_empty(),
        "entity metadata must not leak into document extensions"
    );
}

#[test]
fn review_1_extensions_survive_a_json_byte_round_trip() {
    let mut doc = sample_document();
    let page = doc.pages()[0].id;
    let node_id = doc.pages()[0].roots()[0];
    doc.set_page_extensions(page, BTreeMap::from([("p".to_owned(), json!(1))]));
    doc.set_node_extensions(node_id, BTreeMap::from([("n".to_owned(), json!(true))]));

    let bytes = to_json(&export(&doc)).expect("serializes");
    let dto = from_json(&bytes).expect("parses");
    let reopened = import(&dto).expect("imports");

    assert_eq!(reopened.page_extensions(page).get("p"), Some(&json!(1)));
    assert_eq!(
        reopened.node_extensions(node_id).get("n"),
        Some(&json!(true))
    );
}

#[test]
fn review_1_unknown_page_level_keys_fold_into_extensions() {
    // A page written by a future build carries a key this schema does not
    // model. It must be captured, not dropped by Serde.
    let raw = json!({
        "schema_version": SCHEMA_VERSION,
        "id": swotvibe_core::DocumentId::new().to_string(),
        "pages": [{
            "id": swotvibe_core::PageId::new().to_string(),
            "name": "Page",
            "roots": [],
            "future_page_hint": { "grid": 8 }
        }],
        "assets": [],
        "nodes": []
    });

    let dto = from_json(&serde_json::to_vec(&raw).unwrap()).expect("parses");
    assert_eq!(
        dto.pages[0].extensions.get("future_page_hint"),
        Some(&json!({ "grid": 8 }))
    );

    // Re-serializing keeps the key where it was found.
    let reparsed: serde_json::Value = serde_json::from_slice(&to_json(&dto).unwrap()).unwrap();
    assert_eq!(
        reparsed["pages"][0]["future_page_hint"],
        json!({ "grid": 8 })
    );
}

#[test]
fn review_1_a_non_object_extensions_field_is_an_error_not_a_deletion() {
    // Silently deleting a malformed `extensions` is the data loss the schema
    // forbids; the reader reports it instead.
    let raw = json!({
        "schema_version": SCHEMA_VERSION,
        "id": swotvibe_core::DocumentId::new().to_string(),
        "pages": [],
        "assets": [],
        "nodes": [],
        "extensions": "not an object"
    });

    assert!(matches!(
        from_json(&serde_json::to_vec(&raw).unwrap()),
        Err(JsonError::WrongFieldType {
            field: "extensions",
            found: "a string"
        })
    ));
}

// ---------------------------------------------------------------------------
// Finding 2: resource limits on the untrusted path
// ---------------------------------------------------------------------------

#[test]
fn review_2_an_oversized_input_is_refused_before_parsing() {
    let bytes = vec![b' '; 64];
    assert!(matches!(
        from_json_with_limits(
            &bytes,
            ReadLimits {
                max_bytes: Some(8),
                ..ReadLimits::UNTRUSTED
            }
        ),
        Err(JsonError::TooLarge {
            limit: "input size in bytes",
            found: 64,
            allowed: 8
        })
    ));
}

#[test]
fn review_2_page_and_node_counts_are_bounded_at_read_time() {
    let raw = json!({
        "schema_version": SCHEMA_VERSION,
        "id": swotvibe_core::DocumentId::new().to_string(),
        "pages": [{
            "id": swotvibe_core::PageId::new().to_string(),
            "name": "Page",
            "roots": []
        }],
        "assets": [],
        "nodes": []
    });
    let bytes = serde_json::to_vec(&raw).unwrap();

    assert!(matches!(
        from_json_with_limits(
            &bytes,
            ReadLimits {
                max_pages: Some(0),
                ..ReadLimits::UNTRUSTED
            }
        ),
        Err(JsonError::TooLarge {
            limit: "page count",
            ..
        })
    ));
}

#[test]
fn review_2_an_over_deep_file_returns_a_resource_limit_not_a_stack_overflow() {
    // A chain deeper than the import bound. Built as DTOs directly so the test
    // itself does not depend on the parser.
    const DEPTH: usize = 64;
    let page = swotvibe_core::PageId::new();
    let ids: Vec<swotvibe_core::NodeId> =
        (0..DEPTH).map(|_| swotvibe_core::NodeId::new()).collect();

    // 0 is the page root; each node owns the next, so the chain is DEPTH long.
    let nodes: Vec<serde_json::Value> = ids
        .iter()
        .enumerate()
        .map(|(index, id)| {
            let children: Vec<String> = ids
                .get(index + 1)
                .map(|child| vec![child.to_string()])
                .unwrap_or_default();
            json!({
                "id": id.to_string(),
                "kind": "frame",
                "name": null,
                "children": children
            })
        })
        .collect();

    let raw = json!({
        "schema_version": SCHEMA_VERSION,
        "id": swotvibe_core::DocumentId::new().to_string(),
        "pages": [{
            "id": page.to_string(),
            "name": "Page",
            "roots": [ids[0].to_string()]
        }],
        "assets": [],
        "nodes": nodes
    });

    let dto: DtoDocument = serde_json::from_value(raw).unwrap();

    // The default depth bound accepts it.
    assert!(
        import(&dto).is_ok(),
        "a {DEPTH}-deep document is within the default bound"
    );

    // A tighter bound refuses it with a resource error rather than recursing.
    let result = import_with_limits(
        &dto,
        ImportLimits {
            max_depth: 8,
            ..ImportLimits::UNTRUSTED
        },
    );
    assert!(
        matches!(
            result,
            Err(ImportError::ResourceLimit {
                limit: _,
                found,
                allowed: 8
            }) if found == 9
        ),
        "expected a depth resource limit, got {result:?}"
    );
}

#[test]
fn review_2_import_with_limits_refuses_an_over_large_node_set() {
    let doc = sample_document();
    let dto = export(&doc);

    let result = import_with_limits(
        &dto,
        ImportLimits {
            max_nodes: Some(0),
            ..ImportLimits::UNTRUSTED
        },
    );
    assert!(
        matches!(
            result,
            Err(ImportError::ResourceLimit {
                limit: _,
                allowed: 0,
                ..
            })
        ),
        "expected a node-count resource limit, got {result:?}"
    );
}

// ---------------------------------------------------------------------------
// Finding 3: schema version floor
// ---------------------------------------------------------------------------

#[test]
fn review_3_a_past_schema_version_is_refused_by_import() {
    let mut dto = export(&sample_document());
    dto.schema_version = 0;

    let result = import(&dto);
    assert!(
        matches!(
            result,
            Err(ImportError::UnsupportedPastVersion {
                found: 0,
                oldest_supported: 1
            })
        ),
        "expected a past-version error, got {result:?}"
    );
}

#[test]
fn review_3_a_past_schema_version_is_reported_as_past_by_migration() {
    let mut dto = export(&sample_document());
    dto.schema_version = 0;

    // Not "newer than supported", which is what the old code said for v0.
    assert!(matches!(
        migrate_to_current(dto),
        Err(ImportError::UnsupportedPastVersion {
            found: 0,
            oldest_supported: 1
        })
    ));
}

// ---------------------------------------------------------------------------
// Finding 4: bounded history
// ---------------------------------------------------------------------------

#[test]
fn review_4_history_defaults_to_a_bounded_depth() {
    let mut engine = DocumentEngine::new();
    let page = swotvibe_core::PageId::new();

    let steps = DEFAULT_HISTORY_CAPACITY + 32;
    for step in 0..steps {
        let id = swotvibe_core::NodeId::new();
        engine
            .apply(&[Command::CreateNode {
                id,
                kind: NodeKind::Shape,
                name: Some(format!("n{step}")),
                parent: NodePlacement::PageRoot {
                    page,
                    position: Position::At(0),
                },
            }])
            .ok();
        if step == 0 {
            engine
                .apply(&[Command::CreatePage {
                    id: page,
                    name: "Page".into(),
                }])
                .expect("page is created first");
        }
    }

    assert_eq!(
        engine.history().capacity(),
        Some(DEFAULT_HISTORY_CAPACITY),
        "a fresh engine is bounded by the documented default"
    );
    assert_eq!(
        engine.history().undo_depth(),
        DEFAULT_HISTORY_CAPACITY,
        "the oldest steps are discarded once the bound is reached"
    );
}

#[test]
fn review_4_a_zero_capacity_history_records_nothing() {
    let mut engine = DocumentEngine::with_history_capacity(0);
    let page = swotvibe_core::PageId::new();
    engine
        .apply(&[Command::CreatePage {
            id: page,
            name: "Page".into(),
        }])
        .expect("applies");
    assert_eq!(engine.history().undo_depth(), 0);
    assert!(!engine.can_undo());
}

// ---------------------------------------------------------------------------
// Finding 5: error-contract details
// ---------------------------------------------------------------------------

#[test]
fn review_5_a_duplicate_page_identity_is_an_id_collision() {
    let mut doc = Document::new();
    let page = swotvibe_core::PageId::new();
    apply_batch_at_current(
        &mut doc,
        &[Command::CreatePage {
            id: page,
            name: "Page".into(),
        }],
    )
    .expect("first page applies");

    let error = apply_batch_at_current(
        &mut doc,
        &[Command::CreatePage {
            id: page,
            name: "Other".into(),
        }],
    )
    .expect_err("a duplicate page id is refused");
    assert_eq!(error.code(), Some(CommandErrorCode::IdCollision));
}

#[test]
fn review_5_a_duplicate_asset_identity_is_an_id_collision() {
    let mut doc = Document::new();
    let asset = swotvibe_core::AssetId::new();
    apply_batch_at_current(
        &mut doc,
        &[Command::CreateAsset {
            id: asset,
            name: "a".into(),
        }],
    )
    .expect("first asset applies");

    let error = apply_batch_at_current(
        &mut doc,
        &[Command::CreateAsset {
            id: asset,
            name: "b".into(),
        }],
    )
    .expect_err("a duplicate asset id is refused");
    assert_eq!(error.code(), Some(CommandErrorCode::IdCollision));
}

#[test]
fn review_5_revisions_saturate_instead_of_wrapping() {
    let top = Revision::from_raw(u64::MAX);
    assert_eq!(
        top.next(),
        top,
        "the counter never wraps to the initial value"
    );
    assert_ne!(top.next(), Revision::INITIAL);

    let penultimate = Revision::from_raw(u64::MAX - 1);
    assert_eq!(penultimate.next(), top);
}
