//! M1 acceptance tests for the application service.
//!
//! These exercise the loop the interface will actually drive, but headlessly and
//! without a host: open the M0 sample, edit one property, undo, redo, save,
//! reopen, and confirm the failure paths leave the session untouched.
//!
//! The fixtures and fonts are the pinned ones the rest of the workspace uses, so
//! a result here means the same thing it means in `swotvibe-tools`: the numbers
//! come from `assets/fonts`, never from a font installed on the machine.
//!
//! ## Revision is read, never assumed
//!
//! Revision is a session counter and is not persisted, so a document that was
//! just imported is at whatever revision the import produced. Every test here
//! reads the current revision and sends it back, which is also what a host must
//! do. A hard-coded `0` would be a test that only passes by accident.

use std::path::PathBuf;
use std::sync::Arc;

use swotvibe_app::{
    AppErrorCode, EditCommand, EditRequest, EditorSession, NodeParent, PreviewOptions, Rgba,
    SharedTextEngine,
};
use swotvibe_core::PageId;
use swotvibe_text::{FontSet, ParleyTextEngine};

/// The node identities in `tests/fixtures/m0-sample-v2.json`.
mod sample {
    /// The frame that contains the whole sample.
    pub const CARD: &str = "018f0000-0000-7000-8000-000000020001";
    /// The blue header shape.
    pub const HEADER: &str = "018f0000-0000-7000-8000-000000020002";
    /// The Latin text node.
    pub const LATIN: &str = "018f0000-0000-7000-8000-000000020003";
    /// The Arabic text node.
    pub const ARABIC: &str = "018f0000-0000-7000-8000-000000020004";
    /// The red ellipse.
    pub const DOT: &str = "018f0000-0000-7000-8000-000000020005";
    /// A node identity that parses but is not in the document.
    pub const ABSENT: &str = "018f0000-0000-7000-8000-0000000fffff";
}

/// The sample's artboard, matching `tests/golden/m0-sample.layout.json`.
const PAGE_SIZE: [f64; 2] = [480.0, 320.0];

fn repository_root() -> PathBuf {
    let mut directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    loop {
        if directory.join("assets").join("fonts").is_dir() {
            return directory;
        }
        assert!(directory.pop(), "cannot find the repository root");
    }
}

fn read(path: PathBuf) -> Vec<u8> {
    std::fs::read(&path).unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()))
}

fn sample_bytes() -> Vec<u8> {
    read(repository_root().join("tests/fixtures/m0-sample-v2.json"))
}

/// The pinned fonts, loaded from the repository rather than the system.
fn pinned_fonts() -> FontSet {
    let mut fonts = FontSet::new();
    fonts
        .register(
            "Inter",
            read(repository_root().join("assets/fonts/inter/Inter-variable.ttf")),
        )
        .expect("the Latin face should register");
    fonts
        .register(
            "Noto Sans Arabic",
            read(
                repository_root().join("assets/fonts/noto-sans-arabic/NotoSansArabic-variable.ttf"),
            ),
        )
        .expect("the Arabic face should register");
    fonts
}

fn text_engine() -> SharedTextEngine {
    Arc::new(ParleyTextEngine::new(pinned_fonts()))
}

fn open_sample() -> EditorSession {
    let mut session = EditorSession::new(text_engine());
    session
        .open_json(&sample_bytes(), Some("m0-sample-v2.json".to_owned()))
        .expect("the sample should open");
    session
}

fn preview_options() -> PreviewOptions {
    PreviewOptions {
        page_size: Some(PAGE_SIZE),
        scale: 1.0,
        background: [255, 255, 255, 255],
    }
}

/// A request at the session's own revision, which is what a host sends.
fn request_now(session: &EditorSession, commands: Vec<EditCommand>) -> EditRequest {
    EditRequest {
        expected_revision: session.revision().as_u64(),
        commands,
    }
}

fn set_fill(node: &str, fill: Option<Rgba>) -> EditCommand {
    EditCommand::SetNodeFill {
        node: node.to_owned(),
        fill,
    }
}

fn rename(node: &str, name: Option<&str>) -> EditCommand {
    EditCommand::RenameNode {
        node: node.to_owned(),
        name: name.map(ToOwned::to_owned),
    }
}

fn create(node: &str, kind: &str, name: Option<&str>, parent: NodeParent) -> EditCommand {
    EditCommand::CreateNode {
        node: node.to_owned(),
        node_kind: kind.to_owned(),
        name: name.map(ToOwned::to_owned),
        fill: None,
        parent,
    }
}

/// A create that also sets the node's fill, in one command.
fn create_filled(node: &str, kind: &str, fill: Rgba, parent: NodeParent) -> EditCommand {
    EditCommand::CreateNode {
        node: node.to_owned(),
        node_kind: kind.to_owned(),
        name: None,
        fill: Some(fill),
        parent,
    }
}

fn delete(node: &str) -> EditCommand {
    EditCommand::DeleteNode {
        node: node.to_owned(),
    }
}

fn move_node(node: &str, parent: NodeParent) -> EditCommand {
    EditCommand::MoveNode {
        node: node.to_owned(),
        parent,
    }
}

/// A fresh identity that is not in the sample, for a create command.
fn fresh_id(suffix: u32) -> String {
    format!("018f0000-0000-7000-8000-0000000{suffix:05x}")
}

fn first_page(session: &EditorSession) -> PageId {
    session.first_page().expect("the sample has a page")
}

fn node_name(session: &mut EditorSession, id: &str) -> String {
    session
        .view()
        .expect("the view is available")
        .nodes
        .iter()
        .find(|node| node.id == id)
        .map(|node| node.name.clone())
        .unwrap_or_else(|| panic!("{id} is in the document"))
}

#[test]
fn a_new_session_is_empty_and_not_dirty() {
    // A read needs no mutable session: `is_dirty` and `view` take `&self`.
    let session = EditorSession::new(text_engine());

    assert!(
        !session.is_dirty(),
        "an empty document matches what it would write"
    );
    assert!(!session.can_undo());
    assert!(!session.can_redo());
    assert_eq!(session.revision().as_u64(), 0);
    assert_eq!(session.display_name(), None);
    assert!(session.first_page().is_none());

    let view = session.view().expect("a view is always available");
    assert!(view.pages.is_empty());
    assert!(view.nodes.is_empty());
    assert!(!view.dirty);
}

#[test]
fn opening_the_sample_produces_the_documented_view() {
    let session = open_sample();
    let revision = session.revision().as_u64();

    assert_eq!(session.display_name(), Some("m0-sample-v2.json"));
    assert!(
        !session.is_dirty(),
        "a freshly opened document is not modified"
    );
    assert!(!session.can_undo(), "a load does not create an undo step");

    let view = session.view().expect("the sample has a view");
    assert_eq!(view.revision, revision);
    assert!(!view.dirty);
    assert_eq!(view.display_name.as_deref(), Some("m0-sample-v2.json"));
    assert_eq!(view.pages.len(), 1);
    assert_eq!(view.pages[0].name, "Page 1");
    assert_eq!(view.pages[0].roots, vec![sample::CARD.to_owned()]);

    // Pre-order is paint order, so the container comes before its children.
    let ids: Vec<&str> = view.nodes.iter().map(|node| node.id.as_str()).collect();
    assert_eq!(
        ids,
        vec![
            sample::CARD,
            sample::HEADER,
            sample::LATIN,
            sample::ARABIC,
            sample::DOT,
        ]
    );

    let card = &view.nodes[0];
    assert_eq!(card.kind, "frame");
    assert_eq!(card.name, "Card");
    assert_eq!(card.parent, None);
    assert_eq!(card.children.len(), 4);

    let header = view
        .nodes
        .iter()
        .find(|node| node.id == sample::HEADER)
        .expect("the header is present");
    assert_eq!(header.kind, "shape");
    assert_eq!(header.parent.as_deref(), Some(sample::CARD));
    assert_eq!(header.props.fill, Some(Rgba::opaque(76, 110, 245)));
    assert_eq!(header.props.shape_geometry.as_deref(), Some("rect"));
    assert_eq!(header.props.corner_radius, Some(8.0));

    // Nothing in the view carries session state such as a selection or a path.
    let json = serde_json::to_value(&view).expect("the view serializes");
    assert!(json.get("selection").is_none());
    assert!(json.get("path").is_none());
}

#[test]
fn the_sample_reads_back_its_kind_specific_properties() {
    let session = open_sample();

    let header = session
        .node_props(sample::HEADER)
        .expect("the header exists");
    assert_eq!(header.fill, Some(Rgba::opaque(76, 110, 245)));
    assert_eq!(header.shape_geometry.as_deref(), Some("rect"));
    assert_eq!(header.corner_radius, Some(8.0));
    assert_eq!(header.text_content, None);
    assert_eq!(header.frame_layout, None);

    let dot = session.node_props(sample::DOT).expect("the dot exists");
    assert_eq!(dot.shape_geometry.as_deref(), Some("ellipse"));

    let latin = session.node_props(sample::LATIN).expect("the text exists");
    assert_eq!(latin.font_family.as_deref(), Some("Inter"));
    assert_eq!(latin.shape_geometry, None);
    assert!(latin.text_content.is_some());

    let arabic = session.node_props(sample::ARABIC).expect("the text exists");
    assert_eq!(arabic.font_family.as_deref(), Some("Noto Sans Arabic"));

    let card = session.node_props(sample::CARD).expect("the frame exists");
    assert!(card.frame_layout.is_some());
}

#[test]
fn an_edit_advances_the_revision_and_marks_the_document_dirty() {
    let mut session = open_sample();
    let before = session.revision().as_u64();
    let new_fill = Rgba::opaque(10, 20, 30);

    let summary = session
        .apply(&request_now(
            &session,
            vec![set_fill(sample::HEADER, Some(new_fill))],
        ))
        .expect("the edit applies");

    assert_eq!(summary.previous_revision, before);
    assert_eq!(summary.revision, before + 1);
    assert!(summary.dirty);
    assert!(summary.can_undo);
    assert!(!summary.can_redo);
    assert_eq!(summary.changed_nodes, vec![sample::HEADER.to_owned()]);
    assert_eq!(
        summary.affected_pages,
        vec![session.first_page().unwrap().to_string()]
    );

    assert!(session.is_dirty());
    assert_eq!(
        session.node_props(sample::HEADER).unwrap().fill,
        Some(new_fill),
        "the stored property is the one that was requested"
    );
}

#[test]
fn a_stale_revision_is_refused_and_changes_nothing() {
    let mut session = open_sample();
    let before = session.revision().as_u64();

    session
        .apply(&EditRequest {
            expected_revision: before,
            commands: vec![set_fill(sample::HEADER, Some(Rgba::opaque(1, 2, 3)))],
        })
        .expect("the first edit applies");

    let error = session
        .apply(&EditRequest {
            expected_revision: before,
            commands: vec![set_fill(sample::HEADER, Some(Rgba::opaque(9, 9, 9)))],
        })
        .expect_err("the stale revision is refused");

    assert_eq!(error.code, AppErrorCode::RevisionConflict);
    assert_eq!(error.revision, Some(before + 1));
    assert_eq!(
        session.revision().as_u64(),
        before + 1,
        "no revision was consumed by the refusal"
    );
    assert_eq!(
        session.node_props(sample::HEADER).unwrap().fill,
        Some(Rgba::opaque(1, 2, 3)),
        "the first edit is untouched"
    );
}

#[test]
fn undo_returns_to_the_saved_state_and_redo_leaves_it_again() {
    let mut session = open_sample();
    let original = session.node_props(sample::HEADER).unwrap().fill;

    session
        .apply(&request_now(
            &session,
            vec![set_fill(sample::HEADER, Some(Rgba::opaque(7, 7, 7)))],
        ))
        .expect("the edit applies");
    assert!(session.is_dirty());

    let undone = session
        .undo(session.revision().as_u64())
        .expect("there is a step to undo");
    assert!(
        !undone.dirty,
        "undo restored the saved bytes, so the document is clean"
    );
    assert!(undone.can_redo);
    assert_eq!(session.node_props(sample::HEADER).unwrap().fill, original);
    assert!(!session.is_dirty());

    let redone = session
        .redo(session.revision().as_u64())
        .expect("there is a step to redo");
    assert!(redone.dirty, "redo moved away from the saved state again");
    assert_eq!(
        session.node_props(sample::HEADER).unwrap().fill,
        Some(Rgba::opaque(7, 7, 7))
    );
}

#[test]
fn an_empty_timeline_reports_a_history_error_rather_than_doing_nothing() {
    let mut session = open_sample();
    let revision = session.revision().as_u64();

    let undo = session.undo(revision).expect_err("nothing has been edited");
    assert_eq!(undo.code, AppErrorCode::History);

    let redo = session.redo(revision).expect_err("nothing has been undone");
    assert_eq!(redo.code, AppErrorCode::History);

    assert_eq!(session.revision().as_u64(), revision);
}

#[test]
fn saving_and_reopening_preserves_the_edit_the_tree_and_the_identities() {
    let mut session = open_sample();
    let new_fill = Rgba::opaque(200, 30, 90);
    session
        .apply(&request_now(
            &session,
            vec![set_fill(sample::HEADER, Some(new_fill))],
        ))
        .expect("the edit applies");

    let bytes = session.export_bytes().expect("the document exports");
    assert!(session.is_dirty(), "exporting is not saving");

    session.note_saved().expect("the save is recorded");
    assert!(
        !session.is_dirty(),
        "a successful write clears the modified state"
    );

    // Reopen the written bytes in a fresh session, as a host would after a
    // restart, and confirm the edit and the structure survived.
    let mut reopened = EditorSession::new(text_engine());
    let view = reopened
        .open_json(&bytes, Some("m0-sample-v2.json".to_owned()))
        .expect("the written bytes reopen");

    assert!(!view.dirty);
    assert_eq!(
        view.revision,
        reopened.revision().as_u64(),
        "the view names the revision the session is actually at"
    );
    assert_eq!(view.pages.len(), 1);
    assert_eq!(view.pages[0].roots, vec![sample::CARD.to_owned()]);
    assert_eq!(
        view.nodes
            .iter()
            .map(|node| node.id.as_str())
            .collect::<Vec<_>>(),
        vec![
            sample::CARD,
            sample::HEADER,
            sample::LATIN,
            sample::ARABIC,
            sample::DOT,
        ],
        "hierarchy and order survive the round trip"
    );
    assert_eq!(
        reopened.node_props(sample::HEADER).unwrap().fill,
        Some(new_fill)
    );
    assert!(!reopened.can_undo(), "session history is not persisted");

    // The written bytes are canonical: writing them again reproduces them.
    let again = reopened
        .export_bytes()
        .expect("the reopened document exports");
    assert_eq!(again, bytes, "save is byte-stable for the same content");
}

#[test]
fn renaming_a_node_round_trips_and_an_empty_name_clears_it() {
    let mut session = open_sample();

    session
        .apply(&request_now(
            &session,
            vec![rename(sample::LATIN, Some("Caption"))],
        ))
        .expect("the rename applies");
    assert_eq!(node_name(&mut session, sample::LATIN), "Caption");

    session
        .apply(&request_now(&session, vec![rename(sample::LATIN, None)]))
        .expect("clearing the name applies");
    assert_eq!(
        node_name(&mut session, sample::LATIN),
        "",
        "an unnamed node reads as empty, not as its identity"
    );
}

#[test]
fn a_batch_whose_last_command_is_invalid_leaves_no_trace() {
    let mut session = open_sample();
    let revision = session.revision().as_u64();
    let original_name = node_name(&mut session, sample::HEADER);

    let error = session
        .apply(&EditRequest {
            expected_revision: revision,
            commands: vec![
                rename(sample::HEADER, Some("Renamed")),
                set_fill(sample::ABSENT, Some(Rgba::opaque(1, 1, 1))),
            ],
        })
        .expect_err("the batch names a node that does not exist");

    assert_eq!(error.code, AppErrorCode::UnknownNode);
    assert_eq!(error.node.as_deref(), Some(sample::ABSENT));
    assert_eq!(
        session.revision().as_u64(),
        revision,
        "no revision was consumed"
    );
    assert!(!session.is_dirty());
    assert!(!session.can_undo());
    assert_eq!(
        node_name(&mut session, sample::HEADER),
        original_name,
        "the earlier command was not applied"
    );
}

#[test]
fn an_unparsable_identity_is_an_invalid_request() {
    let mut session = open_sample();
    let revision = session.revision().as_u64();

    let error = session
        .apply(&request_now(
            &session,
            vec![set_fill("not-an-identity", Some(Rgba::opaque(1, 1, 1)))],
        ))
        .expect_err("the identity does not parse");

    assert_eq!(error.code, AppErrorCode::InvalidRequest);
    assert_eq!(session.revision().as_u64(), revision);
}

#[test]
fn a_malformed_document_does_not_replace_the_open_one() {
    let mut session = open_sample();
    session
        .apply(&request_now(
            &session,
            vec![set_fill(sample::HEADER, Some(Rgba::opaque(5, 5, 5)))],
        ))
        .expect("the edit applies");
    let revision = session.revision().as_u64();

    let error = session
        .open_json(b"{ not json", Some("broken.json".to_owned()))
        .expect_err("the payload does not parse");

    assert_eq!(error.code, AppErrorCode::Schema);
    assert_eq!(
        session.revision().as_u64(),
        revision,
        "the open document is unchanged"
    );
    assert_eq!(session.display_name(), Some("m0-sample-v2.json"));
    assert!(session.is_dirty(), "the pending edit is still pending");
    assert_eq!(
        session.node_props(sample::HEADER).unwrap().fill,
        Some(Rgba::opaque(5, 5, 5))
    );
}

#[test]
fn a_future_schema_version_is_refused_rather_than_downgraded() {
    let mut session = EditorSession::new(text_engine());
    // The persisted schema uses snake_case field names, unlike the camelCase
    // interface contract. A file is not a message.
    let future = br#"{"schema_version": 99, "id": "018f0000-0000-7000-8000-000000000021", "pages": [], "assets": [], "nodes": []}"#;

    let error = session
        .open_json(future, None)
        .expect_err("a newer schema is not readable");

    assert_eq!(error.code, AppErrorCode::Schema);
    assert!(
        error.message.contains("99"),
        "the message names the version: {}",
        error.message
    );
    assert!(
        session.document().pages().is_empty(),
        "the refused payload did not become the open document"
    );
}

#[test]
fn a_preview_renders_the_sample_at_its_revision() {
    let session = open_sample();
    let page = first_page(&session);
    let revision = session.revision().as_u64();

    let preview = session
        .preview(page, preview_options())
        .expect("the sample renders");

    assert_eq!(
        preview.revision, revision,
        "the preview names the revision it came from"
    );
    assert_eq!(preview.page, page.to_string());
    assert_eq!(preview.page_size, PAGE_SIZE);
    assert_eq!((preview.width, preview.height), (480, 320));
    assert_eq!(preview.scale, 1.0);
    assert!(!preview.png.is_empty());
    assert_eq!(
        &preview.png[..8],
        &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a],
        "the payload is a PNG"
    );
    assert!(preview.layout_diagnostics.is_empty());
    assert!(preview.render_diagnostics.is_empty());
    assert_eq!(session.renderer_id(), swotvibe_render::VELLO_ENGINE_ID);

    // The same revision renders to the same bytes, which is what makes a
    // preview comparable rather than merely plausible.
    let again = session.preview(page, preview_options()).unwrap();
    assert_eq!(again.png, preview.png);
}

#[test]
fn an_edit_is_visible_in_the_next_preview() {
    let mut session = open_sample();
    let page = first_page(&session);
    let before = session.preview(page, preview_options()).unwrap();

    session
        .apply(&request_now(
            &session,
            vec![set_fill(sample::DOT, Some(Rgba::opaque(0, 0, 0)))],
        ))
        .expect("the edit applies");

    let after = session.preview(page, preview_options()).unwrap();
    assert_eq!(after.revision, session.revision().as_u64());
    assert_ne!(after.png, before.png, "the rendered pixels changed");
}

#[test]
fn a_created_text_node_names_a_registered_family() {
    let mut session = open_sample();
    let new_id = fresh_id(0xB1);

    session
        .apply(&request_now(
            &session,
            vec![create(
                &new_id,
                "text",
                Some("Caption"),
                NodeParent::PageRoot {
                    page: first_page(&session).to_string(),
                },
            )],
        ))
        .expect("the text node is created");

    let props = session.node_props(&new_id).expect("the node exists");
    let family = props.font_family.expect("a text node names a family");

    // The default content names `sans-serif`, which no host registers. A text
    // node the session created must be measurable, or every later layout pass
    // fails; the family therefore comes from the engine, not from a constant.
    assert_ne!(family, "sans-serif");
    assert!(
        session.font_families().contains(&family),
        "`{family}` should be a registered family, found {:?}",
        session.font_families()
    );
}

#[test]
fn a_created_text_node_lays_out_without_diagnostics() {
    let mut session = open_sample();
    let new_id = fresh_id(0xB2);
    let page = first_page(&session);

    session
        .apply(&request_now(
            &session,
            vec![create(
                &new_id,
                "text",
                Some("Measurable"),
                NodeParent::PageRoot {
                    page: page.to_string(),
                },
            )],
        ))
        .expect("the text node is created");

    // Layout measures every text node, so a family that cannot be measured turns
    // into a failure here. This is the assertion that the create is usable rather
    // than merely stored.
    let layout = session
        .layout(page, preview_options())
        .expect("the page lays out");
    assert!(
        layout.nodes.iter().any(|node| node.id == new_id),
        "the created node has geometry"
    );
    assert!(
        layout.diagnostics.is_empty(),
        "layout reported: {:?}",
        layout.diagnostics
    );
}

#[test]
fn capabilities_report_what_the_build_can_do() {
    let session = open_sample();
    let capabilities = session.capabilities();

    assert_eq!(
        capabilities.creatable_kinds,
        vec!["frame", "group", "shape", "text", "image"]
    );
    assert!(
        capabilities.font_families.contains(&"Inter".to_owned()),
        "the pinned Latin face is reported: {:?}",
        capabilities.font_families
    );
    assert!(
        capabilities
            .font_families
            .contains(&"Noto Sans Arabic".to_owned()),
        "the pinned Arabic face is reported: {:?}",
        capabilities.font_families
    );
    // The engines are named because they are temporary: a caller that records
    // what it saw should record what produced it.
    assert!(capabilities.renderer.contains("vello_cpu"));
    assert!(capabilities.layout_engine.contains("taffy"));

    // The list a caller reads is the same one the create path uses.
    assert_eq!(session.font_families(), capabilities.font_families);
}

#[test]
fn capabilities_cross_the_wire_in_the_documented_shape() {
    let session = open_sample();
    let json = serde_json::to_value(session.capabilities()).expect("capabilities serialize");

    assert!(json["creatableKinds"].is_array());
    assert!(json["fontFamilies"].is_array());
    assert!(json["renderer"].is_string());
    assert!(json["layoutEngine"].is_string());
}

#[test]
fn a_preview_carries_its_pixels_through_serialization() {
    let session = open_sample();
    let page = first_page(&session);
    let preview = session.preview(page, preview_options()).unwrap();

    // A preview is returned by a host command, so it is serialized. A field that
    // is skipped would make the command look successful while returning no
    // image, which is why this asserts on the round trip and not on the struct.
    let json = serde_json::to_value(&preview).expect("the preview serializes");
    let bytes = json["png"]
        .as_array()
        .expect("the PNG is present in the serialized form");
    assert_eq!(
        bytes.len(),
        preview.png.len(),
        "every pixel byte crosses the wire"
    );
    assert_eq!(bytes[0].as_u64(), Some(0x89), "the PNG signature survives");
    assert_eq!(json["revision"], serde_json::json!(preview.revision));
    assert_eq!(json["width"], serde_json::json!(preview.width));

    let parsed: swotvibe_app::Preview =
        serde_json::from_value(json).expect("the preview parses back");
    assert_eq!(parsed, preview, "the round trip is lossless");
}

#[test]
fn a_preview_of_an_unknown_page_is_a_typed_error() {
    let session = open_sample();

    let error = session
        .preview(PageId::new(), preview_options())
        .expect_err("the page is not in the document");

    assert_eq!(error.code, AppErrorCode::UnknownPage);
}

#[test]
fn an_unusable_preview_configuration_is_a_typed_error() {
    let session = open_sample();
    let page = first_page(&session);

    let zero_scale = PreviewOptions {
        scale: 0.0,
        ..preview_options()
    };
    assert_eq!(
        session.preview(page, zero_scale).unwrap_err().code,
        AppErrorCode::InvalidRequest
    );

    let not_finite = PreviewOptions {
        scale: f64::NAN,
        ..preview_options()
    };
    assert_eq!(
        session.preview(page, not_finite).unwrap_err().code,
        AppErrorCode::InvalidRequest
    );

    let oversized = PreviewOptions {
        page_size: Some([100_000.0, 100.0]),
        ..preview_options()
    };
    assert_eq!(
        session.preview(page, oversized).unwrap_err().code,
        AppErrorCode::ResourceLimit
    );
}

#[test]
fn a_hit_test_finds_the_node_on_top_and_nothing_outside_the_page() {
    let session = open_sample();
    let page = first_page(&session);

    // Inside the header shape, which paints above the frame that contains it.
    let hit = session
        .hit_test(page, (60.0, 60.0), preview_options())
        .expect("the hit test runs");
    assert_eq!(hit.node.as_deref(), Some(sample::HEADER));
    assert_eq!(hit.revision, session.revision().as_u64());

    // Inside the ellipse, which is the last node painted.
    let hit = session
        .hit_test(page, (70.0, 225.0), preview_options())
        .expect("the hit test runs");
    assert_eq!(hit.node.as_deref(), Some(sample::DOT));

    // Inside the frame, in the gap between its children: the frame itself.
    let hit = session
        .hit_test(page, (430.0, 270.0), preview_options())
        .expect("the hit test runs");
    assert_eq!(hit.node.as_deref(), Some(sample::CARD));

    // Outside every node.
    let hit = session
        .hit_test(page, (5.0, 5.0), preview_options())
        .expect("the hit test runs");
    assert_eq!(hit.node, None);
}

#[test]
fn a_hit_test_refuses_a_point_that_is_not_a_number() {
    let session = open_sample();
    let page = first_page(&session);

    let error = session
        .hit_test(page, (f64::NAN, 0.0), preview_options())
        .expect_err("NaN is not a point");

    assert_eq!(error.code, AppErrorCode::InvalidRequest);
    assert_eq!(
        session
            .hit_test(PageId::new(), (0.0, 0.0), preview_options())
            .unwrap_err()
            .code,
        AppErrorCode::UnknownPage
    );
}

#[test]
fn the_layout_view_reports_the_geometry_a_selection_overlay_draws() {
    let session = open_sample();
    let page = first_page(&session);

    let layout = session
        .layout(page, preview_options())
        .expect("the page lays out");

    assert_eq!(layout.revision, session.revision().as_u64());
    assert_eq!(layout.page, page.to_string());
    assert_eq!(layout.page_size, PAGE_SIZE);
    assert!(layout.diagnostics.is_empty());
    assert_eq!(layout.nodes.len(), 5, "every node reports its geometry");

    // The sample's Card frame, from the committed layout reference.
    let card = layout
        .nodes
        .iter()
        .find(|node| node.id == sample::CARD)
        .expect("the frame is laid out");
    assert_eq!(card.rect, [40.0, 40.0, 400.0, 240.0]);
    assert_eq!(card.size, [400.0, 240.0]);

    // Paint order: the container precedes its children.
    assert_eq!(layout.nodes[0].id, sample::CARD);
    assert_eq!(
        layout
            .nodes
            .iter()
            .map(|node| node.id.as_str())
            .collect::<Vec<_>>(),
        vec![
            sample::CARD,
            sample::HEADER,
            sample::LATIN,
            sample::ARABIC,
            sample::DOT,
        ]
    );

    assert_eq!(
        session
            .layout(PageId::new(), preview_options())
            .unwrap_err()
            .code,
        AppErrorCode::UnknownPage
    );
}

#[test]
fn a_created_node_lands_at_the_end_of_its_container() {
    let mut session = open_sample();
    let page = first_page(&session);
    let new_id = fresh_id(0xA1);

    let summary = session
        .apply(&request_now(
            &session,
            vec![create(
                &new_id,
                "shape",
                Some("Added"),
                NodeParent::PageRoot {
                    page: page.to_string(),
                },
            )],
        ))
        .expect("the create applies");

    assert_eq!(summary.added_nodes, vec![new_id.clone()]);
    assert_eq!(summary.revision, session.revision().as_u64());

    // The new node is the last root, after the Card frame.
    let view = session.view().expect("the view is available");
    assert_eq!(
        view.pages[0].roots.last().map(String::as_str),
        Some(new_id.as_str()),
        "a created node lands at the end of the page's roots"
    );
    assert_eq!(view.nodes.last().unwrap().id, new_id);
    assert_eq!(view.nodes.last().unwrap().name, "Added");
    assert_eq!(view.nodes.last().unwrap().kind, "shape");

    // Default properties for the kind, so the node is immediately renderable.
    let props = session.node_props(&new_id).expect("the node exists");
    assert_eq!(props.shape_geometry.as_deref(), Some("rect"));
    assert_eq!(props.fill, None);
}

#[test]
fn a_created_child_goes_under_its_parent() {
    let mut session = open_sample();
    let new_id = fresh_id(0xA2);

    session
        .apply(&request_now(
            &session,
            vec![create(
                &new_id,
                "shape",
                None,
                NodeParent::Child {
                    parent: sample::CARD.to_owned(),
                },
            )],
        ))
        .expect("the create applies");

    let view = session.view().expect("the view is available");
    let card = view
        .nodes
        .iter()
        .find(|node| node.id == sample::CARD)
        .expect("the frame is present");
    assert_eq!(
        card.children.last().map(String::as_str),
        Some(new_id.as_str()),
        "a created child lands at the end of its parent's children"
    );
    assert_eq!(
        view.nodes
            .iter()
            .find(|node| node.id == new_id)
            .unwrap()
            .parent
            .as_deref(),
        Some(sample::CARD)
    );
}

#[test]
fn a_create_with_a_taken_identity_is_refused() {
    let mut session = open_sample();
    let revision = session.revision().as_u64();

    let error = session
        .apply(&request_now(
            &session,
            vec![create(
                sample::HEADER,
                "shape",
                None,
                NodeParent::PageRoot {
                    page: first_page(&session).to_string(),
                },
            )],
        ))
        .expect_err("the identity is already taken");

    assert_eq!(error.code, AppErrorCode::CommandRejected);
    assert_eq!(error.node.as_deref(), Some(sample::HEADER));
    assert_eq!(
        session.revision().as_u64(),
        revision,
        "nothing was consumed"
    );
}

#[test]
fn an_unknown_kind_is_an_invalid_request() {
    let mut session = open_sample();
    let revision = session.revision().as_u64();

    let error = session
        .apply(&request_now(
            &session,
            vec![create(
                &fresh_id(0xA3),
                "spline",
                None,
                NodeParent::PageRoot {
                    page: first_page(&session).to_string(),
                },
            )],
        ))
        .expect_err("the kind does not exist");

    assert_eq!(error.code, AppErrorCode::InvalidRequest);
    assert!(error.message.contains("spline"));
    assert_eq!(session.revision().as_u64(), revision);
}

#[test]
fn a_create_under_an_unknown_parent_is_refused() {
    let mut session = open_sample();
    let revision = session.revision().as_u64();

    let error = session
        .apply(&request_now(
            &session,
            vec![create(
                &fresh_id(0xA4),
                "shape",
                None,
                NodeParent::Child {
                    parent: sample::ABSENT.to_owned(),
                },
            )],
        ))
        .expect_err("the parent does not exist");

    assert_eq!(error.code, AppErrorCode::UnknownNode);
    assert_eq!(session.revision().as_u64(), revision);
}

#[test]
fn deleting_a_leaf_removes_only_that_node() {
    let mut session = open_sample();
    let revision = session.revision().as_u64();

    let summary = session
        .apply(&request_now(&session, vec![delete(sample::DOT)]))
        .expect("the delete applies");

    assert_eq!(summary.removed_nodes, vec![sample::DOT.to_owned()]);

    let view = session.view().expect("the view is available");
    assert_eq!(view.nodes.len(), 4, "one node left the document");
    assert!(
        !view.nodes.iter().any(|node| node.id == sample::DOT),
        "the deleted node is gone"
    );
    let card = view
        .nodes
        .iter()
        .find(|node| node.id == sample::CARD)
        .unwrap();
    assert!(
        !card.children.iter().any(|id| id == sample::DOT),
        "the parent no longer names the deleted child"
    );
    assert_eq!(session.revision().as_u64(), revision + 1);
}

#[test]
fn deleting_a_container_removes_its_whole_subtree() {
    let mut session = open_sample();

    let summary = session
        .apply(&request_now(&session, vec![delete(sample::CARD)]))
        .expect("the delete applies");

    // The frame and its four children are all gone, which is what "removes the
    // subtree" means.
    assert_eq!(summary.removed_nodes.len(), 5);
    let view = session.view().expect("the view is available");
    assert!(view.nodes.is_empty());
    assert!(view.pages[0].roots.is_empty());
}

#[test]
fn deleting_an_unknown_node_is_refused_without_consuming_a_revision() {
    let mut session = open_sample();
    let revision = session.revision().as_u64();

    let error = session
        .apply(&request_now(&session, vec![delete(sample::ABSENT)]))
        .expect_err("the node does not exist");

    assert_eq!(error.code, AppErrorCode::UnknownNode);
    assert_eq!(session.revision().as_u64(), revision);
}

#[test]
fn undo_restores_a_deleted_subtree_and_redo_removes_it_again() {
    let mut session = open_sample();

    session
        .apply(&request_now(&session, vec![delete(sample::CARD)]))
        .expect("the delete applies");
    assert!(session.view().unwrap().nodes.is_empty());

    session
        .undo(session.revision().as_u64())
        .expect("there is a step to undo");
    let view = session.view().expect("the view is available");
    assert_eq!(view.nodes.len(), 5, "the whole subtree came back");
    assert_eq!(view.pages[0].roots, vec![sample::CARD.to_owned()]);

    session
        .redo(session.revision().as_u64())
        .expect("there is a step to redo");
    assert!(session.view().unwrap().nodes.is_empty());
}

#[test]
fn a_moved_node_keeps_its_identity_and_changes_its_parent() {
    let mut session = open_sample();
    let new_id = fresh_id(0xA5);

    session
        .apply(&request_now(
            &session,
            vec![
                create(
                    &new_id,
                    "shape",
                    None,
                    NodeParent::PageRoot {
                        page: first_page(&session).to_string(),
                    },
                ),
                move_node(
                    &new_id,
                    NodeParent::Child {
                        parent: sample::CARD.to_owned(),
                    },
                ),
            ],
        ))
        .expect("the create and move apply");

    let view = session.view().expect("the view is available");
    assert_eq!(
        view.pages[0].roots.len(),
        1,
        "the node left the page's roots"
    );
    let moved = view.nodes.iter().find(|node| node.id == new_id).unwrap();
    assert_eq!(moved.parent.as_deref(), Some(sample::CARD));
    let card = view
        .nodes
        .iter()
        .find(|node| node.id == sample::CARD)
        .unwrap();
    assert_eq!(card.children.len(), 5, "the frame gained a fifth child");
}

#[test]
fn a_created_node_can_carry_its_fill_from_the_same_batch() {
    let mut session = open_sample();
    let new_id = fresh_id(0xA6);
    let fill = Rgba::opaque(12, 200, 90);

    // One batch: create a shape with a fill. The fill travels in the create
    // command because a property edit reads the node's current properties, and a
    // node the same batch creates has none yet.
    let summary = session
        .apply(&request_now(
            &session,
            vec![create_filled(
                &new_id,
                "shape",
                fill,
                NodeParent::PageRoot {
                    page: first_page(&session).to_string(),
                },
            )],
        ))
        .expect("the batch applies");

    assert_eq!(summary.added_nodes, vec![new_id.clone()]);
    assert_eq!(
        session.node_props(&new_id).unwrap().fill,
        Some(fill),
        "the fill landed on the node this batch created"
    );
}

#[test]
fn a_fill_on_a_node_created_in_an_earlier_batch_is_applied() {
    let mut session = open_sample();
    let new_id = fresh_id(0xA8);
    let fill = Rgba::opaque(9, 9, 9);

    session
        .apply(&request_now(
            &session,
            vec![create(
                &new_id,
                "shape",
                Some("Badge"),
                NodeParent::PageRoot {
                    page: first_page(&session).to_string(),
                },
            )],
        ))
        .expect("the create applies");

    // A separate batch, so the node exists before the property edit is built.
    session
        .apply(&request_now(&session, vec![set_fill(&new_id, Some(fill))]))
        .expect("the fill applies");

    assert_eq!(session.node_props(&new_id).unwrap().fill, Some(fill));
}

#[test]
fn a_property_edit_of_a_node_created_in_the_same_batch_is_refused() {
    let mut session = open_sample();
    let revision = session.revision().as_u64();
    let new_id = fresh_id(0xA9);

    // A property edit reads the node's current properties to build the
    // replacement, and a node created earlier in the same batch has none. The
    // refusal is explicit rather than a silently dropped fill, and the batch
    // leaves nothing behind.
    let error = session
        .apply(&request_now(
            &session,
            vec![
                create(
                    &new_id,
                    "shape",
                    None,
                    NodeParent::PageRoot {
                        page: first_page(&session).to_string(),
                    },
                ),
                set_fill(&new_id, Some(Rgba::opaque(1, 1, 1))),
            ],
        ))
        .expect_err("the property edit cannot read a node that does not exist yet");

    assert_eq!(error.code, AppErrorCode::UnknownNode);
    assert_eq!(error.node.as_deref(), Some(new_id.as_str()));
    assert_eq!(session.revision().as_u64(), revision);
    assert!(
        session
            .view()
            .unwrap()
            .nodes
            .iter()
            .all(|node| node.id != new_id),
        "the whole batch was rolled back"
    );
}

#[test]
fn a_failed_create_in_a_batch_with_a_later_edit_leaves_no_trace() {
    let mut session = open_sample();
    let revision = session.revision().as_u64();

    let error = session
        .apply(&request_now(
            &session,
            vec![
                create(
                    sample::HEADER,
                    "shape",
                    None,
                    NodeParent::PageRoot {
                        page: first_page(&session).to_string(),
                    },
                ),
                set_fill(sample::DOT, Some(Rgba::opaque(1, 1, 1))),
            ],
        ))
        .expect_err("the create names a taken identity");

    assert_eq!(error.code, AppErrorCode::CommandRejected);
    assert_eq!(session.revision().as_u64(), revision);
    assert!(
        !session.is_dirty(),
        "the refused batch changed nothing, including the later edit"
    );
    assert_eq!(
        session.node_props(sample::DOT).unwrap().fill,
        Some(Rgba::opaque(255, 107, 107)),
        "the dot keeps its original fill"
    );
}

#[test]
fn a_create_command_crosses_the_wire_in_the_documented_shape() {
    let request = EditRequest {
        expected_revision: 2,
        commands: vec![create(
            &fresh_id(0xA7),
            "text",
            Some("Label"),
            NodeParent::Child {
                parent: sample::CARD.to_owned(),
            },
        )],
    };
    let json = serde_json::to_value(&request).expect("the request serializes");

    let command = &json["commands"][0];
    assert_eq!(command["kind"], serde_json::json!("create-node"));
    assert_eq!(command["nodeKind"], serde_json::json!("text"));
    assert_eq!(command["parent"]["in"], serde_json::json!("child"));
    assert_eq!(command["parent"]["parent"], serde_json::json!(sample::CARD));

    let parsed: EditRequest = serde_json::from_value(json).expect("the request parses");
    assert_eq!(parsed, request);
}

#[test]
fn an_error_crosses_the_wire_with_a_code_and_its_subject() {
    let mut session = open_sample();

    let error = session
        .apply(&request_now(
            &session,
            vec![set_fill(sample::ABSENT, Some(Rgba::opaque(1, 1, 1)))],
        ))
        .expect_err("the node does not exist");
    let json = serde_json::to_value(&error).expect("the error serializes");

    assert_eq!(json["code"], serde_json::json!("unknown-node"));
    assert_eq!(json["node"], serde_json::json!(sample::ABSENT));
    assert!(json["message"].is_string(), "the message is for a person");
    assert!(
        json.get("revision").is_none(),
        "an absent subject is omitted rather than sent as null"
    );
}

#[test]
fn an_edit_request_crosses_the_wire_in_the_documented_shape() {
    let request = EditRequest {
        expected_revision: 3,
        commands: vec![set_fill(sample::HEADER, Some(Rgba::opaque(1, 2, 3)))],
    };
    let json = serde_json::to_value(&request).expect("the request serializes");

    assert_eq!(json["expectedRevision"], serde_json::json!(3));
    assert_eq!(
        json["commands"][0]["kind"],
        serde_json::json!("set-node-fill")
    );
    assert_eq!(
        json["commands"][0]["node"],
        serde_json::json!(sample::HEADER)
    );
    assert_eq!(json["commands"][0]["fill"]["a"], serde_json::json!(255));

    // The same shape parses back, so a host can round-trip a request.
    let parsed: EditRequest = serde_json::from_value(json).expect("the request parses");
    assert_eq!(parsed, request);
}
