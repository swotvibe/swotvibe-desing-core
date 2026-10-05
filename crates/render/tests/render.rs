//! Rendering against the pinned fonts and the sample document.

// The helper lives beside the text crate's tests so one definition serves the
// text, layout, and render reference tests.
#[path = "../../text/tests/support/mod.rs"]
mod support;

use std::sync::Arc;

use support::{LATIN_FAMILY, fixture_text, pinned_fonts};
use swotvibe_core::{
    AssetId, Color, Command, Content, Document, FrameLayout, ImageProps, NodeId, NodeKind,
    NodePlacement, NodeProps, PageId, Position, Scalar, ShapeGeometry, ShapeProps, Size, Snapshot,
    Stroke, Transform, apply_batch_at_current,
};
use swotvibe_format::{from_json, import};
use swotvibe_layout::{LayoutEngine, LayoutOptions, TaffyLayoutEngine};
use swotvibe_render::{
    PngDecodeLimits, RenderAssetLimits, RenderAssets, RenderConfig, RenderDiagnostic, RenderError,
    Renderer, VelloCpuRenderer, engine_id,
};
use swotvibe_text::{ParleyTextEngine, TextLayoutEngine};

const WHITE: Color = Color::rgb(255, 255, 255);

struct Scene {
    document: Document,
    page: PageId,
    text: Arc<dyn TextLayoutEngine>,
}

impl Scene {
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
        apply_batch_at_current(&mut document, &commands).expect("the scene should build");

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
            text: Arc::new(ParleyTextEngine::new(pinned_fonts())),
        }
    }

    fn render(&self, config: &RenderConfig) -> swotvibe_render::RenderedPage {
        let layout_engine = TaffyLayoutEngine::new(Arc::clone(&self.text));
        let snapshot = Snapshot::of(&self.document);
        let layout = layout_engine
            .layout(
                &snapshot,
                self.page,
                &LayoutOptions {
                    page_size: Some(config.design_extent()),
                },
            )
            .expect("layout should succeed");
        let renderer = VelloCpuRenderer::new(Arc::clone(&self.text));
        renderer
            .render(&snapshot, self.page, &layout, config)
            .expect("render should succeed")
    }
}

fn shape_props(size: (f64, f64), geometry: ShapeGeometry, fill: Color) -> NodeProps {
    let mut props = NodeProps::default_for(NodeKind::Shape);
    props.size = Size::new(size.0, size.1).expect("non-negative");
    props.fill = Some(fill);
    props.content = Content::Shape(ShapeProps {
        geometry,
        corner_radius: Scalar::ZERO,
    });
    props
}

fn config(width: u32, height: u32, scale: f64) -> RenderConfig {
    RenderConfig {
        width,
        height,
        scale,
        background: WHITE,
    }
}

#[test]
fn an_empty_page_renders_the_background_everywhere() {
    let scene = Scene::new(&[]);
    let page = scene.render(&config(16, 16, 1.0));
    assert_eq!((page.image.width(), page.image.height()), (16, 16));
    for y in 0..16 {
        for x in 0..16 {
            assert_eq!(
                page.image.pixel(x, y),
                Some([255, 255, 255, 255]),
                "pixel ({x}, {y}) should be the background"
            );
        }
    }
    assert!(page.diagnostics.is_empty());
}

#[test]
fn a_rectangle_fills_its_box_and_leaves_the_outside_alone() {
    let scene = Scene::new(&[(
        NodeKind::Shape,
        shape_props((10.0, 6.0), ShapeGeometry::Rect, Color::rgb(255, 0, 0)),
    )]);
    let page = scene.render(&config(20, 20, 1.0));

    // The rectangle starts at the origin, so its own corner is filled and the
    // first pixel outside its box is not.
    assert_eq!(page.image.pixel(0, 0), Some([255, 0, 0, 255]));
    assert_eq!(page.image.pixel(5, 3), Some([255, 0, 0, 255]));
    assert_eq!(page.image.pixel(10, 6), Some([255, 255, 255, 255]));
}

#[test]
fn a_shape_translated_by_its_transform_is_drawn_where_layout_put_it() {
    let mut props = shape_props((6.0, 6.0), ShapeGeometry::Rect, Color::rgb(0, 0, 255));
    props.transform = Transform::translate(4.0, 4.0).expect("finite");
    let scene = Scene::new(&[(NodeKind::Shape, props)]);
    let page = scene.render(&config(20, 20, 1.0));

    assert_eq!(page.image.pixel(6, 6), Some([0, 0, 255, 255]));
    assert_eq!(page.image.pixel(1, 1), Some([255, 255, 255, 255]));
}

#[test]
fn an_ellipse_is_round() {
    let scene = Scene::new(&[(
        NodeKind::Shape,
        shape_props((20.0, 20.0), ShapeGeometry::Ellipse, Color::rgb(0, 128, 0)),
    )]);
    let page = scene.render(&config(20, 20, 1.0));

    assert_eq!(page.image.pixel(10, 10), Some([0, 128, 0, 255]));
    // The corner of the box is outside the inscribed ellipse.
    assert_eq!(page.image.pixel(0, 0), Some([255, 255, 255, 255]));
    assert_eq!(page.image.pixel(19, 19), Some([255, 255, 255, 255]));
}

#[test]
fn a_stroke_is_drawn_around_the_fill() {
    let mut props = shape_props((10.0, 10.0), ShapeGeometry::Rect, Color::rgb(255, 255, 255));
    props.stroke = Some(Stroke {
        color: Color::rgb(0, 0, 0),
        width: Scalar::new(2.0).expect("finite"),
    });
    let scene = Scene::new(&[(NodeKind::Shape, props)]);
    let page = scene.render(&config(14, 14, 1.0));

    // The stroke is centred on the edge, so a pixel just inside the outline is
    // dark and the middle is the fill.
    assert_eq!(page.image.pixel(0, 5), Some([0, 0, 0, 255]));
    assert_eq!(page.image.pixel(5, 5), Some([255, 255, 255, 255]));
}

#[test]
fn a_scale_multiplies_the_drawn_size() {
    let scene = Scene::new(&[(
        NodeKind::Shape,
        shape_props((5.0, 5.0), ShapeGeometry::Rect, Color::rgb(0, 0, 0)),
    )]);
    let unit = scene.render(&config(20, 20, 1.0));
    let doubled = scene.render(&config(40, 40, 2.0));

    assert_eq!(unit.image.pixel(4, 4), Some([0, 0, 0, 255]));
    assert_eq!(unit.image.pixel(5, 5), Some([255, 255, 255, 255]));
    assert_eq!(doubled.image.pixel(9, 9), Some([0, 0, 0, 255]));
    assert_eq!(doubled.image.pixel(10, 10), Some([255, 255, 255, 255]));
}

#[test]
fn text_puts_ink_inside_its_measured_box() {
    let mut props = NodeProps::default_for(NodeKind::Text);
    props.fill = Some(Color::rgb(0, 0, 0));
    if let Content::Text(text) = &mut props.content {
        text.content = "Hello".into();
        text.font_family = LATIN_FAMILY.into();
        text.font_size = Scalar::new(32.0).expect("finite");
    }
    let scene = Scene::new(&[(NodeKind::Text, props)]);
    let page = scene.render(&config(200, 80, 1.0));

    let ink = (0..200)
        .flat_map(|x| (0..80).map(move |y| (x, y)))
        .filter(|(x, y)| page.image.pixel(*x, *y) != Some([255, 255, 255, 255]))
        .count();
    assert!(
        ink > 50,
        "shaping and drawing `Hello` should leave ink, found {ink} pixels"
    );
    // The far side of the image is untouched, so the ink is where the box is.
    assert_eq!(page.image.pixel(199, 79), Some([255, 255, 255, 255]));
}

#[test]
fn orientable_text_and_arabic_text_both_draw() {
    let mut latin = NodeProps::default_for(NodeKind::Text);
    latin.fill = Some(Color::rgb(0, 0, 0));
    latin.transform = Transform::translate(0.0, 0.0).expect("finite");
    if let Content::Text(text) = &mut latin.content {
        text.content = "Latin".into();
        text.font_family = LATIN_FAMILY.into();
        text.font_size = Scalar::new(20.0).expect("finite");
    }

    let mut arabic = NodeProps::default_for(NodeKind::Text);
    arabic.fill = Some(Color::rgb(0, 0, 0));
    arabic.transform = Transform::translate(0.0, 30.0).expect("finite");
    if let Content::Text(text) = &mut arabic.content {
        text.content = "مرحبا".into();
        text.font_family = support::ARABIC_FAMILY.into();
        text.font_size = Scalar::new(20.0).expect("finite");
    }

    let scene = Scene::new(&[(NodeKind::Text, latin), (NodeKind::Text, arabic)]);
    let page = scene.render(&config(200, 60, 1.0));

    let ink_in = |top: u32, bottom: u32| {
        (0..200u32)
            .flat_map(|x| (top..bottom).map(move |y| (x, y)))
            .filter(|(x, y)| page.image.pixel(*x, *y) != Some([255, 255, 255, 255]))
            .count()
    };
    assert!(ink_in(0, 30) > 20, "the Latin line should have ink");
    assert!(ink_in(30, 60) > 20, "the Arabic line should have ink");
}

#[test]
fn two_renders_of_one_scene_are_byte_identical() {
    let scene = Scene::new(&[(
        NodeKind::Shape,
        shape_props((12.5, 7.25), ShapeGeometry::Ellipse, Color::rgb(20, 40, 60)),
    )]);
    let first = scene.render(&config(24, 16, 1.0));
    let second = scene.render(&config(24, 16, 1.0));
    assert_eq!(first.image, second.image);
    assert_eq!(first.fingerprint, second.fingerprint);
    assert_eq!(
        first.image.to_png().expect("encode"),
        second.image.to_png().expect("encode")
    );
}

#[test]
fn the_fingerprint_records_the_backend_scale_and_fonts() {
    let scene = Scene::new(&[(
        NodeKind::Shape,
        shape_props((4.0, 4.0), ShapeGeometry::Rect, Color::rgb(0, 0, 0)),
    )]);
    let page = scene.render(&config(16, 16, 2.0));
    let fingerprint = &page.fingerprint;

    assert_eq!(fingerprint.engine, engine_id());
    assert_eq!(fingerprint.scale, 2.0);
    assert_eq!(fingerprint.pixels, (16, 16));
    assert_eq!(fingerprint.background, [255, 255, 255, 255]);
    assert_eq!(fingerprint.fonts.len(), 2);
    assert!(fingerprint.layout_engine.starts_with("taffy"));
}

#[test]
fn an_image_node_without_pixels_is_reported_and_skipped() {
    let asset = AssetId::new();
    let mut document = Document::new();
    let page = PageId::new();
    let image = NodeId::new();
    apply_batch_at_current(
        &mut document,
        &[
            Command::CreatePage {
                id: page,
                name: "Page 1".into(),
            },
            Command::CreateAsset {
                id: asset,
                name: "logo.png".into(),
            },
            Command::CreateNode {
                id: image,
                kind: NodeKind::Image,
                name: None,
                parent: NodePlacement::PageRoot {
                    page,
                    position: Position::Last,
                },
            },
        ],
    )
    .expect("the document should build");

    let mut props = NodeProps::default_for(NodeKind::Image);
    props.size = Size::new(8.0, 8.0).expect("non-negative");
    props.transform = Transform::translate(2.0, 2.0).expect("finite");
    props.content = Content::Image(ImageProps { asset: Some(asset) });
    apply_batch_at_current(
        &mut document,
        &[Command::SetNodeProps {
            id: image,
            props: Box::new(props),
        }],
    )
    .expect("the properties should apply");

    let text: Arc<dyn TextLayoutEngine> = Arc::new(ParleyTextEngine::new(pinned_fonts()));
    let snapshot = Snapshot::of(&document);
    let layout = TaffyLayoutEngine::new(Arc::clone(&text))
        .layout(
            &snapshot,
            page,
            &LayoutOptions {
                page_size: Some((16.0, 16.0)),
            },
        )
        .expect("layout should succeed");
    let rendered = VelloCpuRenderer::new(text)
        .render(&snapshot, page, &layout, &config(16, 16, 1.0))
        .expect("render should succeed");

    assert!(matches!(
        rendered.diagnostics.as_slice(),
        [RenderDiagnostic::UnresolvedImage { .. }]
    ));
    assert_eq!(rendered.image.pixel(4, 4), Some([255, 255, 255, 255]));
}

#[test]
fn png_and_svg_assets_are_decoded_and_painted_inside_image_nodes() {
    let png_id = AssetId::new();
    let svg_id = AssetId::new();
    let png = swotvibe_render::RenderedImage::from_rgba(8, 8, [255, 0, 0, 255].repeat(64))
        .expect("a red PNG image");
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8" viewBox="0 0 8 8"><rect width="8" height="8" fill="#0000ff"/></svg>"##;

    let mut document = Document::new();
    let page = PageId::new();
    let png_node = NodeId::new();
    let svg_node = NodeId::new();
    apply_batch_at_current(
        &mut document,
        &[
            Command::CreatePage {
                id: page,
                name: "Assets".into(),
            },
            Command::CreateAsset {
                id: png_id,
                name: "red.png".into(),
            },
            Command::CreateAsset {
                id: svg_id,
                name: "blue.svg".into(),
            },
            Command::CreateNode {
                id: png_node,
                kind: NodeKind::Image,
                name: Some("PNG".into()),
                parent: NodePlacement::PageRoot {
                    page,
                    position: Position::Last,
                },
            },
            Command::CreateNode {
                id: svg_node,
                kind: NodeKind::Image,
                name: Some("SVG".into()),
                parent: NodePlacement::PageRoot {
                    page,
                    position: Position::Last,
                },
            },
        ],
    )
    .expect("the document should build");

    let mut png_props = NodeProps::default_for(NodeKind::Image);
    png_props.size = Size::new(8.0, 8.0).expect("size");
    png_props.content = Content::Image(ImageProps {
        asset: Some(png_id),
    });
    let mut svg_props = NodeProps::default_for(NodeKind::Image);
    svg_props.size = Size::new(8.0, 8.0).expect("size");
    svg_props.transform = Transform::translate(8.0, 0.0).expect("finite");
    svg_props.content = Content::Image(ImageProps {
        asset: Some(svg_id),
    });
    apply_batch_at_current(
        &mut document,
        &[
            Command::SetNodeProps {
                id: png_node,
                props: Box::new(png_props),
            },
            Command::SetNodeProps {
                id: svg_node,
                props: Box::new(svg_props),
            },
        ],
    )
    .expect("the image properties should apply");

    let limits = RenderAssetLimits {
        max_asset_bytes: 1024 * 1024,
        max_svg_width: 64,
        max_svg_height: 64,
        max_svg_pixels: 4096,
        png: PngDecodeLimits {
            max_input_bytes: 1024 * 1024,
            max_width: 64,
            max_height: 64,
            max_pixels: 4096,
            max_output_bytes: 64 * 64 * 4,
            max_decoder_bytes: 1024 * 1024,
        },
    };
    let mut assets = RenderAssets::new(limits).expect("valid limits");
    assets
        .insert(png_id, png.to_png().expect("encode PNG"))
        .expect("store PNG");
    assets.insert(svg_id, svg.to_vec()).expect("store SVG");

    let text: Arc<dyn TextLayoutEngine> = Arc::new(ParleyTextEngine::new(pinned_fonts()));
    let snapshot = Snapshot::of(&document);
    let layout = TaffyLayoutEngine::new(Arc::clone(&text))
        .layout(
            &snapshot,
            page,
            &LayoutOptions {
                page_size: Some((16.0, 8.0)),
            },
        )
        .expect("layout should succeed");
    let rendered = VelloCpuRenderer::new(text)
        .with_assets(assets)
        .render(&snapshot, page, &layout, &config(16, 8, 1.0))
        .expect("render should succeed");

    assert!(rendered.diagnostics.is_empty());
    assert_eq!(rendered.image.pixel(4, 4), Some([255, 0, 0, 255]));
    assert_eq!(rendered.image.pixel(12, 4), Some([0, 0, 255, 255]));
}

#[test]
fn a_node_without_paint_is_reported_rather_than_drawn() {
    let mut props = shape_props((8.0, 8.0), ShapeGeometry::Rect, Color::rgb(0, 0, 0));
    props.fill = None;
    let scene = Scene::new(&[(NodeKind::Shape, props)]);
    let page = scene.render(&config(16, 16, 1.0));

    assert!(matches!(
        page.diagnostics.as_slice(),
        [RenderDiagnostic::NoPaint { .. }]
    ));
}

#[test]
fn a_group_is_not_drawn() {
    let scene = Scene::new(&[(NodeKind::Group, NodeProps::default_for(NodeKind::Group))]);
    let page = scene.render(&config(8, 8, 1.0));
    assert!(
        page.diagnostics.is_empty(),
        "a group has no paint to report"
    );
    assert_eq!(page.image.pixel(4, 4), Some([255, 255, 255, 255]));
}

#[test]
fn an_unusable_configuration_is_refused() {
    let scene = Scene::new(&[]);
    let layout_engine = TaffyLayoutEngine::new(Arc::clone(&scene.text));
    let snapshot = Snapshot::of(&scene.document);
    let layout = layout_engine
        .layout(&snapshot, scene.page, &LayoutOptions::default())
        .expect("layout");
    let renderer = VelloCpuRenderer::new(Arc::clone(&scene.text));

    assert!(matches!(
        renderer
            .render(&snapshot, scene.page, &layout, &config(0, 10, 1.0))
            .expect_err("a zero width must fail"),
        RenderError::InvalidConfig { .. }
    ));
    assert!(matches!(
        renderer
            .render(&snapshot, scene.page, &layout, &config(10, 10, 0.0))
            .expect_err("a zero scale must fail"),
        RenderError::InvalidConfig { .. }
    ));
}

#[test]
fn a_page_the_layout_does_not_describe_is_refused() {
    let scene = Scene::new(&[]);
    let layout_engine = TaffyLayoutEngine::new(Arc::clone(&scene.text));
    let snapshot = Snapshot::of(&scene.document);
    let layout = layout_engine
        .layout(&snapshot, scene.page, &LayoutOptions::default())
        .expect("layout");
    let renderer = VelloCpuRenderer::new(Arc::clone(&scene.text));
    let other = PageId::new();

    assert_eq!(
        renderer
            .render(&snapshot, other, &layout, &config(8, 8, 1.0))
            .expect_err("an unknown page must fail"),
        RenderError::PageNotFound { page: other }
    );
}

#[test]
fn the_sample_document_renders_with_its_fingerprint_in_the_png() {
    let dto = from_json(fixture_text("m0-sample-v2.json").as_bytes()).expect("the fixture parses");
    let document = import(&dto).expect("the fixture opens");
    let page = document.pages()[0].id;
    let text: Arc<dyn TextLayoutEngine> = Arc::new(ParleyTextEngine::new(pinned_fonts()));
    let snapshot = Snapshot::of(&document);
    let layout = TaffyLayoutEngine::new(Arc::clone(&text))
        .layout(
            &snapshot,
            page,
            &LayoutOptions {
                page_size: Some((480.0, 320.0)),
            },
        )
        .expect("the sample should lay out");

    let config = RenderConfig::for_page(layout.page_size(), 1.0, WHITE);
    let page = VelloCpuRenderer::new(Arc::clone(&text))
        .render(&snapshot, page_id_of(&document), &layout, &config)
        .expect("the sample should render");

    assert_eq!(page.image.width(), 480);
    assert_eq!(page.image.height(), 320);
    assert_eq!(page.diagnostics, [], "the sample should need no omission");
    // The card's fill is white, its stroke is not: the sample must not be a
    // blank image, so at least the header band and the text are drawn.
    let ink = page
        .image
        .rgba()
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|pixel| pixel[..3] != [255, 255, 255])
        .count();
    assert!(
        ink > 1000,
        "the sample should draw its content, found {ink} pixels"
    );
    assert!(page.image.to_png().expect("encode").len() > 1000);
}

fn page_id_of(document: &Document) -> PageId {
    document.pages()[0].id
}

#[test]
fn a_frame_with_no_fill_draws_only_its_children() {
    // A group's frame layout must not add paint, and a frame with no fill must
    // still let its children through.
    let mut frame = NodeProps::default_for(NodeKind::Frame);
    frame.size = Size::new(20.0, 20.0).expect("non-negative");
    frame.content = Content::Frame(swotvibe_core::FrameProps {
        layout: FrameLayout::None,
    });

    let mut child = shape_props((4.0, 4.0), ShapeGeometry::Rect, Color::rgb(0, 0, 0));
    child.transform = Transform::translate(8.0, 8.0).expect("finite");

    let mut document = Document::new();
    let page = PageId::new();
    let frame_id = NodeId::new();
    let child_id = NodeId::new();
    apply_batch_at_current(
        &mut document,
        &[
            Command::CreatePage {
                id: page,
                name: "Page 1".into(),
            },
            Command::CreateNode {
                id: frame_id,
                kind: NodeKind::Frame,
                name: None,
                parent: NodePlacement::PageRoot {
                    page,
                    position: Position::Last,
                },
            },
            Command::CreateNode {
                id: child_id,
                kind: NodeKind::Shape,
                name: None,
                parent: NodePlacement::Child {
                    parent: frame_id,
                    position: Position::Last,
                },
            },
            Command::SetNodeProps {
                id: frame_id,
                props: Box::new(frame),
            },
            Command::SetNodeProps {
                id: child_id,
                props: Box::new(child),
            },
        ],
    )
    .expect("the scene should build");

    let text: Arc<dyn TextLayoutEngine> = Arc::new(ParleyTextEngine::new(pinned_fonts()));
    let snapshot = Snapshot::of(&document);
    let layout = TaffyLayoutEngine::new(Arc::clone(&text))
        .layout(
            &snapshot,
            page,
            &LayoutOptions {
                page_size: Some((20.0, 20.0)),
            },
        )
        .expect("layout should succeed");
    let rendered = VelloCpuRenderer::new(text)
        .render(&snapshot, page, &layout, &config(20, 20, 1.0))
        .expect("render should succeed");

    assert_eq!(rendered.image.pixel(10, 10), Some([0, 0, 0, 255]));
    assert_eq!(rendered.image.pixel(1, 1), Some([255, 255, 255, 255]));
}
