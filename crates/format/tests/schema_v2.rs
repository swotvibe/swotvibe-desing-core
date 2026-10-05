//! Schema v2: node properties, the v1 → v2 migration, and its failure modes.

use std::path::PathBuf;

use swotvibe_core::{
    Color, Command, Content, Document, FlexLayout, FrameLayout, Insets, NodeId, NodeKind,
    NodePlacement, NodeProps, PageId, Position, Scalar, ShapeGeometry, ShapeProps, Size, Sizing,
    Stroke, TextAlign, TextDirection, Transform, apply_batch_at_current,
};
use swotvibe_format::{
    ImportError, OLDEST_SUPPORTED_VERSION, SCHEMA_VERSION, from_json, import, import_migrated,
    migrate_to_current, needs_migration, to_json,
};

/// The committed v1 document, which carries no properties at all.
fn v1_fixture_path() -> PathBuf {
    fixture_path("document-v1-minimal.json")
}

/// The committed v2 document, with explicit properties on every kind.
fn v2_fixture_path() -> PathBuf {
    fixture_path("document-v2-styled.json")
}

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn v1_bytes() -> Vec<u8> {
    read_bytes(&v1_fixture_path())
}

fn read_bytes(path: &std::path::Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()))
}

/// Builds a small document with non-default properties on every kind.
fn styled_document() -> (Document, NodeId, NodeId, NodeId, NodeId) {
    let mut doc = Document::new();
    let page = PageId::new();
    let frame = NodeId::new();
    let shape = NodeId::new();
    let text = NodeId::new();
    let image = NodeId::new();

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
                name: Some("Card".into()),
                parent: NodePlacement::PageRoot {
                    page,
                    position: Position::Last,
                },
            },
            Command::CreateNode {
                id: shape,
                kind: NodeKind::Shape,
                name: Some("Background".into()),
                parent: NodePlacement::Child {
                    parent: frame,
                    position: Position::Last,
                },
            },
            Command::CreateNode {
                id: text,
                kind: NodeKind::Text,
                name: Some("Label".into()),
                parent: NodePlacement::Child {
                    parent: frame,
                    position: Position::Last,
                },
            },
        ],
    )
    .expect("the structural batch should apply");

    let mut frame_props = NodeProps::default_for(NodeKind::Frame);
    frame_props.size = Size::new(400.0, 240.0).expect("non-negative");
    frame_props.width_sizing = Sizing::Fixed;
    frame_props.height_sizing = Sizing::Fixed;
    frame_props.fill = Some(Color::rgb(250, 250, 252));
    frame_props.content = Content::Frame(swotvibe_core::FrameProps {
        layout: FrameLayout::Flex(FlexLayout {
            direction: swotvibe_core::Axis::Column,
            gap: Scalar::new(8.0).expect("finite"),
            padding: Insets::uniform(Scalar::new(16.0).expect("finite")),
            main_align: swotvibe_core::MainAlign::SpaceBetween,
            cross_align: swotvibe_core::CrossAlign::Stretch,
        }),
    });

    let mut shape_props = NodeProps::default_for(NodeKind::Shape);
    shape_props.transform = Transform::translate(24.0, 32.0)
        .expect("finite")
        .compose(&Transform::scale(2.0, 0.5).expect("finite"))
        .expect("finite");
    shape_props.size = Size::new(320.0, 160.0).expect("non-negative");
    shape_props.fill = Some(Color::rgba(20, 40, 60, 128));
    shape_props.stroke = Some(Stroke {
        color: Color::rgb(0, 0, 0),
        width: Scalar::new(1.5).expect("finite"),
    });
    shape_props.content = Content::Shape(ShapeProps {
        geometry: ShapeGeometry::Ellipse,
        corner_radius: Scalar::new(6.0).expect("finite"),
    });

    let mut text_props = NodeProps::default_for(NodeKind::Text);
    text_props.size = Size::new(180.0, 24.0).expect("non-negative");
    text_props.width_sizing = Sizing::Hug;
    text_props.height_sizing = Sizing::Hug;
    if let Content::Text(text_content) = &mut text_props.content {
        text_content.content = "مرحبا بالعالم".into();
        text_content.font_family = "Noto Sans Arabic".into();
        text_content.font_size = Scalar::new(18.0).expect("finite");
        text_content.font_weight = 700;
        text_content.direction = TextDirection::Rtl;
        text_content.align = TextAlign::End;
    }

    apply_batch_at_current(
        &mut doc,
        &[
            Command::SetNodeProps {
                id: frame,
                props: Box::new(frame_props),
            },
            Command::SetNodeProps {
                id: shape,
                props: Box::new(shape_props),
            },
            Command::SetNodeProps {
                id: text,
                props: Box::new(text_props),
            },
        ],
    )
    .expect("the property batch should apply");

    (doc, frame, shape, text, image)
}

#[test]
fn the_styled_v2_fixture_opens_with_every_property_intact() {
    let dto = from_json(&read_bytes(&v2_fixture_path())).expect("the v2 fixture should parse");
    assert_eq!(dto.schema_version, SCHEMA_VERSION);
    assert!(!needs_migration(&dto));

    let document = import(&dto).expect("the v2 fixture should open");
    let by_name = |name: &str| {
        document
            .nodes()
            .find(|node| node.name() == name)
            .unwrap_or_else(|| panic!("the fixture should have a `{name}` node"))
    };

    let frame = by_name("Card");
    assert_eq!(frame.props.size.width(), 400.0);
    assert_eq!(frame.props.fill, Some(Color::rgb(250, 250, 252)));
    let Content::Frame(frame_content) = &frame.props.content else {
        panic!("the frame should carry frame properties");
    };
    let FrameLayout::Flex(flex) = frame_content.layout else {
        panic!("the frame should use flex layout");
    };
    assert_eq!(flex.gap, Scalar::new(8.0).expect("finite"));
    assert_eq!(
        flex.padding,
        Insets::uniform(Scalar::new(16.0).expect("finite"))
    );

    let shape = by_name("Background");
    assert_eq!(
        shape.props.transform.coefficients(),
        [2.0, 0.0, 0.0, 0.5, 24.0, 32.0]
    );
    assert_eq!(shape.props.width_sizing, Sizing::Fill);
    assert_eq!(shape.props.fill, Some(Color::rgba(20, 40, 60, 128)));
    assert_eq!(
        shape.props.stroke.map(|stroke| stroke.width.get()),
        Some(1.5)
    );
    let Content::Shape(shape_content) = &shape.props.content else {
        panic!("the shape should carry shape properties");
    };
    assert_eq!(shape_content.geometry, ShapeGeometry::Ellipse);
    assert_eq!(
        shape_content.corner_radius,
        Scalar::new(6.0).expect("finite")
    );

    let text = by_name("Label");
    let Content::Text(text_content) = &text.props.content else {
        panic!("the text node should carry text properties");
    };
    assert_eq!(text_content.content, "مرحبا بالعالم");
    assert_eq!(text_content.font_family, "Noto Sans Arabic");
    assert_eq!(text_content.direction, TextDirection::Rtl);
    assert_eq!(text_content.align, TextAlign::End);

    let image = by_name("Logo");
    let Content::Image(image_content) = &image.props.content else {
        panic!("the image node should carry image properties");
    };
    assert!(
        image_content.asset.is_some(),
        "the image should reference the declared asset"
    );
}

#[test]
fn an_exported_v2_document_re_opens_with_the_same_properties() {
    let dto = from_json(&read_bytes(&v2_fixture_path())).expect("parse");
    let once = import(&dto).expect("open");
    let again = import(
        &from_json(&to_json(&swotvibe_format::export(&once)).expect("save")).expect("parse"),
    )
    .expect("re-open");

    assert_eq!(once.node_count(), again.node_count());
    for node in once.nodes() {
        let reopened = again
            .node(node.id)
            .expect("the node survives the round trip");
        assert_eq!(reopened.props, node.props);
        assert_eq!(reopened.kind, node.kind);
        assert_eq!(reopened.name, node.name);
    }
}

#[test]
fn the_current_schema_is_two_and_v1_is_still_readable() {
    assert_eq!(SCHEMA_VERSION, 2);
    assert_eq!(OLDEST_SUPPORTED_VERSION, 1);
}

#[test]
fn a_v1_file_needs_migration_and_opens_with_default_properties() {
    let bytes = v1_bytes();
    let dto = from_json(&bytes).expect("the v1 fixture should parse");
    assert_eq!(dto.schema_version, 1);
    assert!(needs_migration(&dto));

    let document = import_migrated(dto).expect("a v1 file should open");
    for node in document.nodes() {
        assert_eq!(
            node.props,
            NodeProps::default_for(node.kind),
            "a v1 node keeps the defaults of its kind"
        );
    }
    assert_eq!(document.node_count(), 3);
}

#[test]
fn migrating_a_v1_file_bumps_the_version_and_preserves_structure() {
    let dto = from_json(&v1_bytes()).expect("the v1 fixture should parse");
    let migrated = migrate_to_current(dto).expect("v1 should migrate");
    assert_eq!(migrated.schema_version, SCHEMA_VERSION);
    assert!(!needs_migration(&migrated));

    // Unknown fields the file carried stay preserved through the migration.
    let frame = migrated
        .nodes
        .iter()
        .find(|node| node.kind == "frame")
        .expect("the fixture has a frame");
    assert!(frame.extensions.contains_key("futureFrameField"));
}

#[test]
fn a_v1_file_saved_again_is_written_as_v2_with_explicit_properties() {
    let document =
        import_migrated(from_json(&v1_bytes()).expect("parse")).expect("a v1 file should open");
    let written = to_json(&swotvibe_format::export(&document)).expect("the document should save");
    let text = String::from_utf8(written).expect("the output is UTF-8");

    assert!(text.contains("\"schema_version\": 2"));
    assert!(text.contains("\"props\""));
    // The preserved unknown field survives the whole round trip.
    assert!(text.contains("futureFrameField"));
}

#[test]
fn properties_round_trip_through_the_json_content_codec() {
    let (doc, frame, shape, text, _) = styled_document();
    let bytes = to_json(&swotvibe_format::export(&doc)).expect("the document should save");
    let dto = from_json(&bytes).expect("the saved document should parse");
    let reopened = import(&dto).expect("the saved document should open");

    assert_eq!(
        reopened.node(frame).expect("frame").props,
        doc.node(frame).expect("frame").props
    );
    assert_eq!(
        reopened.node(shape).expect("shape").props,
        doc.node(shape).expect("shape").props
    );
    assert_eq!(
        reopened.node(text).expect("text").props,
        doc.node(text).expect("text").props
    );
    assert_eq!(
        reopened.node(text).expect("text").name,
        doc.node(text).expect("text").name
    );
}

#[test]
fn a_v2_file_without_a_props_record_still_opens_with_defaults() {
    let (doc, _, _, _, _) = styled_document();
    let mut dto = swotvibe_format::export(&doc);
    for node in &mut dto.nodes {
        node.props = None;
    }
    let reopened = import(&dto).expect("absent properties mean their defaults");
    for node in reopened.nodes() {
        assert_eq!(node.props, NodeProps::default_for(node.kind));
    }
}

#[test]
fn a_record_for_another_kind_is_rejected_rather_than_ignored() {
    let (doc, _, _, _, _) = styled_document();
    let mut dto = swotvibe_format::export(&doc);
    let shape = dto
        .nodes
        .iter_mut()
        .find(|node| node.kind == "shape")
        .expect("the document has a shape");
    // A shape carrying text properties is a file that describes two different
    // things at once; the difference must not disappear silently.
    if let Some(props) = shape.props.as_mut() {
        props.text = Some(swotvibe_format::DtoTextProps {
            content: "wrong".into(),
            font_family: "sans-serif".into(),
            font_size: 12.0,
            font_weight: 400,
            direction: "auto".into(),
            align: "start".into(),
            extensions: Default::default(),
        });
    }

    let error = import(&dto).expect_err("a mismatched record must fail");
    assert!(
        matches!(error, ImportError::InvalidProps { .. }),
        "expected an InvalidProps error, found {error:?}"
    );
}

#[test]
fn an_unknown_vocabulary_name_is_rejected() {
    let (doc, _, _, _, _) = styled_document();
    let mut dto = swotvibe_format::export(&doc);
    let shape = dto
        .nodes
        .iter_mut()
        .find(|node| node.kind == "shape")
        .expect("the document has a shape");
    if let Some(props) = shape.props.as_mut() {
        props.width = "enormous".into();
    }
    let error = import(&dto).expect_err("an unknown name must fail");
    assert!(
        matches!(error, ImportError::InvalidProps { .. }),
        "expected an InvalidProps error, found {error:?}"
    );
}

#[test]
fn a_non_finite_transform_is_rejected() {
    let (doc, _, _, _, _) = styled_document();
    let mut dto = swotvibe_format::export(&doc);
    let shape = dto
        .nodes
        .iter_mut()
        .find(|node| node.kind == "shape")
        .expect("the document has a shape");
    if let Some(props) = shape.props.as_mut() {
        props.transform[0] = f64::NAN;
    }
    assert!(matches!(
        import(&dto),
        Err(ImportError::InvalidProps { .. })
    ));
}

#[test]
fn an_image_pointing_at_a_missing_asset_is_rejected() {
    let (doc, _, _, _, _) = styled_document();
    let mut dto = swotvibe_format::export(&doc);
    let shape = dto
        .nodes
        .iter_mut()
        .find(|node| node.kind == "shape")
        .expect("the document has a shape");
    if let Some(props) = shape.props.as_mut() {
        props.shape = None;
        props.image = Some(swotvibe_format::DtoImageProps {
            asset: Some(swotvibe_core::AssetId::new().to_string()),
            extensions: Default::default(),
        });
    }
    // The kind is still "shape", so the mismatch fires before the asset check.
    assert!(matches!(
        import(&dto),
        Err(ImportError::InvalidProps { .. })
    ));
}

#[test]
fn a_future_schema_version_is_still_refused() {
    let dto = from_json(&v1_bytes()).expect("parse");
    let mut dto = migrate_to_current(dto).expect("migrate");
    dto.schema_version = SCHEMA_VERSION + 1;
    assert!(matches!(
        import(&dto),
        Err(ImportError::UnsupportedFutureVersion { .. })
    ));
}

#[test]
fn a_version_below_the_floor_is_refused() {
    let mut dto = from_json(&v1_bytes()).expect("parse");
    dto.schema_version = 0;
    assert!(matches!(
        import(&dto),
        Err(ImportError::UnsupportedPastVersion { .. })
    ));
}

#[test]
fn unknown_keys_inside_a_property_record_survive_a_save() {
    let (doc, _, _, _, _) = styled_document();
    let mut dto = swotvibe_format::export(&doc);
    let shape = dto
        .nodes
        .iter_mut()
        .find(|node| node.kind == "shape")
        .expect("the document has a shape");
    if let Some(props) = shape.props.as_mut() {
        props
            .extensions
            .insert("futurePropsField".into(), serde_json::json!(7));
    }

    let bytes = to_json(&dto).expect("the document should save");
    let text = String::from_utf8(bytes).expect("the output is UTF-8");
    assert!(text.contains("futurePropsField"));

    let reopened = from_json(text.as_bytes()).expect("the saved document should parse");
    let shape = reopened
        .nodes
        .iter()
        .find(|node| node.kind == "shape")
        .expect("the document has a shape");
    assert!(
        shape
            .props
            .as_ref()
            .expect("a property record")
            .extensions
            .contains_key("futurePropsField")
    );
}
