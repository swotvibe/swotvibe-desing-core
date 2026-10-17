#![allow(dead_code)]

use std::{collections::BTreeMap, path::PathBuf, sync::Arc};
use swotvibe_app::{
    BundleLimits, EditCommand, EditRequest, EditorSession, NodeParent, PngDecodeLimits,
    PreviewOptions, RenderAssetLimits, SharedTextEngine,
};
use swotvibe_core::PageId;
use swotvibe_format::{Bundle, from_json, pack_bundle};
use swotvibe_text::{FontSet, ParleyTextEngine};

pub const NODE: &str = "018f0000-0000-7000-8000-000000000003";
pub const ASSET: &str = "018f0000-0000-7000-8000-000000000004";

pub fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}
pub fn fonts() -> SharedTextEngine {
    let mut fonts = FontSet::new();
    for (family, file) in [
        ("Inter", "inter/Inter-variable.ttf"),
        (
            "Noto Sans Arabic",
            "noto-sans-arabic/NotoSansArabic-variable.ttf",
        ),
    ] {
        fonts
            .register(
                family,
                std::fs::read(root().join("assets/fonts").join(file)).unwrap(),
            )
            .unwrap();
    }
    Arc::new(ParleyTextEngine::new(fonts))
}
pub fn limits() -> BundleLimits {
    BundleLimits {
        max_archive_bytes: 64 << 20,
        max_manifest_bytes: 8 << 20,
        max_assets: 32,
        max_asset_bytes: 16 << 20,
        max_total_asset_bytes: 48 << 20,
    }
}
pub fn render_limits() -> RenderAssetLimits {
    RenderAssetLimits {
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
    }
}
pub fn blank() -> EditorSession {
    let mut session = EditorSession::new(fonts());
    session.new_document().unwrap();
    session
}
pub fn image_seed() -> Bundle {
    let document = from_json(br#"{"schema_version":2,"id":"018f0000-0000-7000-8000-000000000001","pages":[{"id":"018f0000-0000-7000-8000-000000000002","name":"Blank","roots":[]}],"assets":[{"id":"018f0000-0000-7000-8000-000000000004","name":"two-colors.png"}],"nodes":[]}"#).unwrap();
    let image =
        swotvibe_render::RenderedImage::from_rgba(2, 1, vec![255, 0, 0, 255, 0, 0, 255, 255])
            .unwrap()
            .to_png()
            .unwrap();
    Bundle {
        document,
        assets: BTreeMap::from([(ASSET.parse().unwrap(), image)]),
    }
}
pub fn open_bundle(bundle: &Bundle) -> EditorSession {
    let mut session = EditorSession::new(fonts());
    session
        .open_document(
            &pack_bundle(bundle, limits()).unwrap(),
            None,
            limits(),
            render_limits(),
        )
        .unwrap();
    session
}
pub fn request(session: &EditorSession, commands: Vec<EditCommand>) -> EditRequest {
    EditRequest {
        expected_revision: session.revision().as_u64(),
        commands,
    }
}
pub fn create(session: &EditorSession, kind: &str) -> EditCommand {
    EditCommand::CreateNode {
        node: NODE.into(),
        node_kind: kind.into(),
        name: Some("Editable layer".into()),
        fill: None,
        parent: NodeParent::PageRoot {
            page: session.view().unwrap().pages[0].id.clone(),
        },
    }
}
pub fn style() -> EditCommand {
    EditCommand::SetNodeTextStyle {
        node: NODE.into(),
        font_family: "Noto Sans Arabic".into(),
        font_weight: 900,
        align: "center".into(),
        direction: "auto".into(),
    }
}
pub fn preview(session: &EditorSession, size: [f64; 2]) -> swotvibe_app::Preview {
    let page: PageId = session.view().unwrap().pages[0].id.parse().unwrap();
    session
        .preview(
            page,
            PreviewOptions {
                page_size: Some(size),
                scale: 1.,
                background: [21, 14, 35, 255],
            },
        )
        .unwrap()
}
