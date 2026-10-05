//! The M0 vertical-slice acceptance test (§12.2).
//!
//! This is the gate, not a unit test: it drives one document through every
//! boundary the kernel owns, in the order the specification lists them, and
//! asserts the M0 success definition at the end:
//!
//! > no semantic loss on save/open, a reorder in one transaction survives
//! > undo/redo and a save/open round-trip, one undo restores the prior state,
//! > and a failed batch leaves no trace.
//!
//! Steps 3–4 of §12.2 (layout and render backends) are deliberately out of
//! scope here: `DEC-LAYOUT` and `DEC-RENDERER` are still open, so no backend
//! may be committed. This test covers the steps that do not depend on those
//! decisions and would fail if the kernel's core contract were broken.

use swotvibe_core::{
    Command, DocumentEngine, NodeId, NodeKind, NodePlacement, PageId, Position, Revision,
};
use swotvibe_format::{export, from_json, import, to_json};

/// Builds the §12.2 step-1 document: a page, a frame, and two simple shapes.
///
/// The frame and shapes are created in one batch so the whole skeleton is one
/// undoable step, matching what a host would do when opening a template.
fn build_document() -> (DocumentEngine, PageId, NodeId, NodeId, NodeId) {
    let mut engine = DocumentEngine::new();

    let page = PageId::new();
    let frame = NodeId::new();
    let first = NodeId::new();
    let second = NodeId::new();

    engine
        .apply(&[
            Command::CreatePage {
                id: page,
                name: "Page 1".into(),
            },
            Command::CreateNode {
                id: frame,
                kind: NodeKind::Frame,
                name: Some("Frame".into()),
                parent: NodePlacement::PageRoot {
                    page,
                    position: Position::Last,
                },
            },
            Command::CreateNode {
                id: first,
                kind: NodeKind::Shape,
                name: Some("A".into()),
                parent: NodePlacement::Child {
                    parent: frame,
                    position: Position::Last,
                },
            },
            Command::CreateNode {
                id: second,
                kind: NodeKind::Shape,
                name: Some("B".into()),
                parent: NodePlacement::Child {
                    parent: frame,
                    position: Position::Last,
                },
            },
        ])
        .expect("the step-1 document is well formed");

    (engine, page, frame, first, second)
}

/// Step 2 and step 5 (round-trip half): export to the DTO, JSON-encode it, then
/// decode and import it, and assert nothing was lost.
#[test]
fn m0_step_2_save_and_open_lose_no_semantics() {
    let (engine, page, frame, first, second) = build_document();

    // Verify the structure the kernel built before it is persisted (§12.2/2).
    engine
        .validate()
        .expect("the built document is internally valid");

    let bytes = to_json(&export(engine.document())).expect("the document serializes to JSON");
    let reopened_dto = from_json(&bytes).expect("the JSON parses back to a DTO");
    let reopened = import(&reopened_dto).expect("the DTO imports into the runtime model");

    // Same identity, same structure, same order, same revision.
    assert_eq!(reopened.id(), engine.document().id());
    assert_eq!(reopened.revision(), engine.revision());
    assert_eq!(reopened.pages().len(), 1);
    assert_eq!(reopened.page(page).map(|p| p.name.as_str()), Some("Page 1"));

    assert_eq!(reopened.node(frame).map(|n| n.name()), Some("Frame"));
    assert_eq!(reopened.node(frame).map(|n| n.kind), Some(NodeKind::Frame));
    assert_eq!(reopened.node(first).map(|n| n.kind), Some(NodeKind::Shape));
    assert_eq!(reopened.node(second).map(|n| n.kind), Some(NodeKind::Shape));

    assert_eq!(
        reopened.node(frame).map(|n| n.children()),
        Some([first, second].as_slice()),
        "child order must survive the file boundary exactly"
    );
    assert_eq!(
        reopened.page(page).map(|p| p.roots()),
        Some([frame].as_slice())
    );
    assert_eq!(reopened.parent_of(first), Some(frame));
    assert_eq!(reopened.page_of(second), Some(page));

    // The reopened document is valid on its own, and re-exporting it produces
    // the same bytes, which is the strongest available "no semantic loss"
    // statement for a JSON contract.
    assert_eq!(reopened.revision(), Revision::INITIAL.next());
    let second_bytes = to_json(&export(&reopened)).expect("re-export succeeds");
    assert_eq!(bytes, second_bytes, "save/open must be byte-stable");
}

/// Step 5 (mutation half): reorder one element in one transaction, then confirm
/// the result, undo/redo, and the post-reorder round-trip.
#[test]
fn m0_step_5_a_reorder_survives_undo_redo_and_save_open() {
    let (mut engine, _page, frame, first, second) = build_document();

    // Sanity: the declared order before the edit.
    assert_eq!(
        engine.document().node(frame).map(|n| n.children()),
        Some([first, second].as_slice())
    );

    // Move `second` before `first` in a single batch — one transaction.
    engine
        .apply(&[Command::ReorderNode {
            id: second,
            position: Position::Before(first),
        }])
        .expect("a reorder to an earlier position is accepted");
    assert_eq!(
        engine.document().node(frame).map(|n| n.children()),
        Some([second, first].as_slice()),
        "the reorder must be visible immediately"
    );

    // One undo restores the prior state exactly (M0 success definition).
    engine.undo().expect("one undo restores the prior order");
    assert_eq!(
        engine.document().node(frame).map(|n| n.children()),
        Some([first, second].as_slice()),
        "one undo must restore the exact prior order"
    );

    // Redo reapplies it.
    engine.redo().expect("redo reapplies the reorder");
    assert_eq!(
        engine.document().node(frame).map(|n| n.children()),
        Some([second, first].as_slice())
    );

    // The reordered document now round-trips through the file boundary.
    let bytes = to_json(&export(engine.document())).expect("reordered document serializes");
    let reopened = import(&from_json(&bytes).expect("JSON parses")).expect("DTO imports");
    assert_eq!(
        reopened.node(frame).map(|n| n.children()),
        Some([second, first].as_slice()),
        "the reorder must survive save/open"
    );
}

/// Step 6: a batch whose last command is invalid must leave the document, the
/// revision, and the history untouched — the "failed batch leaves no trace"
/// clause.
#[test]
fn m0_step_6_a_failed_batch_leaves_no_trace() {
    let (mut engine, page, frame, first, second) = build_document();

    let revision_before = engine.revision();
    let undo_depth_before = engine.undo_depth();
    let children_before = engine
        .document()
        .node(frame)
        .map(|n| n.children().to_vec())
        .expect("the frame exists");

    // First command is valid and would reorder the frame's children. The last
    // command targets a node that does not exist, so the whole batch must be
    // rejected and the earlier command must not be visible.
    let missing = NodeId::new();
    let error = engine
        .apply(&[
            Command::ReorderNode {
                id: second,
                position: Position::Before(first),
            },
            Command::RenameNode {
                id: missing,
                name: Some("ghost".into()),
            },
        ])
        .expect_err("a batch referring to a missing node is rejected");

    // The error is a typed command rejection, not a panic or a partial write.
    assert!(
        matches!(error, swotvibe_core::BatchError::Command(_)),
        "expected a command rejection, got {error:?}"
    );

    // Nothing moved: revision, history, and structure are all as before.
    assert_eq!(
        engine.revision(),
        revision_before,
        "revision must be unchanged"
    );
    assert_eq!(
        engine.undo_depth(),
        undo_depth_before,
        "history must be unchanged"
    );
    assert_eq!(
        engine.document().node(frame).map(|n| n.children().to_vec()),
        Some(children_before),
        "the first command's reorder must not have been applied"
    );
    assert!(engine.document().node(missing).is_none());
    assert_eq!(engine.document().page_of(first), Some(page));
}

/// The whole gate in one pass: build → validate → save/open → reorder →
/// undo/redo → save/open again → failed batch. This is the single test a
/// reviewer can read to see the M0 slice end to end.
#[test]
fn m0_vertical_slice_end_to_end() {
    let (mut engine, page, frame, first, second) = build_document();
    engine.validate().expect("step 2: the document is valid");

    // Step 2: persist and reopen without loss.
    let opened =
        import(&from_json(&to_json(&export(engine.document())).unwrap()).unwrap()).unwrap();
    assert_eq!(
        opened.node(frame).map(|n| n.children()),
        Some([first, second].as_slice())
    );

    // Step 5: a single-transaction reorder, its undo, and its redo.
    engine
        .apply(&[Command::ReorderNode {
            id: second,
            position: Position::First,
        }])
        .expect("reorder succeeds");
    let reordered = engine
        .document()
        .node(frame)
        .map(|n| n.children().to_vec())
        .unwrap();
    assert_eq!(reordered, [second, first]);

    engine.undo().unwrap();
    assert_eq!(
        engine.document().node(frame).map(|n| n.children().to_vec()),
        Some(vec![first, second])
    );
    engine.redo().unwrap();

    // Step 5: the reordered result survives a save/open cycle.
    let final_bytes = to_json(&export(engine.document())).unwrap();
    let final_doc = import(&from_json(&final_bytes).unwrap()).unwrap();
    assert_eq!(
        final_doc.node(frame).map(|n| n.children().to_vec()),
        Some(vec![second, first])
    );
    assert_eq!(final_doc.page_of(second), Some(page));

    // Step 6: a trailing invalid command wipes the whole batch.
    let revision_before = engine.revision();
    let depth_before = engine.undo_depth();
    let _ = engine
        .apply(&[
            Command::RenameNode {
                id: first,
                name: Some("changed".into()),
            },
            Command::MoveNode {
                id: second,
                parent: NodePlacement::PageRoot {
                    page,
                    position: Position::After(NodeId::new()),
                },
            },
        ])
        .expect_err("the missing sibling reference rejects the batch");
    assert_eq!(engine.revision(), revision_before);
    assert_eq!(engine.undo_depth(), depth_before);
    assert_eq!(engine.document().node(first).map(|n| n.name()), Some("A"));
    assert_eq!(
        engine.document().node(frame).map(|n| n.children().to_vec()),
        Some(vec![second, first])
    );
}
