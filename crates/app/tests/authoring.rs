#[path = "support/authoring.rs"]
mod support;
use support::*;
use swotvibe_app::{AppErrorCode, EditCommand, EditRequest};

#[test]
fn text_can_be_created_styled_and_edited_in_one_atomic_batch() {
    let mut session = blank();
    let commands = vec![
        create(&session, "text"),
        EditCommand::SetNodeText {
            node: NODE.into(),
            content: "عرض جديد — SALE".into(),
            font_size: 28.,
        },
        EditCommand::SetNodeSize {
            node: NODE.into(),
            size: [400., 100.],
        },
        EditCommand::SetNodePosition {
            node: NODE.into(),
            position: [12., 18.],
        },
        style(),
    ];
    session.apply(&request(&session, commands)).unwrap();
    let props = session.node_props(NODE).unwrap();
    assert_eq!(props.text_content.as_deref(), Some("عرض جديد — SALE"));
    assert_eq!(props.font_family.as_deref(), Some("Noto Sans Arabic"));
    assert_eq!(props.font_weight, Some(900));
    assert_eq!(props.text_align.as_deref(), Some("center"));
    assert_eq!(props.font_size, Some(28.));
    assert_eq!(props.transform[4..], [12., 18.]);
    let pixels = preview(&session, [440., 140.]);
    assert!(pixels.render_diagnostics.is_empty());
    assert!(pixels.layout_diagnostics.is_empty());
    session.undo(session.revision().as_u64()).unwrap();
    assert!(session.view().unwrap().nodes.is_empty());
    assert!(!session.is_dirty());
    session.redo(session.revision().as_u64()).unwrap();
    assert_eq!(session.node_props(NODE).unwrap(), props);

    // Style changes preserve content, size, paint, transform and sizing modes.
    let mut command = style();
    if let EditCommand::SetNodeTextStyle {
        font_family,
        font_weight,
        align,
        ..
    } = &mut command
    {
        *font_family = "Inter".into();
        *font_weight = 500;
        *align = "end".into();
    }
    session.apply(&request(&session, vec![command])).unwrap();
    let after = session.node_props(NODE).unwrap();
    assert_eq!(after.text_content, props.text_content);
    assert_eq!(after.font_size, props.font_size);
    assert_eq!(after.size, props.size);
    assert_eq!(after.transform, props.transform);
    assert_eq!(after.fill, props.fill);
    assert_eq!(after.width_sizing, props.width_sizing);
    assert_eq!(after.text_align.as_deref(), Some("end"));
    session.undo(session.revision().as_u64()).unwrap();
    assert_eq!(session.node_props(NODE).unwrap(), props);
}

#[test]
fn late_invalid_style_leaves_scene_revision_dirty_and_history_unchanged() {
    let cases = [
        ("Inter", 0, "start", "auto", AppErrorCode::InvalidRequest),
        ("Inter", 1001, "start", "auto", AppErrorCode::InvalidRequest),
        (
            "Unknown font",
            400,
            "start",
            "auto",
            AppErrorCode::InvalidRequest,
        ),
        (
            "Inter",
            400,
            "justify",
            "auto",
            AppErrorCode::InvalidRequest,
        ),
        (
            "Inter",
            400,
            "start",
            "sideways",
            AppErrorCode::InvalidRequest,
        ),
        ("Inter", 400, "start", "rtl", AppErrorCode::CommandRejected),
        ("Inter", 400, "start", "ltr", AppErrorCode::CommandRejected),
    ];
    for (family, weight, align, direction, code) in cases {
        let mut session = blank();
        let before = session.view().unwrap();
        let bytes = session.export_bytes().unwrap();
        let error = session
            .apply(&request(
                &session,
                vec![
                    create(&session, "text"),
                    EditCommand::SetNodeTextStyle {
                        node: NODE.into(),
                        font_family: family.into(),
                        font_weight: weight,
                        align: align.into(),
                        direction: direction.into(),
                    },
                ],
            ))
            .unwrap_err();
        assert_eq!(error.code, code);
        assert_eq!(error.node.as_deref(), Some(NODE));
        assert_eq!(session.view().unwrap(), before);
        assert_eq!(session.export_bytes().unwrap(), bytes);
    }
}

#[test]
fn image_binding_is_atomic_reversible_and_keeps_resources_across_clear_and_save() {
    let bundle = image_seed();
    let mut session = open_bundle(&bundle);
    session
        .apply(&request(
            &session,
            vec![
                create(&session, "image"),
                EditCommand::SetNodeImageAsset {
                    node: NODE.into(),
                    asset: Some(ASSET.into()),
                },
                EditCommand::SetNodeSize {
                    node: NODE.into(),
                    size: [80., 40.],
                },
            ],
        ))
        .unwrap();
    let image = swotvibe_render::RenderedImage::from_png(
        &preview(&session, [80., 40.]).png,
        render_limits().png,
    )
    .unwrap();
    assert_eq!(image.pixel(5, 20), Some([255, 0, 0, 255]));
    assert_eq!(image.pixel(75, 20), Some([0, 0, 255, 255]));
    session
        .apply(&request(
            &session,
            vec![EditCommand::SetNodeImageAsset {
                node: NODE.into(),
                asset: None,
            }],
        ))
        .unwrap();
    assert_eq!(session.node_props(NODE).unwrap().image_asset, None);
    session.undo(session.revision().as_u64()).unwrap();
    assert_eq!(
        session.node_props(NODE).unwrap().image_asset.as_deref(),
        Some(ASSET)
    );
    session.redo(session.revision().as_u64()).unwrap();
    let saved = swotvibe_format::unpack_bundle(&session.export_bytes().unwrap(), limits()).unwrap();
    assert_eq!(saved.assets, bundle.assets);
    let mut reopened = open_bundle(&saved);
    reopened
        .apply(&request(
            &reopened,
            vec![EditCommand::SetNodeImageAsset {
                node: NODE.into(),
                asset: Some(ASSET.into()),
            }],
        ))
        .unwrap();
    assert_eq!(preview(&reopened, [80., 40.]).png, image.to_png().unwrap());
}

#[test]
fn invalid_or_unavailable_assets_and_wrong_node_types_are_rejected_atomically() {
    for asset in [
        "bad-identity",
        "018f0000-0000-7000-8000-000000000099",
        ASSET,
    ] {
        let bundle = image_seed();
        let mut session = if asset == ASSET {
            let mut editor = blank();
            editor
                .open_json(&swotvibe_format::to_json(&bundle.document).unwrap(), None)
                .unwrap();
            editor
        } else {
            open_bundle(&bundle)
        };
        let before = session.view().unwrap();
        let bytes = session.export_bytes().unwrap();
        let error = session
            .apply(&request(
                &session,
                vec![
                    create(&session, "image"),
                    EditCommand::SetNodeImageAsset {
                        node: NODE.into(),
                        asset: Some(asset.into()),
                    },
                ],
            ))
            .unwrap_err();
        assert_eq!(
            error.code,
            if asset == "bad-identity" {
                AppErrorCode::InvalidRequest
            } else {
                AppErrorCode::CommandRejected
            }
        );
        assert_eq!(session.view().unwrap(), before);
        assert_eq!(session.export_bytes().unwrap(), bytes);
    }
    for command in [
        style(),
        EditCommand::SetNodeImageAsset {
            node: NODE.into(),
            asset: None,
        },
    ] {
        let mut session = blank();
        let before = session.view().unwrap();
        assert_eq!(
            session
                .apply(&request(&session, vec![create(&session, "shape"), command]))
                .unwrap_err()
                .code,
            AppErrorCode::CommandRejected
        );
        assert_eq!(session.view().unwrap(), before);
    }
}

#[test]
fn stale_authoring_request_preserves_an_intervening_edit() {
    let mut session = blank();
    session
        .apply(&request(&session, vec![create(&session, "text")]))
        .unwrap();
    let stale = request(&session, vec![style()]);
    session
        .apply(&request(
            &session,
            vec![EditCommand::SetNodeText {
                node: NODE.into(),
                content: "تعديل المستخدم".into(),
                font_size: 24.,
            }],
        ))
        .unwrap();
    let before = session.view().unwrap();
    assert_eq!(
        session.apply(&stale).unwrap_err().code,
        AppErrorCode::RevisionConflict
    );
    assert_eq!(session.view().unwrap(), before);
}

#[test]
fn authoring_contract_rejects_missing_unknown_or_mistyped_fields() {
    let base = serde_json::json!({"expectedRevision":1,"commands":[{"kind":"set-node-text-style","node":NODE,"fontFamily":"Inter","fontWeight":700,"align":"center","direction":"auto"}]});
    let parsed: EditRequest = serde_json::from_value(base.clone()).unwrap();
    assert_eq!(serde_json::to_value(&parsed).unwrap(), base);
    for field in ["node", "fontFamily", "fontWeight", "align", "direction"] {
        let mut value = base.clone();
        value["commands"][0].as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<EditRequest>(value).is_err(),
            "{field}"
        );
    }
    for weight in [
        serde_json::json!(-1),
        serde_json::json!(1.5),
        serde_json::json!("700"),
    ] {
        let mut value = base.clone();
        value["commands"][0]["fontWeight"] = weight;
        assert!(serde_json::from_value::<EditRequest>(value).is_err());
    }
    let mut extra = base;
    extra["commands"][0]["fontWeigth"] = serde_json::json!(400);
    assert!(serde_json::from_value::<EditRequest>(extra).is_err());
    for asset in [serde_json::json!(null), serde_json::json!(ASSET)] {
        let value = serde_json::json!({"expectedRevision":1,"commands":[{"kind":"set-node-image-asset","node":NODE,"asset":asset}]});
        let request: EditRequest = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(request).unwrap(), value);
    }
    assert!(serde_json::from_value::<EditRequest>(serde_json::json!({"expectedRevision":1,"commands":[{"kind":"set-node-image-asset","node":NODE}]})).is_err());
}

#[test]
fn capabilities_report_authoring_commands_and_actual_text_limits() {
    let session = blank();
    let capabilities = serde_json::to_value(session.capabilities()).unwrap();
    assert!(
        capabilities["editableCommands"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("set-node-text-style"))
    );
    assert!(
        capabilities["editableCommands"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("set-node-image-asset"))
    );
    assert_eq!(capabilities["textDirections"], serde_json::json!(["auto"]));
    assert_eq!(
        capabilities["textAlignments"],
        serde_json::json!(["start", "center", "end"])
    );
    assert_eq!(
        capabilities["fontWeightRange"],
        serde_json::json!([1, 1000])
    );
}
