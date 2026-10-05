//! Tests for the identity types.

use std::collections::HashSet;
use std::str::FromStr;

use swotvibe_core::ids::{AssetId, DocumentId, IdError, IdKind, NodeId, PageId};

#[test]
fn new_ids_are_unique() {
    let ids: HashSet<NodeId> = (0..1000).map(|_| NodeId::new()).collect();
    assert_eq!(ids.len(), 1000);
}

#[test]
fn identity_types_are_distinct() {
    // A NodeId and a PageId built from the same UUID are different types; the
    // compiler prevents passing one where the other is expected. This test
    // documents that distinctness at the value level too.
    let uuid = NodeId::new().as_uuid();
    let node = NodeId::from_uuid(uuid);
    let page = PageId::from_uuid(uuid);
    assert_eq!(node.as_uuid(), page.as_uuid());
    assert_eq!(NodeId::KIND, IdKind::Node);
    assert_eq!(PageId::KIND, IdKind::Page);
    assert_eq!(DocumentId::KIND, IdKind::Document);
    assert_eq!(AssetId::KIND, IdKind::Asset);
}

#[test]
fn round_trip_through_text() {
    let id = DocumentId::new();
    let parsed = DocumentId::from_str(&id.to_string()).expect("valid id");
    assert_eq!(id, parsed);
}

#[test]
fn deterministic_is_stable_and_input_sensitive() {
    let a1 = NodeId::deterministic("import", "layer-1");
    let a2 = NodeId::deterministic("import", "layer-1");
    let b = NodeId::deterministic("import", "layer-2");
    let c = NodeId::deterministic("other", "layer-1");

    assert_eq!(a1, a2, "same inputs must produce the same id");
    assert_ne!(a1, b, "value changes the id");
    assert_ne!(a1, c, "namespace changes the id");
}

#[test]
fn deterministic_ids_are_distinct_across_kinds() {
    // Same namespace and value, different kind -> different id.
    let node = NodeId::deterministic("ns", "value");
    let page = PageId::deterministic("ns", "value");
    assert_ne!(node.as_uuid(), page.as_uuid());
}

#[test]
fn invalid_text_is_rejected_with_kind() {
    let err = NodeId::from_str("not-a-uuid").expect_err("must fail");
    match err {
        IdError::Invalid { kind, value, .. } => {
            assert_eq!(kind, IdKind::Node);
            assert_eq!(value, "not-a-uuid");
        }
    }
}

#[test]
fn parsed_ids_are_well_formed_uuids() {
    let id = PageId::new();
    let text = id.to_string();
    // 8-4-4-4-12 canonical form.
    assert_eq!(text.len(), 36);
    assert_eq!(text.matches('-').count(), 4);
    assert_eq!(text.as_bytes()[14], b'4', "v4 version nibble");
}
