//! The M0 vertical slice: one document through every stage of the system.
//!
//! This is the gate the technical specification §12.2 defines. It walks one
//! small document through the whole path and asserts what the slice is for:
//!
//! 1. A document with a page, a frame, shapes, and text in known fonts.
//! 2. Validated structure, saved and reopened through the versioned DTO.
//! 3. Laid out by one layout adapter and rasterized by one renderer, both behind
//!    their documented contracts.
//! 4. A PNG and a layout report compared against committed references whose
//!    fingerprint pins the backend, the fonts, the scale, and the colour.
//! 5. An edit in a transaction, with undo, redo, and a round trip after saving.
//! 6. A batch whose last command is invalid, leaving no trace.
//!
//! The reference files are `tests/golden/m0-sample.png`,
//! `tests/golden/m0-sample.layout.json`, and
//! `tests/golden/m0-sample.fingerprint.json`. They are regenerated with
//! `SWOTVIBE_UPDATE_GOLDEN=1 cargo test -p swotvibe-tools --test m0_vertical_slice`
//! and a human reviews them before they are committed: see
//! `tests/golden/README.md` for what that review covers.
//!
//! What this test does **not** prove: market fit, performance on large
//! documents, or that the chosen backends are the right ones. It proves the
//! kernel and the contracts work together on one real flow.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{Value, json};
use swotvibe_core::{Color, Command, DocumentEngine, NodeId, PageId, Snapshot, validate_untrusted};
use swotvibe_format::{SCHEMA_VERSION, export, from_json, import, to_json};
use swotvibe_layout::{
    LayoutEngine, LayoutOptions, LayoutResult, TaffyLayoutEngine, engine_id as layout_engine_id,
};
use swotvibe_render::{
    PngDecodeLimits, RenderConfig, RenderedPage, Renderer, VelloCpuRenderer, compare_images,
};
use swotvibe_text::{FontSet, ParleyTextEngine, TextLayoutEngine};

/// The pinned fonts, loaded from `assets/fonts`.
const LATIN_FAMILY: &str = "Inter";
const ARABIC_FAMILY: &str = "Noto Sans Arabic";

/// The comparison rules for the reference report.
///
/// The tolerances are part of the contract, not a convenience: a layout
/// comparison is a numeric comparison of derived values, so it is documented
/// here and in `tests/golden/README.md` rather than hidden in an assertion.
const GEOMETRY_TOLERANCE: f64 = 1.0e-6;
const TRANSFORM_TOLERANCE: f64 = 1.0e-9;
const PIXEL_TOLERANCE: u8 = 2;

fn repository_root() -> PathBuf {
    let mut directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    loop {
        if directory.join("assets").join("fonts").is_dir() {
            return directory;
        }
        assert!(directory.pop(), "cannot find the repository root");
    }
}

fn golden_path(name: &str) -> PathBuf {
    repository_root().join("tests").join("golden").join(name)
}

/// The reference image for this platform, or the shared one.
///
/// `tests/golden/README.md` allows a per-OS reference because rasterization can
/// differ by architecture, and it forbids averaging two platforms into one file.
/// A platform-specific file is used when it exists, so a divergence is resolved
/// by committing the platform's own reference rather than by widening the
/// tolerance.
fn reference_image_path() -> PathBuf {
    if updating_goldens() {
        return golden_path("m0-sample.png");
    }
    let platform = if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    };
    let specific = golden_path(&format!("m0-sample.{platform}.png"));
    if specific.is_file() {
        specific
    } else {
        golden_path("m0-sample.png")
    }
}

fn updating_goldens() -> bool {
    std::env::var_os("SWOTVIBE_UPDATE_GOLDEN").is_some()
}

fn pinned_fonts() -> FontSet {
    let mut fonts = FontSet::new();
    fonts
        .register(
            LATIN_FAMILY,
            read(repository_root().join("assets/fonts/inter/Inter-variable.ttf")),
        )
        .expect("the Latin face should register");
    fonts
        .register(
            ARABIC_FAMILY,
            read(
                repository_root().join("assets/fonts/noto-sans-arabic/NotoSansArabic-variable.ttf"),
            ),
        )
        .expect("the Arabic face should register");
    fonts
}

fn read(path: PathBuf) -> Vec<u8> {
    std::fs::read(&path).unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()))
}

fn sample_bytes() -> Vec<u8> {
    read(repository_root().join("tests/fixtures/m0-sample-v2.json"))
}

/// The stage a rendering pass produced, kept together so a test can assert on
/// the same numbers a reader sees in the report.
struct Pass {
    document: swotvibe_core::Document,
    page: PageId,
    layout: LayoutResult,
    render: RenderedPage,
    text: Arc<dyn TextLayoutEngine>,
}

const PAGE_SIZE: (f64, f64) = (480.0, 320.0);
const SCALE: f64 = 1.0;
const BACKGROUND: Color = Color::rgb(255, 255, 255);

fn pass(document: swotvibe_core::Document) -> Pass {
    let text: Arc<dyn TextLayoutEngine> = Arc::new(ParleyTextEngine::new(pinned_fonts()));
    let page = document.pages()[0].id;
    let snapshot = Snapshot::of(&document);
    let layout = TaffyLayoutEngine::new(Arc::clone(&text))
        .layout(
            &snapshot,
            page,
            &LayoutOptions {
                page_size: Some(PAGE_SIZE),
            },
        )
        .expect("the sample should lay out");
    let config = RenderConfig::for_page(layout.page_size(), SCALE, BACKGROUND);
    let render = VelloCpuRenderer::new(Arc::clone(&text))
        .render(&snapshot, page, &layout, &config)
        .expect("the sample should render");
    Pass {
        document,
        page,
        layout,
        render,
        text,
    }
}

/// The layout report: every node's geometry, plus the fingerprint that says what
/// produced it.
fn layout_report(pass: &Pass) -> Value {
    let nodes: Vec<Value> = pass
        .layout
        .nodes()
        .map(|node| {
            let name = pass
                .document
                .node(node.id)
                .map(|node| node.name().to_owned())
                .unwrap_or_default();
            let kind = pass
                .document
                .node(node.id)
                .map(|node| node.kind.as_str())
                .unwrap_or("unknown");
            json!({
                "id": node.id.to_string(),
                "name": name,
                "kind": kind,
                "offset": [node.offset.0, node.offset.1],
                "size": [node.size.0, node.size.1],
                "transform": node.transform.coefficients(),
                "world": node.world.coefficients(),
                "rect": [node.rect.x, node.rect.y, node.rect.width, node.rect.height],
            })
        })
        .collect();

    json!({
        "page": {
            "id": pass.page.to_string(),
            "size": [pass.layout.page_size().0, pass.layout.page_size().1],
        },
        "fingerprint": {
            "layoutEngine": pass.layout.fingerprint().engine,
            "units": pass.layout.fingerprint().units,
            "rounded": pass.layout.fingerprint().rounded,
            "renderEngine": pass.render.fingerprint.engine,
            "scale": pass.render.fingerprint.scale,
            "pixels": [pass.render.fingerprint.pixels.0, pass.render.fingerprint.pixels.1],
            "background": pass.render.fingerprint.background,
            "fonts": pass
                .render
                .fingerprint
                .fonts
                .iter()
                .map(|font| json!({
                    "family": font.family,
                    "bytes": font.bytes,
                    "sha256": font.sha256,
                }))
                .collect::<Vec<_>>(),
        },
        "diagnostics": pass
            .layout
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.to_string())
            .collect::<Vec<String>>(),
        "nodes": nodes,
    })
}

/// The image fingerprint, kept in its own file so a change to the backend or a
/// font fails with a readable difference rather than a pixel count.
fn image_fingerprint(pass: &Pass) -> Value {
    json!({
        "renderEngine": pass.render.fingerprint.engine,
        "layoutEngine": pass.render.fingerprint.layout_engine,
        "pixels": [pass.render.fingerprint.pixels.0, pass.render.fingerprint.pixels.1],
        "scale": pass.render.fingerprint.scale,
        "background": pass.render.fingerprint.background,
        "pixelTolerance": PIXEL_TOLERANCE,
        "fonts": pass
            .render
            .fingerprint
            .fonts
            .iter()
            .map(|font| json!({
                "family": font.family,
                "index": font.index,
                "bytes": font.bytes,
                "sha256": font.sha256,
            }))
            .collect::<Vec<_>>(),
        "reviewed": "pending human review of tests/golden/m0-sample.png",
    })
}

fn write_goldens(pass: &Pass) {
    let png = pass.render.image.to_png().expect("the image should encode");
    let report = serde_json::to_string_pretty(&layout_report(pass)).expect("the report serializes");
    let fingerprint =
        serde_json::to_string_pretty(&image_fingerprint(pass)).expect("the fingerprint serializes");
    for (name, bytes) in [
        ("m0-sample.png", png),
        ("m0-sample.layout.json", report.into_bytes()),
        ("m0-sample.fingerprint.json", fingerprint.into_bytes()),
    ] {
        let path = golden_path(name);
        std::fs::write(&path, &bytes)
            .unwrap_or_else(|error| panic!("cannot write {}: {error}", path.display()));
        println!("wrote {}", path.display());
    }
}

/// Compares two report values, using a tolerance for geometry and an exact
/// comparison for everything else.
fn compare_reports(actual: &Value, expected: &Value, path: &str) -> Vec<String> {
    let mut differences: Vec<String> = Vec::new();
    match (actual, expected) {
        (Value::Object(actual), Value::Object(expected)) => {
            for (key, expected) in expected {
                match actual.get(key) {
                    Some(actual) => differences.extend(compare_reports(
                        actual,
                        expected,
                        &format!("{path}.{key}"),
                    )),
                    None => differences.push(format!("{path}.{key} is missing")),
                }
            }
            for key in actual.keys() {
                if !expected.contains_key(key) {
                    differences.push(format!("{path}.{key} is new"));
                }
            }
        }
        (Value::Array(actual), Value::Array(expected)) => {
            if actual.len() != expected.len() {
                differences.push(format!(
                    "{path} has {} entries, expected {}",
                    actual.len(),
                    expected.len()
                ));
            }
            for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
                differences.extend(compare_reports(
                    actual,
                    expected,
                    &format!("{path}[{index}]"),
                ));
            }
        }
        (Value::Number(actual), Value::Number(expected)) => {
            let actual = actual.as_f64().unwrap_or(f64::NAN);
            let expected = expected.as_f64().unwrap_or(f64::NAN);
            // A transform or a world matrix is compared tightly, because it is
            // the composition result and not a rounded measurement; geometry
            // gets the documented, wider tolerance.
            let tolerance = if path.ends_with("transform") || path.ends_with("world") {
                TRANSFORM_TOLERANCE
            } else {
                GEOMETRY_TOLERANCE
            };
            // A NaN difference is not within any tolerance, and comparing with
            // `partial_cmp` makes that explicit instead of relying on `<=`.
            let difference = (actual - expected).abs();
            if !difference.is_finite() || difference > tolerance {
                differences.push(format!("{path} is {actual}, expected {expected}"));
            }
        }
        (Value::String(actual), Value::String(expected)) if actual == expected => {}
        (actual, expected) if actual == expected => {}
        (actual, expected) => {
            differences.push(format!("{path} is {actual}, expected {expected}"));
        }
    }
    differences
}

// ---------------------------------------------------------------------------
// Steps 1 and 2: build, validate, save, reopen.
// ---------------------------------------------------------------------------

#[test]
fn m0_step_1_and_2_the_sample_opens_validates_and_round_trips() {
    let dto = from_json(&sample_bytes()).expect("the sample should parse");
    assert_eq!(dto.schema_version, SCHEMA_VERSION);
    let document = import(&dto).expect("the sample should open");
    validate_untrusted(&document).expect("the opened document should be valid");

    // Saving and reopening must not lose meaning: the same nodes, names, kinds,
    // and properties come back, and the second save is byte-identical.
    let saved = to_json(&export(&document)).expect("the sample should save");
    let reopened = import(&from_json(&saved).expect("the saved document should parse"))
        .expect("the saved document should open");
    assert_eq!(reopened.node_count(), document.node_count());
    for node in document.nodes() {
        let other = reopened
            .node(node.id)
            .unwrap_or_else(|| panic!("node {} should survive the round trip", node.id));
        assert_eq!(other.kind, node.kind);
        assert_eq!(other.name, node.name);
        assert_eq!(other.props, node.props);
    }
    assert_eq!(
        to_json(&export(&reopened)).expect("the reopened document should save"),
        saved
    );
}

// ---------------------------------------------------------------------------
// Steps 3 and 4: lay out, render, and compare against the references.
// ---------------------------------------------------------------------------

#[test]
fn m0_step_3_and_4_the_sample_lays_out_and_renders_as_the_references_describe() {
    let document = import(&from_json(&sample_bytes()).expect("parse")).expect("open");
    let pass = pass(document);
    assert_eq!(
        LayoutEngine::engine_id(&TaffyLayoutEngine::new(Arc::clone(&pass.text))),
        layout_engine_id()
    );

    if updating_goldens() {
        write_goldens(&pass);
        return;
    }

    // The layout report is compared field by field, so a difference names a node
    // and a value rather than a byte offset.
    let expected_report: Value = serde_json::from_str(
        &String::from_utf8(read(golden_path("m0-sample.layout.json")))
            .expect("the report is UTF-8"),
    )
    .expect("the golden report should parse");
    let differences = compare_reports(&layout_report(&pass), &expected_report, "$");
    assert!(
        differences.is_empty(),
        "the layout report differs from the committed reference:\n  {}",
        differences.join("\n  ")
    );

    // The image fingerprint is compared exactly: a backend, font, scale, or
    // colour change is not a tolerance question.
    let expected_fingerprint: Value = serde_json::from_str(
        &String::from_utf8(read(golden_path("m0-sample.fingerprint.json")))
            .expect("the fingerprint is UTF-8"),
    )
    .expect("the golden fingerprint should parse");
    let mut expected_fingerprint = expected_fingerprint;
    if let Some(object) = expected_fingerprint.as_object_mut() {
        // The review verdict is a human statement, not part of the comparison.
        object.remove("reviewed");
    }
    let mut actual_fingerprint = image_fingerprint(&pass);
    if let Some(object) = actual_fingerprint.as_object_mut() {
        object.remove("reviewed");
    }
    assert_eq!(
        actual_fingerprint, expected_fingerprint,
        "the render fingerprint changed; regenerate the reference deliberately"
    );

    // The pixels are compared with the documented tolerance.
    let reference_path = reference_image_path();
    let reference_bytes = read(reference_path.clone());
    let reference = swotvibe_render::RenderedImage::from_png(
        &reference_bytes,
        PngDecodeLimits {
            max_input_bytes: 16 * 1024 * 1024,
            max_width: 1440,
            max_height: 900,
            max_pixels: 1440 * 900,
            max_output_bytes: 1440 * 900 * 4,
            max_decoder_bytes: 32 * 1024 * 1024,
        },
    )
    .expect("the reference image should decode within its test budget");
    let difference = compare_images(&pass.render.image, &reference, PIXEL_TOLERANCE);
    assert!(
        difference.is_match(),
        "the rendered image differs from {}: {difference:?}\n\
         If this platform rasterizes differently, commit its own reference as \
         tests/golden/m0-sample.{}.png and record the fingerprint beside it, rather than \
         widening the tolerance; see tests/golden/README.md.",
        reference_path.display(),
        std::env::consts::OS
    );
    assert!(
        pass.render.diagnostics.is_empty(),
        "the sample should render without omissions, found {:?}",
        pass.render.diagnostics
    );
}

// ---------------------------------------------------------------------------
// Step 5: an edit, undo, redo, and a round trip.
// ---------------------------------------------------------------------------

#[test]
fn m0_step_5_a_property_change_moves_geometry_and_survives_undo_redo_and_a_save() {
    let document = import(&from_json(&sample_bytes()).expect("parse")).expect("open");
    let mut engine = DocumentEngine::with_document(document);
    let card = node_named(engine.document(), "Card");
    let header = node_named(engine.document(), "Header");

    let before = pass(engine.document().clone());
    let header_before = *before.layout.node(header).expect("the header is laid out");
    let latin_before = *before
        .layout
        .node(node_named(engine.document(), "Latin"))
        .expect("the Latin line is laid out");

    // One transaction: the card's padding grows, which must move every child.
    let mut props = engine
        .document()
        .node(card)
        .expect("the card")
        .props
        .clone();
    if let swotvibe_core::Content::Frame(frame) = &mut props.content
        && let swotvibe_core::FrameLayout::Flex(flex) = &mut frame.layout
    {
        flex.padding =
            swotvibe_core::Insets::uniform(swotvibe_core::Scalar::new(40.0).expect("finite"));
    }
    let before_revision = engine.revision();
    let commit = engine
        .apply(&[Command::SetNodeProps {
            id: card,
            props: Box::new(props),
        }])
        .expect("the edit should apply");
    assert_eq!(commit.revision(), before_revision.next());
    assert!(commit.change_set().changed_nodes().any(|id| id == card));

    let after = pass(engine.document().clone());
    let header_after = *after.layout.node(header).expect("the header is laid out");
    let latin_after = *after
        .layout
        .node(node_named(engine.document(), "Latin"))
        .expect("the Latin line is laid out");
    assert_eq!(header_after.offset, (40.0, 40.0));
    assert_ne!(
        header_after.offset, header_before.offset,
        "the edit should move the children"
    );
    assert_ne!(latin_after.offset, latin_before.offset);
    // The rendered pixels change too, which is the point of the slice: an edit
    // reaches the image.
    assert!(
        !compare_images(&after.render.image, &before.render.image, PIXEL_TOLERANCE).is_match(),
        "the edit should change the rendered image"
    );

    // One undo restores the previous state exactly, and redo reapplies it.
    engine.undo().expect("undo should apply");
    let undone = pass(engine.document().clone());
    assert_eq!(
        undone.layout.node(header).expect("the header").offset,
        header_before.offset
    );
    assert!(
        compare_images(&undone.render.image, &before.render.image, PIXEL_TOLERANCE).is_match(),
        "undo should restore the image"
    );

    engine.redo().expect("redo should apply");
    let redone = pass(engine.document().clone());
    assert_eq!(
        redone.layout.node(header).expect("the header").offset,
        header_after.offset
    );
    assert_eq!(
        redone.layout, after.layout,
        "redo should restore the geometry the edit produced"
    );
    assert_eq!(redone.document.revision(), engine.revision());

    // The edited document still round-trips, and the reopened copy lays out to
    // the same geometry.
    let saved = to_json(&export(engine.document())).expect("the edited document should save");
    let reopened = import(&from_json(&saved).expect("the saved document should parse"))
        .expect("the saved document should open");
    let reopened_pass = pass(reopened);
    assert_eq!(
        reopened_pass.layout, redone.layout,
        "a saved and reopened document must lay out identically"
    );
}

#[test]
fn m0_step_5_a_reorder_changes_the_painted_order_and_is_one_undoable_step() {
    let document = import(&from_json(&sample_bytes()).expect("parse")).expect("open");
    let mut engine = DocumentEngine::with_document(document);
    let card = node_named(engine.document(), "Card");
    let dot = node_named(engine.document(), "Dot");
    let header = node_named(engine.document(), "Header");

    let before = engine.document().clone();
    assert_eq!(
        before.node(card).expect("the card").children(),
        &[
            header,
            node_named(&before, "Latin"),
            node_named(&before, "Arabic"),
            dot
        ]
    );

    engine
        .apply(&[Command::ReorderNode {
            id: dot,
            position: swotvibe_core::Position::First,
        }])
        .expect("the reorder should apply");
    assert_eq!(
        engine.document().node(card).expect("the card").children()[0],
        dot,
        "the dot should move to the front"
    );
    // The first child now starts at the top of the card, so the header moved.
    let after = pass(engine.document().clone());
    let header_after = after.layout.node(header).expect("the header").offset;
    assert!(
        header_after.1 > 16.0,
        "the header should no longer be the first child, found {header_after:?}"
    );

    engine.undo().expect("undo should apply");
    assert_eq!(
        engine.document().node(card).expect("the card").children(),
        &[
            header,
            node_named(&before, "Latin"),
            node_named(&before, "Arabic"),
            dot
        ]
    );

    let saved = to_json(&export(engine.document())).expect("the document should save");
    let reopened = import(&from_json(&saved).expect("parse")).expect("open");
    assert_eq!(
        reopened.node(card).expect("the card").children(),
        before.node(card).expect("the card").children()
    );
}

// ---------------------------------------------------------------------------
// Step 6: a failed batch leaves no trace.
// ---------------------------------------------------------------------------

#[test]
fn m0_step_6_a_batch_that_fails_at_its_last_command_leaves_no_trace() {
    let document = import(&from_json(&sample_bytes()).expect("parse")).expect("open");
    let mut engine = DocumentEngine::with_document(document);
    let card = node_named(engine.document(), "Card");

    let revision = engine.revision();
    let undo_depth = engine.history().undo_depth();
    let before = engine.document().clone();

    // A valid property change followed by a command whose target does not exist.
    let mut props = engine
        .document()
        .node(card)
        .expect("the card")
        .props
        .clone();
    props.fill = Some(Color::rgb(1, 2, 3));
    let error = engine
        .apply(&[
            Command::SetNodeProps {
                id: card,
                props: Box::new(props),
            },
            Command::DeleteNode { id: NodeId::new() },
        ])
        .expect_err("the batch must fail as a whole");
    assert_eq!(
        error.code(),
        Some(swotvibe_core::CommandErrorCode::NodeNotFound)
    );

    assert_eq!(engine.revision(), revision, "the revision must not advance");
    assert_eq!(
        engine.history().undo_depth(),
        undo_depth,
        "nothing may be recorded for an undo"
    );
    assert_eq!(
        export(engine.document()),
        export(&before),
        "the document must be untouched"
    );
    validate_untrusted(engine.document()).expect("the document must stay valid");

    let untouched = pass(engine.document().clone());
    assert_eq!(
        untouched.layout,
        pass(before).layout,
        "the geometry must be exactly what it was"
    );
}

fn node_named(document: &swotvibe_core::Document, name: &str) -> NodeId {
    document
        .nodes()
        .find(|node| node.name() == name)
        .unwrap_or_else(|| panic!("the sample should have a `{name}` node"))
        .id
}

#[test]
fn the_reference_files_exist_where_the_documentation_says_they_do() {
    // A guard so a rename of the golden directory cannot silently skip the
    // comparison above.
    let directory = repository_root().join("tests").join("golden");
    assert!(Path::new(&directory).is_dir());
    if !updating_goldens() {
        for name in [
            "m0-sample.png",
            "m0-sample.layout.json",
            "m0-sample.fingerprint.json",
        ] {
            let path = directory.join(name);
            assert!(
                path.is_file(),
                "the reference {} is missing",
                path.display()
            );
        }
    }
}
