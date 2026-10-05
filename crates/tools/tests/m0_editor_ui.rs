//! Static Arabic editor UI sample: mixed-script labels, layer list, panels,
//! bounded SVG/PNG resources, deterministic layout and a separately reviewed
//! reference render.

use std::path::PathBuf;
use std::sync::Arc;

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use swotvibe_core::{Color, Snapshot};
use swotvibe_format::{DtoDocument, from_json, import};
use swotvibe_layout::{LayoutEngine, LayoutOptions, TaffyLayoutEngine};
use swotvibe_render::{
    PngDecodeLimits, RenderAssetLimits, RenderAssets, RenderConfig, RenderedImage, Renderer,
    VelloCpuRenderer, compare_images,
};
use swotvibe_text::{FontSet, ParleyTextEngine, TextLayoutEngine};

const WIDTH: u32 = 1440;
const HEIGHT: u32 = 900;
const PIXEL_TOLERANCE: u8 = 2;

fn root() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    while !path.join("assets").join("fonts").is_dir() {
        assert!(path.pop(), "repository root not found");
    }
    path
}

fn read(path: PathBuf) -> Vec<u8> {
    std::fs::read(&path).unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()))
}

fn fonts() -> FontSet {
    let mut fonts = FontSet::new();
    fonts
        .register(
            "Inter",
            read(root().join("assets/fonts/inter/Inter-variable.ttf")),
        )
        .expect("Inter registers");
    fonts
        .register(
            "Noto Sans Arabic",
            read(root().join("assets/fonts/noto-sans-arabic/NotoSansArabic-variable.ttf")),
        )
        .expect("Noto Sans Arabic registers");
    fonts
}

fn render() -> (RenderedImage, Value, Value) {
    let repository = root();
    let fixture = read(repository.join("tests/fixtures/m0-editor-ui-v2.json"));
    let dto: DtoDocument = from_json(&fixture).expect("UI fixture parses");
    let contents: Vec<&str> = dto
        .nodes
        .iter()
        .filter_map(|node| node.props.as_ref()?.text.as_ref())
        .map(|text| text.content.as_str())
        .collect();
    for required in [
        "شاشة الدفع — مسودة",
        "مشاركة",
        "معاينة",
        "النسخة ٣",
        "تحديد",
        "إطار",
        "مستطيل",
        "نص",
        "قلم",
        "الطبقات",
        "الصفحة ١",
        "شريط التنقل",
        "إتمام الشراء",
        "الموضع",
        "العرض",
        "الارتفاع",
        "التعبئة",
        "الحدود",
        "التخطيط التلقائي",
        "المسافة بين العناصر",
        "Frame",
        "Auto Layout",
        "اطلب من المساعد",
        "اجعل الزر أكبر وغيّر لونه إلى الأخضر",
        "تم تعديل ٣ طبقات",
        "قبول",
        "رفض",
    ] {
        assert!(
            contents.iter().any(|content| content.contains(required)),
            "the UI fixture must include {required:?}"
        );
    }
    assert_eq!(dto.assets.len(), 8, "seven SVG icons and one product PNG");
    assert_eq!(
        dto.nodes.iter().filter(|node| node.kind == "image").count(),
        8,
        "every SVG/PNG asset is exercised by an image node"
    );
    for name in ["مشهد شاشة الهاتف", "شجرة الطبقات"] {
        let group = dto
            .nodes
            .iter()
            .find(|node| node.name.as_deref() == Some(name));
        assert!(
            group.is_some_and(|group| !group.children.is_empty()),
            "{name} is nested"
        );
    }
    let mut assets = RenderAssets::new(RenderAssetLimits {
        max_asset_bytes: 2 * 1024 * 1024,
        max_svg_width: 64,
        max_svg_height: 64,
        max_svg_pixels: 4096,
        png: PngDecodeLimits {
            max_input_bytes: 2 * 1024 * 1024,
            max_width: 1024,
            max_height: 1024,
            max_pixels: 1024 * 1024,
            max_output_bytes: 4 * 1024 * 1024,
            max_decoder_bytes: 4 * 1024 * 1024,
        },
    })
    .expect("bounded renderer resources");
    let mut asset_fingerprints = Vec::with_capacity(dto.assets.len());
    for asset in &dto.assets {
        let id = asset.id.parse().expect("asset identity is valid");
        let bytes = read(
            repository
                .join("assets/samples/m0-editor-ui")
                .join(&asset.name),
        );
        let hash = Sha256::digest(&bytes);
        asset_fingerprints.push(json!({
            "name": asset.name,
            "bytes": bytes.len(),
            "sha256": format!("{hash:x}"),
        }));
        assets
            .insert(id, bytes)
            .expect("fixture asset fits its explicit budget");
    }
    let document = import(&dto).expect("UI fixture imports");
    let page = document.pages()[0].id;
    let snapshot = Snapshot::of(&document);
    let text: Arc<dyn TextLayoutEngine> = Arc::new(ParleyTextEngine::new(fonts()));
    let layout = TaffyLayoutEngine::new(Arc::clone(&text))
        .layout(
            &snapshot,
            page,
            &LayoutOptions {
                page_size: Some((f64::from(WIDTH), f64::from(HEIGHT))),
            },
        )
        .expect("UI fixture lays out");
    assert!(
        layout.diagnostics().is_empty(),
        "layout diagnostics: {:?}",
        layout.diagnostics()
    );
    let rendered = VelloCpuRenderer::new(text)
        .with_assets(assets)
        .render(
            &snapshot,
            page,
            &layout,
            &RenderConfig::for_page(
                (f64::from(WIDTH), f64::from(HEIGHT)),
                1.0,
                Color::rgb(244, 246, 249),
            ),
        )
        .expect("UI fixture renders");
    assert!(
        rendered.diagnostics.is_empty(),
        "render diagnostics: {:?}",
        rendered.diagnostics
    );

    let layout_nodes: Vec<Value> = layout
        .nodes()
        .map(|node| {
            let item = document
                .node(node.id)
                .expect("layout nodes refer to the document");
            json!({
                "id": node.id.to_string(),
                "name": item.name(),
                "kind": item.kind.as_str(),
                "offset": node.offset,
                "size": node.size,
                "world": node.world.coefficients(),
                "rect": [node.rect.x, node.rect.y, node.rect.width, node.rect.height],
            })
        })
        .collect();
    let report = json!({
        "viewport": [WIDTH, HEIGHT],
        "units": "design-units at scale 1",
        "nodes": layout_nodes,
    });
    let fingerprint = json!({
        "reviewed": null,
        "fixture": "tests/fixtures/m0-editor-ui-v2.json",
        "layoutEngine": layout.fingerprint().engine,
        "renderEngine": rendered.fingerprint.engine,
        "scale": rendered.fingerprint.scale,
        "pixels": [rendered.fingerprint.pixels.0, rendered.fingerprint.pixels.1],
        "background": rendered.fingerprint.background,
        "pixelTolerance": PIXEL_TOLERANCE,
        "assets": asset_fingerprints,
        "fonts": rendered.fingerprint.fonts.iter().map(|font| json!({
            "family": font.family,
            "bytes": font.bytes,
            "sha256": font.sha256,
        })).collect::<Vec<_>>(),
    });
    (rendered.image, report, fingerprint)
}

#[test]
fn arabic_editor_ui_fixture_matches_its_static_references() {
    let repository = root();
    let golden = repository.join("tests/golden");
    let (actual_image, report, fingerprint) = render();
    assert_eq!(
        (actual_image.width(), actual_image.height()),
        (WIDTH, HEIGHT)
    );
    assert!(actual_image.pixel(10, 10).is_some());

    let files = [
        (
            "m0-editor-ui.layout.json",
            serde_json::to_vec_pretty(&report).expect("layout JSON"),
        ),
        (
            "m0-editor-ui.fingerprint.json",
            serde_json::to_vec_pretty(&fingerprint).expect("fingerprint JSON"),
        ),
    ];
    if std::env::var_os("SWOTVIBE_UPDATE_EDITOR_GOLDEN").is_some() {
        std::fs::write(
            golden.join("m0-editor-ui.png"),
            actual_image.to_png().expect("PNG encode"),
        )
        .expect("write UI image golden");
        for (name, bytes) in files {
            std::fs::write(golden.join(name), [bytes.as_slice(), b"\n"].concat())
                .unwrap_or_else(|error| panic!("cannot write {name}: {error}"));
        }
        return;
    }

    for (name, bytes) in files {
        let expected = read(golden.join(name));
        let expected: Value = serde_json::from_slice(&expected).expect("golden JSON parses");
        let actual: Value = serde_json::from_slice(&bytes).expect("actual JSON parses");
        let normalized = if name.ends_with("fingerprint.json") {
            let mut actual = actual;
            if let Some(object) = actual.as_object_mut() {
                object.remove("reviewed");
            }
            let mut expected = expected;
            if let Some(object) = expected.as_object_mut() {
                object.remove("reviewed");
            }
            (actual, expected)
        } else {
            (actual, expected)
        };
        assert_eq!(
            normalized.0, normalized.1,
            "{name} changed; regenerate deliberately"
        );
    }

    let png = read(golden.join("m0-editor-ui.png"));
    let expected_image = RenderedImage::from_png(
        &png,
        PngDecodeLimits {
            max_input_bytes: 16 * 1024 * 1024,
            max_width: WIDTH,
            max_height: HEIGHT,
            max_pixels: u64::from(WIDTH) * u64::from(HEIGHT),
            max_output_bytes: 16 * 1024 * 1024,
            max_decoder_bytes: 16 * 1024 * 1024,
        },
    )
    .expect("golden PNG decodes within bounds");
    let difference = compare_images(&actual_image, &expected_image, PIXEL_TOLERANCE);
    assert!(
        difference.is_match(),
        "golden pixels differ: {difference:?}"
    );
}
