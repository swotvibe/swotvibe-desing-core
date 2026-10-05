//! `SetNodeProps`: replacement semantics, validation, and undo round-trips.

use swotvibe_core::{
    Color, Command, CommandErrorCode, Content, DocumentEngine, FrameLayout, GeometryError, NodeId,
    NodeKind, NodePlacement, NodeProps, PageId, Position, Scalar, ShapeGeometry, ShapeProps, Size,
    Sizing, Stroke, Transform, validate,
};

fn page() -> PageId {
    PageId::new()
}

/// Creates a page plus one node of `kind` and returns `(engine, node)`.
fn engine_with(kind: NodeKind) -> (DocumentEngine, NodeId) {
    let mut engine = DocumentEngine::new();
    let page = page();
    let node = NodeId::new();
    engine
        .apply(&[
            Command::CreatePage {
                id: page,
                name: "Page".into(),
            },
            Command::CreateNode {
                id: node,
                kind,
                name: None,
                parent: NodePlacement::PageRoot {
                    page,
                    position: Position::Last,
                },
            },
        ])
        .expect("setup batch should apply");
    (engine, node)
}

fn props_of(engine: &DocumentEngine, node: NodeId) -> NodeProps {
    engine
        .document()
        .node(node)
        .expect("node should exist")
        .props
        .clone()
}

fn set(engine: &mut DocumentEngine, node: NodeId, props: NodeProps) {
    engine
        .apply(&[Command::SetNodeProps {
            id: node,
            props: Box::new(props),
        }])
        .expect("props should apply");
}

#[test]
fn a_new_node_starts_with_the_default_properties_of_its_kind() {
    for kind in [
        NodeKind::Frame,
        NodeKind::Group,
        NodeKind::Shape,
        NodeKind::Text,
        NodeKind::Image,
    ] {
        let (engine, node) = engine_with(kind);
        assert_eq!(props_of(&engine, node), NodeProps::default_for(kind));
    }
}

#[test]
fn replacing_properties_is_visible_on_the_document_and_validates() {
    let (mut engine, node) = engine_with(NodeKind::Shape);
    let mut props = NodeProps::default_for(NodeKind::Shape);
    props.transform = Transform::translate(12.0, 24.0).expect("finite");
    props.size = Size::new(120.0, 80.0).expect("non-negative");
    props.fill = Some(Color::rgba(10, 20, 30, 128));
    props.stroke = Some(Stroke {
        color: Color::rgb(0, 0, 0),
        width: Scalar::new(2.0).expect("finite"),
    });
    props.content = Content::Shape(ShapeProps {
        geometry: ShapeGeometry::Ellipse,
        corner_radius: Scalar::ZERO,
    });
    set(&mut engine, node, props.clone());

    assert_eq!(props_of(&engine, node), props);
    validate(engine.document(), swotvibe_core::ResourceLimits::UNLIMITED)
        .expect("the document should stay valid");
}

#[test]
fn a_property_change_marks_the_node_as_changed_exactly_once() {
    let (mut engine, node) = engine_with(NodeKind::Text);
    let commit = engine
        .apply(&[Command::SetNodeProps {
            id: node,
            props: Box::new(NodeProps::default_for(NodeKind::Text)),
        }])
        .expect("an identical value is still a legal command");

    // The value equals the previous one, so the diff sees no content change.
    assert!(!commit.change_set().changed_nodes().any(|id| id == node));
    assert_eq!(commit.effects().len(), 1);
}

#[test]
fn undo_restores_the_previous_properties_and_redo_reapplies_them() {
    let (mut engine, node) = engine_with(NodeKind::Shape);
    let before = props_of(&engine, node);

    let mut after = before.clone();
    after.transform = Transform::scale(3.0, 3.0).expect("finite");
    after.width_sizing = Sizing::Fill;
    set(&mut engine, node, after.clone());
    assert_eq!(props_of(&engine, node), after);

    engine.undo().expect("undo should apply");
    assert_eq!(props_of(&engine, node), before);
    assert!(engine.can_redo());

    engine.redo().expect("redo should apply");
    assert_eq!(props_of(&engine, node), after);

    engine.undo().expect("undo should apply");
    assert_eq!(props_of(&engine, node), before);
}

#[test]
fn undo_restores_properties_of_a_recreated_node() {
    let (mut engine, node) = engine_with(NodeKind::Text);
    let mut props = NodeProps::default_for(NodeKind::Text);
    props.size = Size::new(200.0, 40.0).expect("non-negative");
    props.fill = Some(Color::rgb(1, 2, 3));
    if let Content::Text(text) = &mut props.content {
        text.content = "hello".into();
        text.font_size = Scalar::new(24.0).expect("finite");
    }
    set(&mut engine, node, props.clone());

    // Delete the node, then undo: recreating it must restore the properties,
    // not just the kind and name.
    engine
        .apply(&[Command::DeleteNode { id: node }])
        .expect("delete should apply");
    assert!(engine.document().node(node).is_none());

    engine.undo().expect("undo should apply");
    assert_eq!(props_of(&engine, node), props);
}

#[test]
fn a_mismatched_content_variant_is_rejected_without_touching_the_document() {
    let (mut engine, node) = engine_with(NodeKind::Shape);
    let before = props_of(&engine, node);
    let revision = engine.revision();
    let undo_depth = engine.history().undo_depth();

    let error = engine
        .apply(&[Command::SetNodeProps {
            id: node,
            props: Box::new(NodeProps::default_for(NodeKind::Text)),
        }])
        .expect_err("a text content on a shape must fail");

    assert_eq!(error.code(), Some(CommandErrorCode::InvalidProps));
    assert_eq!(props_of(&engine, node), before);
    assert_eq!(engine.revision(), revision);
    assert_eq!(engine.history().undo_depth(), undo_depth);
}

#[test]
fn a_negative_length_is_rejected() {
    let (mut engine, node) = engine_with(NodeKind::Frame);
    let mut props = NodeProps::default_for(NodeKind::Frame);
    if let Content::Frame(frame) = &mut props.content {
        frame.layout = FrameLayout::None;
    }
    props.stroke = Some(Stroke {
        color: Color::rgb(0, 0, 0),
        width: Scalar::new(-1.0).expect("finite"),
    });

    let error = engine
        .apply(&[Command::SetNodeProps {
            id: node,
            props: Box::new(props),
        }])
        .expect_err("a negative stroke width must fail");
    assert_eq!(error.code(), Some(CommandErrorCode::InvalidProps));
}

#[test]
fn an_image_referring_to_a_missing_asset_is_rejected() {
    let (mut engine, node) = engine_with(NodeKind::Image);
    let mut props = NodeProps::default_for(NodeKind::Image);
    props.content = Content::Image(swotvibe_core::props::ImageProps {
        asset: Some(swotvibe_core::AssetId::new()),
    });

    let error = engine
        .apply(&[Command::SetNodeProps {
            id: node,
            props: Box::new(props),
        }])
        .expect_err("a dangling asset reference must fail");
    assert_eq!(error.code(), Some(CommandErrorCode::AssetNotFound));
}

#[test]
fn a_batch_that_fails_after_a_property_change_leaves_no_trace() {
    let (mut engine, node) = engine_with(NodeKind::Shape);
    let before = props_of(&engine, node);
    let revision = engine.revision();

    let mut props = before.clone();
    props.fill = Some(Color::rgb(9, 9, 9));
    let error = engine
        .apply(&[
            Command::SetNodeProps {
                id: node,
                props: Box::new(props),
            },
            Command::DeleteNode { id: NodeId::new() },
        ])
        .expect_err("the batch must fail as a whole");

    assert_eq!(error.code(), Some(CommandErrorCode::NodeNotFound));
    assert_eq!(props_of(&engine, node), before);
    assert_eq!(engine.revision(), revision);
}

#[test]
fn a_non_finite_transform_cannot_be_constructed() {
    assert_eq!(
        Transform::translate(f64::NAN, 0.0),
        Err(GeometryError::NotFinite)
    );
    assert_eq!(Size::new(-1.0, 1.0), Err(GeometryError::Negative));
}
