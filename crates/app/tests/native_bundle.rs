use std::{collections::BTreeMap, sync::Arc};
use swotvibe_app::{
    BundleLimits, EditCommand, EditRequest, EditorSession, PngDecodeLimits, PreviewOptions,
    RenderAssetLimits,
};
use swotvibe_core::{AssetId, PageId};
use swotvibe_format::{Bundle, from_json, pack_bundle, unpack_bundle};
use swotvibe_render::RenderedImage;
use swotvibe_text::{FontSet, ParleyTextEngine};

fn limits() -> BundleLimits {
    BundleLimits {
        max_archive_bytes: 2 << 20,
        max_manifest_bytes: 1 << 20,
        max_assets: 8,
        max_asset_bytes: 1 << 20,
        max_total_asset_bytes: 1 << 20,
    }
}
fn render_limits() -> RenderAssetLimits {
    RenderAssetLimits {
        max_asset_bytes: 1 << 20,
        max_svg_width: 1024,
        max_svg_height: 1024,
        max_svg_pixels: 1024 * 1024,
        png: PngDecodeLimits {
            max_input_bytes: 1 << 20,
            max_width: 1024,
            max_height: 1024,
            max_pixels: 1024 * 1024,
            max_output_bytes: 4 << 20,
            max_decoder_bytes: 4 << 20,
        },
    }
}
fn session() -> EditorSession {
    EditorSession::new(Arc::new(ParleyTextEngine::new(FontSet::new())))
}
fn fixture() -> (Bundle, Vec<u8>) {
    let document = from_json(br#"{
        "schema_version":2,"id":"018f0000-0000-7000-8000-000000000001",
        "pages":[{"id":"018f0000-0000-7000-8000-000000000002","name":"Poster","roots":["018f0000-0000-7000-8000-000000000003"]}],
        "assets":[{"id":"018f0000-0000-7000-8000-000000000004","name":"Original cutout.png"}],
        "nodes":[{"id":"018f0000-0000-7000-8000-000000000003","kind":"image","name":"Independent image layer","props":{
            "transform":[1,0,0,1,0,0],"size":[16,16],"width":"fixed","height":"fixed",
            "image":{"asset":"018f0000-0000-7000-8000-000000000004"}}}]
    }"#).unwrap();
    let payload = RenderedImage::from_rgba(1, 1, vec![14, 155, 219, 255])
        .unwrap()
        .to_png()
        .unwrap();
    let id: AssetId = document.assets[0].id.parse().unwrap();
    let bundle = Bundle {
        document,
        assets: BTreeMap::from([(id, payload)]),
    };
    let bytes = pack_bundle(&bundle, limits()).unwrap();
    (bundle, bytes)
}
fn preview(session: &EditorSession) -> swotvibe_app::Preview {
    let page: PageId = session.view().unwrap().pages[0].id.parse().unwrap();
    session
        .preview(
            page,
            PreviewOptions {
                page_size: Some([16., 16.]),
                scale: 1.,
                background: [0, 0, 0, 255],
            },
        )
        .unwrap()
}

#[test]
fn embedded_images_survive_edit_undo_save_and_reopen() {
    let (original, bytes) = fixture();
    let mut editor = session();
    let view = editor
        .open_document(&bytes, Some("poster.swv".into()), limits(), render_limits())
        .unwrap();
    assert_eq!(editor.document_extension(), "swv");
    assert!(preview(&editor).render_diagnostics.is_empty());
    let image = RenderedImage::from_png(&preview(&editor).png, render_limits().png).unwrap();
    assert_eq!(image.pixel(8, 8), Some([14, 155, 219, 255]));
    editor
        .apply(&EditRequest {
            expected_revision: view.revision,
            commands: vec![EditCommand::RenameNode {
                node: view.nodes[0].id.clone(),
                name: Some("Renamed editable cutout".into()),
            }],
        })
        .unwrap();
    editor.undo(editor.revision().as_u64()).unwrap();
    assert_eq!(
        editor.view().unwrap().nodes[0].name,
        "Independent image layer"
    );
    editor.redo(editor.revision().as_u64()).unwrap();
    let saved = editor.export_bytes().unwrap();
    let recovered = unpack_bundle(&saved, limits()).unwrap();
    assert_eq!(recovered.assets, original.assets);
    let mut reopened = session();
    reopened
        .open_document(&saved, None, limits(), render_limits())
        .unwrap();
    assert_eq!(
        reopened.view().unwrap().nodes[0].name,
        "Renamed editable cutout"
    );
    assert_eq!(preview(&reopened).png, preview(&editor).png);
}

#[test]
fn failed_bundle_admission_preserves_the_current_document_and_images() {
    let (_, bytes) = fixture();
    let mut editor = session();
    editor
        .open_document(&bytes, None, limits(), render_limits())
        .unwrap();
    let before = editor.export_bytes().unwrap();
    let pixels = preview(&editor).png;
    assert!(
        editor
            .open_document(b"PKcorrupt", None, limits(), render_limits())
            .is_err()
    );
    let mut tiny = render_limits();
    tiny.max_asset_bytes = 1;
    tiny.png.max_input_bytes = 1;
    assert!(editor.open_document(&bytes, None, limits(), tiny).is_err());
    assert_eq!(editor.export_bytes().unwrap(), before);
    assert_eq!(preview(&editor).png, pixels);
}

#[test]
fn opening_plain_json_clears_the_previous_bundle_payloads() {
    let (bundle, bytes) = fixture();
    let mut editor = session();
    editor
        .open_document(&bytes, None, limits(), render_limits())
        .unwrap();
    let json = swotvibe_format::to_json(&bundle.document).unwrap();
    editor.open_json(&json, None).unwrap();
    assert_eq!(editor.document_extension(), "json");
    assert_eq!(editor.export_bytes().unwrap(), json);
    assert!(!preview(&editor).render_diagnostics.is_empty());
}
