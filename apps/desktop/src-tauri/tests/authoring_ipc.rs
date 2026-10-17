use serde_json::{Value, json};
use std::sync::Arc;
use std::{collections::BTreeMap, fs};
use swotvibe_app::{BundleLimits, EditorSession, PngDecodeLimits, RenderAssetLimits};
use swotvibe_desktop::{
    commands::{self, EditorState},
    font_directory,
};
use swotvibe_format::{Bundle, from_json, pack_bundle};
use swotvibe_text::{FontSet, ParleyTextEngine};
use tauri::test::{MockRuntime, get_ipc_response, mock_builder, mock_context, noop_assets};

fn invoke(
    window: &tauri::WebviewWindow<MockRuntime>,
    command: &str,
    body: Value,
) -> Result<Value, Value> {
    get_ipc_response(
        window,
        tauri::webview::InvokeRequest {
            cmd: command.into(),
            callback: tauri::ipc::CallbackFn(0),
            error: tauri::ipc::CallbackFn(1),
            url: "http://tauri.localhost".parse().unwrap(),
            body: tauri::ipc::InvokeBody::Json(body),
            headers: Default::default(),
            invoke_key: tauri::test::INVOKE_KEY.into(),
        },
    )
    .map(|body| body.deserialize().unwrap())
}

#[test]
fn authoring_uses_existing_ipc_and_preserves_state_after_refusals() {
    let root = font_directory()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_owned();
    let fixture = root.join("tests/fixtures/native-authoring");
    let document = from_json(&fs::read(fixture.join("seed.json")).unwrap()).unwrap();
    assert!(document.nodes.is_empty());
    let mut assets = BTreeMap::new();
    for asset in &document.assets {
        assets.insert(
            asset.id.parse().unwrap(),
            fs::read(fixture.join("assets").join(&asset.id)).unwrap(),
        );
    }
    let limits = BundleLimits {
        max_archive_bytes: 64 << 20,
        max_manifest_bytes: 8 << 20,
        max_assets: 32,
        max_asset_bytes: 16 << 20,
        max_total_asset_bytes: 48 << 20,
    };
    let render = RenderAssetLimits {
        max_asset_bytes: 16 << 20,
        max_svg_width: 4096,
        max_svg_height: 4096,
        max_svg_pixels: 16 << 20,
        png: PngDecodeLimits {
            max_input_bytes: 16 << 20,
            max_width: 4096,
            max_height: 4096,
            max_pixels: 16 << 20,
            max_output_bytes: 64 << 20,
            max_decoder_bytes: 64 << 20,
        },
    };
    let mut fonts = FontSet::new();
    for (family, file) in [
        ("Inter", "inter/Inter-variable.ttf"),
        (
            "Noto Sans Arabic",
            "noto-sans-arabic/NotoSansArabic-variable.ttf",
        ),
    ] {
        fonts
            .register(family, fs::read(font_directory().join(file)).unwrap())
            .unwrap();
    }
    let mut session = EditorSession::new(Arc::new(ParleyTextEngine::new(fonts)));
    let revision = session
        .open_document(
            &pack_bundle(&Bundle { document, assets }, limits).unwrap(),
            None,
            limits,
            render,
        )
        .unwrap()
        .revision;
    let app = mock_builder()
        .manage(EditorState::new(session))
        .invoke_handler(tauri::generate_handler![
            commands::apply,
            commands::get_view,
            commands::capabilities
        ])
        .build(mock_context(noop_assets()))
        .unwrap();
    let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    let commands: Value =
        serde_json::from_slice(&fs::read(fixture.join("commands.json")).unwrap()).unwrap();
    let result = invoke(
        &window,
        "apply",
        json!({"request":{"expectedRevision":revision,"commands":commands}}),
    )
    .unwrap();
    assert_eq!(result["addedNodes"].as_array().unwrap().len(), 22);
    let view = invoke(&window, "get_view", json!({})).unwrap();
    let nodes = view["nodes"].as_array().unwrap();
    let title = nodes
        .iter()
        .find(|node| node["props"]["fontWeight"] == 900)
        .unwrap();
    let image = nodes.iter().find(|node| node["kind"] == "image").unwrap();
    assert_eq!(title["props"]["textAlign"], "center");
    assert!(image["props"]["imageAsset"].is_string());
    let caps = invoke(&window, "capabilities", json!({})).unwrap();
    assert!(
        caps["editableCommands"]
            .as_array()
            .unwrap()
            .contains(&json!("set-node-image-asset"))
    );
    let refused=invoke(&window,"apply",json!({"request":{"expectedRevision":view["revision"],"commands":[
        {"kind":"set-node-text","node":title["id"],"content":"لا يجب حفظ هذا التعديل","fontSize":20},
        {"kind":"set-node-image-asset","node":image["id"],"asset":"018f0000-0000-7000-8000-000000000099"}
    ]}})).unwrap_err();
    assert_eq!(refused["code"], "command-rejected");
    assert_eq!(invoke(&window, "get_view", json!({})).unwrap(), view);
    let stale = invoke(
        &window,
        "apply",
        json!({"request":{"expectedRevision":revision,"commands":[]}}),
    )
    .unwrap_err();
    assert_eq!(stale["code"], "revision-conflict");
    assert_eq!(invoke(&window, "get_view", json!({})).unwrap(), view);
}
