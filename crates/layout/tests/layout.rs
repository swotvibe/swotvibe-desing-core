//! Layout behaviour against the pinned fonts and the sample document.
//!
//! The focused cases build their own documents so a failure names one rule. The
//! last cases lay out the committed sample the M0 gate renders, which is what
//! makes the geometry in that golden reproducible.

// The helper lives beside the text crate's tests so one definition serves the
// text, layout, and render reference tests. It is included by path rather than
// depended on because a test-only helper does not belong in any crate's public
// surface, and a fourth crate for it would be more machinery than it saves.
#[path = "../../text/tests/support/mod.rs"]
mod support;

use std::sync::Arc;

use support::{ARABIC_FAMILY, LATIN_FAMILY, fixture_text, pinned_fonts};
use swotvibe_core::{
    Axis, Color, Command, Content, CrossAlign, Document, FlexLayout, FrameLayout, FrameProps,
    Insets, MainAlign, NodeId, NodeKind, NodePlacement, NodeProps, PageId, Position, Scalar,
    ShapeGeometry, ShapeProps, Size, Sizing, Snapshot, TextAlign, TextDirection, Transform,
    apply_batch_at_current,
};
use swotvibe_format::{from_json, import};
use swotvibe_layout::{
    LayoutDiagnostic, LayoutEngine, LayoutError, LayoutOptions, TaffyLayoutEngine, engine_id,
};
use swotvibe_text::{ParleyTextEngine, TextLayoutEngine};

struct Fixture {
    document: Document,
    page: PageId,
    engine: TaffyLayoutEngine,
}

impl Fixture {
    fn new(kinds: &[(NodeKind, NodeProps)]) -> Self {
        let mut document = Document::new();
        let page = PageId::new();
        let mut commands = vec![Command::CreatePage {
            id: page,
            name: "Page 1".into(),
        }];
        for (kind, _) in kinds {
            commands.push(Command::CreateNode {
                id: NodeId::new(),
                kind: *kind,
                name: None,
                parent: NodePlacement::PageRoot {
                    page,
                    position: Position::Last,
                },
            });
        }
        apply_batch_at_current(&mut document, &commands).expect("the fixture should build");

        let ids: Vec<NodeId> = document
            .page(page)
            .expect("the page exists")
            .roots()
            .to_vec();
        let property_commands: Vec<Command> = ids
            .iter()
            .zip(kinds)
            .map(|(id, (_, props))| Command::SetNodeProps {
                id: *id,
                props: Box::new(props.clone()),
            })
            .collect();
        apply_batch_at_current(&mut document, &property_commands).expect("properties should apply");

        Self {
            document,
            page,
            engine: TaffyLayoutEngine::new(Arc::new(ParleyTextEngine::new(pinned_fonts()))),
        }
    }

    fn roots(&self) -> Vec<NodeId> {
        self.document
            .page(self.page)
            .expect("the page exists")
            .roots()
            .to_vec()
    }

    fn layout(&self) -> swotvibe_layout::LayoutResult {
        let snapshot = Snapshot::of(&self.document);
        self.engine
            .layout(
                &snapshot,
                self.page,
                &LayoutOptions {
                    page_size: Some((1000.0, 1000.0)),
                },
            )
            .expect("layout should succeed")
    }
}

fn props(kind: NodeKind) -> NodeProps {
    NodeProps::default_for(kind)
}

fn shaped(size: (f64, f64), transform: Transform) -> NodeProps {
    let mut props = props(NodeKind::Shape);
    props.size = Size::new(size.0, size.1).expect("non-negative");
    props.transform = transform;
    props
}

fn text(content: &str, family: &str, size: f64) -> NodeProps {
    let mut props = props(NodeKind::Text);
    if let Content::Text(text) = &mut props.content {
        text.content = content.into();
        text.font_family = family.into();
        text.font_size = Scalar::new(size).expect("finite");
        text.direction = TextDirection::Auto;
        text.align = TextAlign::Start;
    }
    props
}

fn close(actual: f64, expected: f64) -> bool {
    (actual - expected).abs() < 0.5
}

#[test]
fn a_root_with_no_layout_parent_is_positioned_by_its_own_transform() {
    let fixture = Fixture::new(&[(
        NodeKind::Shape,
        shaped(
            (100.0, 50.0),
            Transform::translate(30.0, 20.0).expect("finite"),
        ),
    )]);
    let result = fixture.layout();
    let layout = result.node(fixture.roots()[0]).expect("a laid out node");

    assert_eq!(layout.offset, (30.0, 20.0));
    assert_eq!(layout.size, (100.0, 50.0));
    assert_eq!(layout.rect.x, 30.0);
    assert_eq!(layout.rect.y, 20.0);
    assert_eq!((layout.rect.width, layout.rect.height), (100.0, 50.0));
}

#[test]
fn a_rotated_node_keeps_its_size_and_gets_a_larger_bounding_box() {
    let rotation = Transform::translate(100.0, 100.0)
        .expect("finite")
        .compose(&Transform::rotate(std::f64::consts::FRAC_PI_2).expect("finite"))
        .expect("finite");
    let fixture = Fixture::new(&[(NodeKind::Shape, shaped((40.0, 20.0), rotation))]);
    let result = fixture.layout();
    let layout = result.node(fixture.roots()[0]).expect("a laid out node");

    // The box is rotated: the same size, but the axis-aligned bounds swap axes.
    assert_eq!(layout.size, (40.0, 20.0));
    assert!(
        close(layout.rect.width, 20.0) && close(layout.rect.height, 40.0),
        "a quarter turn should swap the bounds, found {:?}",
        layout.rect
    );
    assert_eq!(layout.offset, (100.0, 100.0));
}

#[test]
fn a_child_of_a_frame_without_layout_uses_its_own_translation() {
    let mut document = Document::new();
    let page = PageId::new();
    let frame = NodeId::new();
    let child = NodeId::new();
    apply_batch_at_current(
        &mut document,
        &[
            Command::CreatePage {
                id: page,
                name: "Page 1".into(),
            },
            Command::CreateNode {
                id: frame,
                kind: NodeKind::Frame,
                name: None,
                parent: NodePlacement::PageRoot {
                    page,
                    position: Position::Last,
                },
            },
            Command::CreateNode {
                id: child,
                kind: NodeKind::Shape,
                name: None,
                parent: NodePlacement::Child {
                    parent: frame,
                    position: Position::Last,
                },
            },
        ],
    )
    .expect("the fixture should build");

    let mut frame_props = props(NodeKind::Frame);
    frame_props.size = Size::new(200.0, 100.0).expect("non-negative");
    frame_props.transform = Transform::translate(10.0, 10.0).expect("finite");
    frame_props.content = Content::Frame(FrameProps {
        layout: FrameLayout::None,
    });

    apply_batch_at_current(
        &mut document,
        &[
            Command::SetNodeProps {
                id: frame,
                props: Box::new(frame_props),
            },
            Command::SetNodeProps {
                id: child,
                props: Box::new(shaped(
                    (50.0, 50.0),
                    Transform::translate(25.0, 5.0).expect("finite"),
                )),
            },
        ],
    )
    .expect("properties should apply");

    let engine = TaffyLayoutEngine::new(Arc::new(ParleyTextEngine::new(pinned_fonts())));
    let snapshot = Snapshot::of(&document);
    let result = engine
        .layout(
            &snapshot,
            page,
            &LayoutOptions {
                page_size: Some((400.0, 400.0)),
            },
        )
        .expect("layout should succeed");

    let frame_layout = result.node(frame).expect("the frame");
    let child_layout = result.node(child).expect("the child");
    assert_eq!(frame_layout.offset, (10.0, 10.0));
    assert_eq!(
        child_layout.offset,
        (25.0, 5.0),
        "a child of a frameless container is placed by its own translation"
    );
    // Page space: the frame's transform composes with the child's own.
    assert_eq!(child_layout.rect.x, 35.0);
    assert_eq!(child_layout.rect.y, 15.0);
}

#[test]
fn a_flex_child_is_placed_by_layout_and_not_by_its_translation() {
    let mut document = Document::new();
    let page = PageId::new();
    let frame = NodeId::new();
    let first = NodeId::new();
    let second = NodeId::new();
    apply_batch_at_current(
        &mut document,
        &[
            Command::CreatePage {
                id: page,
                name: "Page 1".into(),
            },
            Command::CreateNode {
                id: frame,
                kind: NodeKind::Frame,
                name: None,
                parent: NodePlacement::PageRoot {
                    page,
                    position: Position::Last,
                },
            },
            Command::CreateNode {
                id: first,
                kind: NodeKind::Shape,
                name: None,
                parent: NodePlacement::Child {
                    parent: frame,
                    position: Position::Last,
                },
            },
            Command::CreateNode {
                id: second,
                kind: NodeKind::Shape,
                name: None,
                parent: NodePlacement::Child {
                    parent: frame,
                    position: Position::Last,
                },
            },
        ],
    )
    .expect("the fixture should build");

    let mut frame_props = props(NodeKind::Frame);
    frame_props.size = Size::new(300.0, 200.0).expect("non-negative");
    frame_props.content = Content::Frame(FrameProps {
        layout: FrameLayout::Flex(FlexLayout {
            direction: Axis::Column,
            gap: Scalar::new(10.0).expect("finite"),
            padding: Insets::uniform(Scalar::new(20.0).expect("finite")),
            main_align: MainAlign::Start,
            cross_align: CrossAlign::Start,
        }),
    });

    // The translation on a flex child is deliberately ignored by layout.
    let mut moved = shaped(
        (100.0, 30.0),
        Transform::translate(999.0, 999.0).expect("finite"),
    );
    moved.transform = Transform::translate(999.0, 999.0)
        .expect("finite")
        .compose(&Transform::scale(2.0, 1.0).expect("finite"))
        .expect("finite");

    apply_batch_at_current(
        &mut document,
        &[
            Command::SetNodeProps {
                id: frame,
                props: Box::new(frame_props),
            },
            Command::SetNodeProps {
                id: first,
                props: Box::new(moved),
            },
            Command::SetNodeProps {
                id: second,
                props: Box::new(shaped(
                    (100.0, 30.0),
                    Transform::translate(5.0, 5.0).expect("finite"),
                )),
            },
        ],
    )
    .expect("properties should apply");

    let engine = TaffyLayoutEngine::new(Arc::new(ParleyTextEngine::new(pinned_fonts())));
    let snapshot = Snapshot::of(&document);
    let result = engine
        .layout(
            &snapshot,
            page,
            &LayoutOptions {
                page_size: Some((400.0, 400.0)),
            },
        )
        .expect("layout should succeed");

    let first_layout = result.node(first).expect("the first child");
    let second_layout = result.node(second).expect("the second child");
    assert_eq!(first_layout.offset, (20.0, 20.0));
    assert_eq!(
        second_layout.offset,
        (20.0, 60.0),
        "the second child starts after the first child's height plus the gap"
    );
    // Layout owns the box, the transform maps it: a scale does not change the
    // laid out size, and therefore does not move a sibling. That is the same
    // rule CSS transforms follow.
    assert_eq!(first_layout.size, (100.0, 30.0));
    assert_eq!(
        first_layout.rect.width, 200.0,
        "the reported page-space bounds include the transform"
    );
}

#[test]
fn a_fill_child_takes_the_free_space_along_the_main_axis() {
    let mut document = Document::new();
    let page = PageId::new();
    let frame = NodeId::new();
    let fixed = NodeId::new();
    let filling = NodeId::new();
    apply_batch_at_current(
        &mut document,
        &[
            Command::CreatePage {
                id: page,
                name: "Page 1".into(),
            },
            Command::CreateNode {
                id: frame,
                kind: NodeKind::Frame,
                name: None,
                parent: NodePlacement::PageRoot {
                    page,
                    position: Position::Last,
                },
            },
            Command::CreateNode {
                id: fixed,
                kind: NodeKind::Shape,
                name: None,
                parent: NodePlacement::Child {
                    parent: frame,
                    position: Position::Last,
                },
            },
            Command::CreateNode {
                id: filling,
                kind: NodeKind::Shape,
                name: None,
                parent: NodePlacement::Child {
                    parent: frame,
                    position: Position::Last,
                },
            },
        ],
    )
    .expect("the fixture should build");

    let mut frame_props = props(NodeKind::Frame);
    frame_props.size = Size::new(400.0, 100.0).expect("non-negative");
    frame_props.content = Content::Frame(FrameProps {
        layout: FrameLayout::Flex(FlexLayout {
            direction: Axis::Row,
            gap: Scalar::ZERO,
            padding: Insets::default(),
            main_align: MainAlign::Start,
            cross_align: CrossAlign::Start,
        }),
    });

    apply_batch_at_current(
        &mut document,
        &[Command::SetNodeProps {
            id: frame,
            props: Box::new(frame_props),
        }],
    )
    .expect("the frame should apply");

    let mut filling_props = shaped((0.0, 100.0), Transform::IDENTITY);
    filling_props.width_sizing = Sizing::Fill;
    apply_batch_at_current(
        &mut document,
        &[
            Command::SetNodeProps {
                id: fixed,
                props: Box::new(shaped((120.0, 100.0), Transform::IDENTITY)),
            },
            Command::SetNodeProps {
                id: filling,
                props: Box::new(filling_props),
            },
        ],
    )
    .expect("the children should apply");

    let engine = TaffyLayoutEngine::new(Arc::new(ParleyTextEngine::new(pinned_fonts())));
    let snapshot = Snapshot::of(&document);
    let result = engine
        .layout(
            &snapshot,
            page,
            &LayoutOptions {
                page_size: Some((500.0, 300.0)),
            },
        )
        .expect("layout should succeed");

    let fixed_layout = result.node(fixed).expect("the fixed child");
    let filling_layout = result.node(filling).expect("the filling child");
    assert_eq!(fixed_layout.size, (120.0, 100.0));
    assert_eq!(
        filling_layout.size.0, 280.0,
        "a fill child should take the space the fixed child left"
    );
    assert_eq!(filling_layout.offset.0, 120.0);
}

#[test]
fn text_hugs_its_measured_size_and_the_measurement_comes_from_the_text_engine() {
    let fixture = Fixture::new(&[(NodeKind::Text, text("Hello, SwotVibe!", LATIN_FAMILY, 24.0))]);
    let result = fixture.layout();
    let layout = result.node(fixture.roots()[0]).expect("a laid out node");

    let engine = ParleyTextEngine::new(pinned_fonts());
    let shaped = engine
        .shape(&swotvibe_text::TextRequest {
            text: "Hello, SwotVibe!",
            font_family: LATIN_FAMILY,
            font_size: 24.0,
            font_weight: 400,
            direction: TextDirection::Auto,
            align: TextAlign::Start,
            max_width: None,
        })
        .expect("shape");
    assert!(
        close(layout.size.0, shaped.width) && close(layout.size.1, shaped.height),
        "the laid out size {:?} should match the measured {:?}",
        layout.size,
        (shaped.width, shaped.height)
    );
}

#[test]
fn arabic_text_measures_from_the_arabic_face() {
    let fixture = Fixture::new(&[(NodeKind::Text, text("مرحبا بالعالم", ARABIC_FAMILY, 20.0))]);
    let result = fixture.layout();
    let layout = result.node(fixture.roots()[0]).expect("a laid out node");
    assert!(
        layout.size.0 > 0.0 && layout.size.1 > 0.0,
        "Arabic text should have a measured size, found {:?}",
        layout.size
    );
}

#[test]
fn an_unregistered_family_fails_the_pass_with_the_offending_node() {
    let fixture = Fixture::new(&[(NodeKind::Text, text("Hello", "Missing Family", 16.0))]);
    let snapshot = Snapshot::of(&fixture.document);
    let error = fixture
        .engine
        .layout(
            &snapshot,
            fixture.page,
            &LayoutOptions {
                page_size: Some((100.0, 100.0)),
            },
        )
        .expect_err("an unregistered family must fail the pass");

    match error {
        LayoutError::Text { node, message } => {
            assert_eq!(node, fixture.roots()[0]);
            assert!(message.contains("Missing Family"), "found `{message}`");
        }
        other => panic!("expected a text error, found {other:?}"),
    }
}

#[test]
fn a_hug_on_a_shape_is_reported_as_a_diagnostic() {
    let mut shape = shaped((40.0, 40.0), Transform::IDENTITY);
    shape.width_sizing = Sizing::Hug;
    let fixture = Fixture::new(&[(NodeKind::Shape, shape)]);
    let result = fixture.layout();

    assert_eq!(result.diagnostics().len(), 1);
    assert!(matches!(
        result.diagnostics()[0],
        LayoutDiagnostic::SizingFallback {
            kind: NodeKind::Shape,
            sizing: Sizing::Hug,
            ..
        }
    ));
    // The documented fallback is the stored size, not a collapsed box.
    assert_eq!(
        result.node(fixture.roots()[0]).expect("node").size,
        (40.0, 40.0)
    );
}

#[test]
fn a_page_that_is_not_in_the_snapshot_is_reported() {
    let fixture = Fixture::new(&[]);
    let snapshot = Snapshot::of(&fixture.document);
    let missing = PageId::new();
    assert_eq!(
        fixture
            .engine
            .layout(&snapshot, missing, &LayoutOptions::default())
            .expect_err("an unknown page must fail"),
        LayoutError::PageNotFound { page: missing }
    );
}

#[test]
fn a_non_positive_page_size_is_refused() {
    let fixture = Fixture::new(&[]);
    let snapshot = Snapshot::of(&fixture.document);
    assert!(matches!(
        fixture
            .engine
            .layout(
                &snapshot,
                fixture.page,
                &LayoutOptions {
                    page_size: Some((0.0, 10.0))
                }
            )
            .expect_err("a zero page size must fail"),
        LayoutError::InvalidPageSize { .. }
    ));
}

#[test]
fn the_report_is_ordered_by_page_structure_and_carries_a_fingerprint() {
    let fixture = Fixture::new(&[
        (NodeKind::Shape, shaped((10.0, 10.0), Transform::IDENTITY)),
        (NodeKind::Text, text("Hi", LATIN_FAMILY, 16.0)),
    ]);
    let result = fixture.layout();
    assert_eq!(result.order(), fixture.roots().as_slice());
    assert_eq!(result.nodes().count(), 2);

    let fingerprint = result.fingerprint();
    assert_eq!(fingerprint.engine, engine_id());
    assert!(!fingerprint.rounded);
    assert_eq!(fingerprint.fonts.len(), 2);
    assert!(result.page_size().0 >= 1000.0);
}

#[test]
fn the_sample_document_that_the_m0_gate_renders_lays_out_deterministically() {
    let dto = from_json(fixture_text("m0-sample-v2.json").as_bytes()).expect("the fixture parses");
    let document = import(&dto).expect("the fixture opens");
    let page = document.pages()[0].id;
    let engine = TaffyLayoutEngine::new(Arc::new(ParleyTextEngine::new(pinned_fonts())));
    let snapshot = Snapshot::of(&document);
    let options = LayoutOptions {
        page_size: Some((480.0, 320.0)),
    };

    let first = engine
        .layout(&snapshot, page, &options)
        .expect("the sample should lay out");
    let second = engine
        .layout(&snapshot, page, &options)
        .expect("the sample should lay out");

    assert_eq!(first, second, "two passes over one document must agree");
    assert_eq!(
        first.diagnostics(),
        [],
        "the sample should need no fallback"
    );

    let card = result_of(&document, &first, "Card");
    assert_eq!(card.size, (400.0, 240.0));
    assert_eq!(card.offset, (40.0, 40.0));

    // The card is a column with a 12-unit gap and 16-unit padding, so the
    // children stack: first at y = 16, then 48 + 12 = 76, then the two text
    // heights, then the dot.
    let header = result_of(&document, &first, "Header");
    let latin = result_of(&document, &first, "Latin");
    let arabic = result_of(&document, &first, "Arabic");
    let dot = result_of(&document, &first, "Dot");

    assert_eq!(header.offset, (16.0, 16.0));
    assert_eq!(header.size, (368.0, 48.0));
    assert_eq!(latin.offset.0, 16.0);
    assert_eq!(
        latin.offset.1, 76.0,
        "the second child starts after the header and the gap"
    );
    assert!(latin.size.0 > 0.0 && latin.size.1 > 0.0);
    assert!(
        arabic.offset.1 > latin.offset.1,
        "the Arabic line should follow the Latin one, found {:?} and {:?}",
        latin.offset,
        arabic.offset
    );
    assert!(dot.offset.1 > arabic.offset.1);
    assert_eq!(
        dot.offset.0, 16.0,
        "a flex child is placed by layout, so the dot's own translation is ignored"
    );
    assert!((dot.size.0 - 32.0).abs() < 0.01 && (dot.size.1 - 32.0).abs() < 0.01);
}

/// The laid out geometry of the node with the given name.
fn result_of(
    document: &Document,
    result: &swotvibe_layout::LayoutResult,
    name: &str,
) -> swotvibe_layout::NodeLayout {
    let id = node_named(document, name);
    *result
        .node(id)
        .unwrap_or_else(|| panic!("`{name}` should be laid out"))
}

#[test]
fn the_sample_card_children_are_laid_out_inside_the_card() {
    let dto = from_json(fixture_text("m0-sample-v2.json").as_bytes()).expect("the fixture parses");
    let document = import(&dto).expect("the fixture opens");
    let page = document.pages()[0].id;
    let engine = TaffyLayoutEngine::new(Arc::new(ParleyTextEngine::new(pinned_fonts())));
    let snapshot = Snapshot::of(&document);
    let result = engine
        .layout(
            &snapshot,
            page,
            &LayoutOptions {
                page_size: Some((480.0, 320.0)),
            },
        )
        .expect("the sample should lay out");

    let card = node_named(&document, "Card");
    let frame = result.node(card).expect("the card");
    for node in result.nodes() {
        if node.id == card {
            continue;
        }
        assert!(
            node.rect.x >= frame.rect.x - 0.01
                && node.rect.y >= frame.rect.y - 0.01
                && node.rect.right() <= frame.rect.right() + 0.01
                && node.rect.bottom() <= frame.rect.bottom() + 0.01,
            "a child at {:?} should sit inside the card at {:?}",
            node.rect,
            frame.rect
        );
    }
}

fn node_named(document: &Document, name: &str) -> NodeId {
    document
        .nodes()
        .find(|node| node.name() == name)
        .unwrap_or_else(|| panic!("the fixture should have a `{name}` node"))
        .id
}

#[test]
fn shape_geometry_and_paint_do_not_change_layout() {
    // Layout owns boxes; paint and shape geometry are the renderer's business.
    // A change there must not move anything, or a colour edit would invalidate
    // geometry caches for no reason.
    let mut plain = shaped((60.0, 40.0), Transform::IDENTITY);
    plain.content = Content::Shape(ShapeProps {
        geometry: ShapeGeometry::Rect,
        corner_radius: Scalar::ZERO,
    });
    let mut painted = plain.clone();
    painted.fill = Some(Color::rgb(1, 2, 3));
    painted.content = Content::Shape(ShapeProps {
        geometry: ShapeGeometry::Ellipse,
        corner_radius: Scalar::new(4.0).expect("finite"),
    });

    let a = Fixture::new(&[(NodeKind::Shape, plain)]).layout();
    let b = Fixture::new(&[(NodeKind::Shape, painted)]).layout();
    assert_eq!(
        a.node(a.order()[0]).expect("node").size,
        b.node(b.order()[0]).expect("node").size
    );
}
