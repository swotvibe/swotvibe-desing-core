//! Frozen acceptance case: the scene is built exclusively through EditRequest.
//! The seed has resources and an empty page; the reference scene is read only
//! after authoring for comparison, never to construct the output document.
#[path = "support/authoring.rs"]
mod support;
use std::{collections::BTreeMap, fs};
use support::*;
use swotvibe_app::{EditCommand, EditorSession};
use swotvibe_format::{Bundle, export, from_json, import, pack_bundle, unpack_bundle};
use swotvibe_render::RenderedImage;

#[test]
fn commands_author_the_layered_poster_and_survive_undo_redo_save_and_reopen() {
    let fixture = root().join("tests/fixtures/native-authoring");
    let seed = from_json(&fs::read(fixture.join("seed.json")).unwrap()).unwrap();
    assert!(seed.nodes.is_empty());
    assert_eq!(seed.pages.len(), 1);
    assert!(seed.pages[0].roots.is_empty());
    let mut resources = BTreeMap::new();
    for asset in &seed.assets {
        resources.insert(
            asset.id.parse().unwrap(),
            fs::read(fixture.join("assets").join(&asset.id)).unwrap(),
        );
    }
    let mut session = open_bundle(&Bundle {
        document: seed,
        assets: resources.clone(),
    });
    let baseline = session.export_bytes().unwrap();
    let commands: Vec<EditCommand> =
        serde_json::from_slice(&fs::read(fixture.join("commands.json")).unwrap()).unwrap();
    assert_eq!(
        commands
            .iter()
            .filter(|command| matches!(command, EditCommand::CreateNode { .. }))
            .count(),
        22
    );
    // Exercise the same wire serialization/deserialization as an external client.
    let wire = serde_json::to_vec(&request(&session, commands)).unwrap();
    session
        .apply(&serde_json::from_slice(&wire).unwrap())
        .unwrap();
    let view = session.view().unwrap();
    assert_eq!(view.nodes.len(), 22);
    assert_eq!(
        view.nodes.iter().filter(|node| node.kind == "text").count(),
        7
    );
    assert_eq!(view.history.undo_depth, 1);
    let preview = preview(&session, [1080., 1350.]);
    assert_eq!((preview.width, preview.height), (1080, 1350));
    assert!(preview.layout_diagnostics.is_empty());
    assert!(preview.render_diagnostics.is_empty());
    let reference = RenderedImage::from_png(
        &fs::read(fixture.join("reference.png")).unwrap(),
        render_limits().png,
    )
    .unwrap();
    let actual = RenderedImage::from_png(&preview.png, render_limits().png).unwrap();
    let difference = actual.compare(&reference, 2);
    assert!(!difference.size_mismatch);
    assert_eq!(
        difference.deviating_pixels, 0,
        "authored poster changed: {difference:?}"
    );
    let authored = session.export_bytes().unwrap();
    let bundle = unpack_bundle(&authored, limits()).unwrap();
    assert_eq!(bundle.assets, resources);
    // Normalize only the reference's schema defaults using ordinary import/export.
    let expected = export(
        &import(&from_json(&fs::read(fixture.join("reference-scene.json")).unwrap()).unwrap())
            .unwrap(),
    );
    assert_eq!(bundle.document, expected);
    session.undo(session.revision().as_u64()).unwrap();
    assert!(session.view().unwrap().nodes.is_empty());
    assert!(!session.is_dirty());
    assert_eq!(session.export_bytes().unwrap(), baseline);
    session.redo(session.revision().as_u64()).unwrap();
    assert_eq!(session.export_bytes().unwrap(), authored);
    let output = root().join("tests/golden/native-authoring/out");
    fs::create_dir_all(&output).unwrap();
    let file = output.join("GTA-VI-Authored.swv");
    fs::write(&file, &authored).unwrap();
    session.note_saved().unwrap();
    assert!(!session.is_dirty());
    let mut reopened = EditorSession::new(fonts());
    reopened
        .open_document(
            &fs::read(&file).unwrap(),
            Some("GTA-VI-Authored.swv".into()),
            limits(),
            render_limits(),
        )
        .unwrap();
    assert_eq!(reopened.view().unwrap().nodes, view.nodes);
    let reopened_preview = support::preview(&reopened, [1080., 1350.]);
    assert_eq!(reopened_preview.png, preview.png);
    fs::write(output.join("preview.png"), &reopened_preview.png).unwrap();
    let report = serde_json::json!({"authored_via":"EditorSession::apply, starting with an empty page and prepared assets",
        "layers":22,"editable_text_layers":7,"resources":7,"commands":109,"undo_steps":1,
        "pixel_tolerance":2,"deviating_pixels":difference.deviating_pixels,
        "scene_matches_reference":true,"undo_redo":true,"save_reopen_identical":true});
    fs::write(
        output.join("verification.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    // Packing remains the standard native format, not a second authoring format.
    assert_eq!(pack_bundle(&bundle, limits()).unwrap(), authored);
}
