//! The vello_cpu renderer: the temporary render backend of the M0 slice.
//!
//! vello_cpu owns rasterization; this module owns the scene walk, the
//! translation of the document's paint and text into drawing calls, and the
//! rules that make a reference image reproducible:
//!
//! - **Quality pipeline, baseline SIMD.** The f32 pipeline is chosen for
//!   accuracy, and the SIMD level is pinned to the baseline so a rasterized edge
//!   cannot depend on the host CPU's vector width.
//! - **Deterministic geometry.** Shape paths are built from kurbo's fixed
//!   tolerance, and glyph outlines come from the unhinted outline data through
//!   the text contract, so no hinting implementation is involved.
//! - **Documented omissions.** A synthesized style, an image without pixels, a
//!   node without paint, and a kind this renderer does not draw all become
//!   diagnostics beside the image instead of disappearing into it.
//!
//! ## What is drawn, and where
//!
//! A node's box is `(0, 0)` to `(width, height)` in its own space, and its
//! layout `world` transform maps that box into page space. The renderer composes
//! the world transform with the pixel scale and draws in page space, so one
//! transform chain serves shapes and glyphs and the two cannot drift apart.
//!
//! Text is shaped for drawing with the same rule the layout measured with: a
//! `Hug` width is never wrapped, and any other width wraps at the laid out box
//! width. That is what keeps a measured height and a drawn height equal.

use std::collections::BTreeMap;
use std::sync::Arc;

use swotvibe_core::{AssetId, Content, Node, PageId, ShapeGeometry, Sizing, Snapshot};
use swotvibe_layout::LayoutResult;
use swotvibe_text::{GlyphOutline, PathCommand, ShapedText, TextLayoutEngine, TextRequest};
use vello_cpu::kurbo::{Affine, BezPath, Ellipse, Rect as KurboRect, RoundedRect, Shape};
use vello_cpu::peniko::color::{AlphaColor, Srgb};
use vello_cpu::{
    Level, Pixmap, RasterizerSettings, RenderContext, RenderMode, RenderSettings, Resources,
    TargetInit,
};

use crate::VELLO_ENGINE_ID;
use crate::contract::{
    PixelDifference, RenderAssetLimits, RenderAssets, RenderConfig, RenderDiagnostic, RenderError,
    RenderFingerprint, RenderedImage, RenderedPage, Renderer,
};

/// The path tolerance, in design units, used when converting a shape to curves.
///
/// A fixed value rather than a scale-dependent one: the tolerance is part of the
/// geometry, so the same document always produces the same path, and a change
/// here is a change to the reference image.
const PATH_TOLERANCE: f64 = 0.1;

/// A renderer built on vello_cpu 0.3.
pub struct VelloCpuRenderer {
    text: Arc<dyn TextLayoutEngine>,
    assets: Option<RenderAssets>,
}

impl VelloCpuRenderer {
    /// Builds a renderer that shapes and draws text through `text`.
    ///
    /// The text engine is the one the layout measured with, so the glyphs that
    /// are drawn are the glyphs that were measured.
    #[must_use]
    pub fn new(text: Arc<dyn TextLayoutEngine>) -> Self {
        Self { text, assets: None }
    }

    /// Supplies encoded image resources for document image nodes.
    #[must_use]
    pub fn with_assets(mut self, assets: RenderAssets) -> Self {
        self.assets = Some(assets);
        self
    }
}

impl Renderer for VelloCpuRenderer {
    fn engine_id(&self) -> &'static str {
        VELLO_ENGINE_ID
    }

    fn render(
        &self,
        snapshot: &Snapshot,
        page: PageId,
        layout: &LayoutResult,
        config: &RenderConfig,
    ) -> Result<RenderedPage, RenderError> {
        config.check()?;
        if snapshot.page(page).is_none() {
            return Err(RenderError::PageNotFound { page });
        }
        if layout.page() != page {
            return Err(RenderError::PageNotFound { page });
        }

        let mut context = RenderContext::new_with(
            config.width as u16,
            config.height as u16,
            RenderSettings {
                // Pinned so a golden cannot depend on the host's vector width.
                level: Level::baseline(),
                num_threads: 0,
            },
        );

        let mut diagnostics: Vec<RenderDiagnostic> = Vec::new();
        let mut decoded_assets: BTreeMap<AssetId, RenderedImage> = BTreeMap::new();
        let pixel_scale = config.scale;

        // Drawing in the layout's own order means a parent is drawn before its
        // children, which is what "children sit on top" means without a second
        // traversal.
        for id in layout.order() {
            let Some(node) = snapshot.node(*id) else {
                continue;
            };
            let Some(shape) = layout.node(*id) else {
                return Err(RenderError::MissingLayout { node: *id });
            };
            // `then_scale` composes in the drawing order: a node point is
            // mapped into page space first, and then into pixels.
            context.set_transform(affine_of(&shape.world).then_scale(pixel_scale));

            match &node.props.content {
                Content::Shape(geometry) => {
                    let path = shape_path(geometry, shape.size);
                    paint(&mut context, node, &path, &mut diagnostics);
                }
                Content::Text(text) => {
                    self.draw_text(
                        &mut context,
                        node,
                        text,
                        shape.size,
                        &shape.world,
                        pixel_scale,
                        &mut diagnostics,
                    )?;
                }
                Content::Image(image) => {
                    let Some(asset) = image.asset else {
                        diagnostics.push(RenderDiagnostic::UnresolvedImage {
                            node: *id,
                            asset: None,
                        });
                        continue;
                    };
                    let Some(resources) = &self.assets else {
                        diagnostics.push(RenderDiagnostic::UnresolvedImage {
                            node: *id,
                            asset: Some(asset.to_string()),
                        });
                        continue;
                    };
                    let Some(bytes) = resources.get(asset) else {
                        diagnostics.push(RenderDiagnostic::UnresolvedImage {
                            node: *id,
                            asset: Some(asset.to_string()),
                        });
                        continue;
                    };
                    if let std::collections::btree_map::Entry::Vacant(entry) =
                        decoded_assets.entry(asset)
                    {
                        let decoded = decode_asset(bytes, asset, resources.limits())?;
                        entry.insert(decoded);
                    }
                    let decoded = &decoded_assets[&asset];
                    draw_image(&mut context, decoded, shape.size, &shape.world, pixel_scale);
                }
                Content::Frame(_) => {
                    // A frame draws its own box; its children are drawn by their
                    // own entries.
                    let path = KurboRect::new(0.0, 0.0, shape.size.0, shape.size.1)
                        .to_path(PATH_TOLERANCE);
                    paint(&mut context, node, &path, &mut diagnostics);
                }
                Content::None => {
                    // A group is a container with no paint of its own.
                }
            }
        }

        context.flush();
        let mut pixmap = Pixmap::new(config.width as u16, config.height as u16);
        let background = to_alpha_color(config.background);
        context.render_with(
            &mut pixmap,
            &mut Resources::new(),
            RasterizerSettings {
                render_mode: RenderMode::OptimizeQuality,
                target_init: TargetInit::Clear(background),
                ..RasterizerSettings::default()
            },
        );

        let image = RenderedImage::from_rgba(
            config.width,
            config.height,
            unpremultiply(pixmap.data_as_u8_slice()),
        )?;

        Ok(RenderedPage {
            image,
            diagnostics,
            fingerprint: RenderFingerprint {
                engine: VELLO_ENGINE_ID.to_owned(),
                layout_engine: layout.fingerprint().engine.clone(),
                pixels: (config.width, config.height),
                scale_bits: config.scale.to_bits(),
                scale: config.scale,
                background: config.background.to_array(),
                fonts: self.text.fonts().fingerprint(),
            },
        })
    }
}

fn decode_asset(
    bytes: &[u8],
    id: AssetId,
    limits: RenderAssetLimits,
) -> Result<RenderedImage, RenderError> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return RenderedImage::from_png(bytes, limits.png).map_err(|error| {
            RenderError::AssetDecode {
                asset: id,
                message: error.to_string(),
            }
        });
    }

    let svg = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    let options = resvg::usvg::Options::default();
    let tree =
        resvg::usvg::Tree::from_data(svg, &options).map_err(|error| RenderError::AssetDecode {
            asset: id,
            message: format!("the SVG parser rejected the asset: {error}"),
        })?;
    let dimensions = tree.size().to_int_size();
    let width = dimensions.width();
    let height = dimensions.height();
    let pixels = u64::from(width) * u64::from(height);
    if width == 0
        || height == 0
        || width > limits.max_svg_width
        || height > limits.max_svg_height
        || pixels > limits.max_svg_pixels
    {
        return Err(RenderError::AssetDecode {
            asset: id,
            message: format!("SVG output dimensions {width}x{height} exceed the configured limits"),
        });
    }
    let mut pixmap =
        resvg::tiny_skia::Pixmap::new(width, height).ok_or_else(|| RenderError::AssetDecode {
            asset: id,
            message: "could not allocate the bounded SVG output surface".to_owned(),
        })?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::default(),
        &mut pixmap.as_mut(),
    );
    RenderedImage::from_rgba(width, height, unpremultiply(pixmap.data()))
}

fn draw_image(
    context: &mut RenderContext,
    image: &RenderedImage,
    size: (f64, f64),
    world: &swotvibe_core::Transform,
    pixel_scale: f64,
) {
    use vello_cpu::ImageSource;
    use vello_cpu::peniko::{
        Blob, ImageAlphaType, ImageBrush, ImageData, ImageFormat, ImageSampler,
    };

    let data = ImageData {
        data: Blob::new(Arc::new(image.rgba().to_vec())),
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::Alpha,
        width: image.width(),
        height: image.height(),
    };
    let brush = ImageBrush {
        image: ImageSource::from_peniko_image_data(&data),
        sampler: ImageSampler::default(),
    };
    context.set_paint(brush);
    context.set_transform(affine_of(world).then_scale(pixel_scale));
    context.fill_rect(&KurboRect::new(0.0, 0.0, size.0, size.1));
}

impl VelloCpuRenderer {
    /// Shapes and draws one text node.
    #[allow(clippy::too_many_arguments)]
    fn draw_text(
        &self,
        context: &mut RenderContext,
        node: &Node,
        text: &swotvibe_core::TextProps,
        size: (f64, f64),
        world: &swotvibe_core::Transform,
        pixel_scale: f64,
        diagnostics: &mut Vec<RenderDiagnostic>,
    ) -> Result<(), RenderError> {
        let request = TextRequest {
            text: &text.content,
            font_family: &text.font_family,
            font_size: text.font_size.get(),
            font_weight: text.font_weight,
            direction: text.direction,
            align: text.align,
            // A hug width was measured unwrapped, so it is drawn unwrapped. Any
            // other width was laid out inside a box, and the glyphs are wrapped
            // at that box.
            max_width: if node.props.width_sizing == Sizing::Hug {
                None
            } else {
                Some(size.0)
            },
        };
        let shaped: Arc<ShapedText> =
            self.text
                .shape(&request)
                .map_err(|error| RenderError::Text {
                    node: node.id,
                    message: error.to_string(),
                })?;

        let Some(fill) = node.props.fill else {
            diagnostics.push(RenderDiagnostic::NoPaint { node: node.id });
            return Ok(());
        };

        let mut path = BezPath::new();
        let mut line_top = 0.0f64;
        for line in &shaped.lines {
            for run in &line.runs {
                if run.synthesis.bold || run.synthesis.italic {
                    diagnostics.push(RenderDiagnostic::SynthesizedStyle {
                        node: node.id,
                        family: text.font_family.clone(),
                        bold: run.synthesis.bold,
                        italic: run.synthesis.italic,
                    });
                }
                for glyph in &run.glyphs {
                    let Some(outline) = self
                        .text
                        .outline(run.font, glyph.id, run.font_size)
                        .map_err(|error| RenderError::Outline {
                            node: node.id,
                            message: error.to_string(),
                        })?
                    else {
                        continue;
                    };
                    append_glyph(
                        &mut path,
                        &outline,
                        glyph.x + line.offset,
                        line_top + line.baseline + glyph.y,
                    );
                }
            }
            line_top += line.height;
        }

        if !path.elements().is_empty() {
            context.set_paint(to_alpha_color(fill));
            context.set_transform(affine_of(world).then_scale(pixel_scale));
            context.fill_path(&path);
        }
        Ok(())
    }
}

/// Appends a glyph outline to `path`, moved to the pen position.
///
/// Skrifa emits font outlines in the font coordinate system, where positive Y
/// points up from the baseline. Parley positions glyph origins in the layout
/// coordinate system, where positive Y points down. Convert only the outline's
/// local Y coordinates here; applying a reflection to the whole scene would
/// also invert the glyph's pen positions and every non-text node.
fn append_glyph(path: &mut BezPath, outline: &GlyphOutline, x: f64, y: f64) {
    use vello_cpu::kurbo::Point;
    for command in &outline.commands {
        match *command {
            PathCommand::MoveTo { x: px, y: py } => path.move_to(Point::new(x + px, y - py)),
            PathCommand::LineTo { x: px, y: py } => path.line_to(Point::new(x + px, y - py)),
            PathCommand::QuadTo {
                cx,
                cy,
                x: px,
                y: py,
            } => path.quad_to(Point::new(x + cx, y - cy), Point::new(x + px, y - py)),
            PathCommand::CubicTo {
                c1x,
                c1y,
                c2x,
                c2y,
                x: px,
                y: py,
            } => path.curve_to(
                Point::new(x + c1x, y - c1y),
                Point::new(x + c2x, y - c2y),
                Point::new(x + px, y - py),
            ),
            PathCommand::Close => path.close_path(),
            // `PathCommand` is non-exhaustive: a future command that this build
            // cannot draw is skipped rather than mis-drawn.
            _ => {}
        }
    }
}

/// The path of a shape's geometry inside the node's box.
fn shape_path(geometry: &swotvibe_core::ShapeProps, size: (f64, f64)) -> BezPath {
    match geometry.geometry {
        ShapeGeometry::Rect => {
            let rect = KurboRect::new(0.0, 0.0, size.0, size.1);
            let radius = geometry
                .corner_radius
                .get()
                .min(size.0 / 2.0)
                .min(size.1 / 2.0);
            if radius > 0.0 {
                RoundedRect::from_rect(rect, radius).to_path(PATH_TOLERANCE)
            } else {
                rect.to_path(PATH_TOLERANCE)
            }
        }
        ShapeGeometry::Ellipse => Ellipse::new(
            vello_cpu::kurbo::Point::new(size.0 / 2.0, size.1 / 2.0),
            (size.0 / 2.0, size.1 / 2.0),
            0.0,
        )
        .to_path(PATH_TOLERANCE),
    }
}

/// Fills and strokes a path from a node's paint, reporting a node with none.
fn paint(
    context: &mut RenderContext,
    node: &Node,
    path: &BezPath,
    diagnostics: &mut Vec<RenderDiagnostic>,
) {
    if node.props.fill.is_none() && node.props.stroke.is_none() {
        diagnostics.push(RenderDiagnostic::NoPaint { node: node.id });
        return;
    }
    if let Some(fill) = node.props.fill {
        context.set_paint(to_alpha_color(fill));
        context.fill_path(path);
    }
    if let Some(stroke) = node.props.stroke {
        context.set_paint(to_alpha_color(stroke.color));
        context.set_stroke(vello_cpu::kurbo::Stroke::new(stroke.width.get()));
        context.stroke_path(path);
    }
}

/// A document transform as a kurbo affine.
fn affine_of(transform: &swotvibe_core::Transform) -> Affine {
    let [a, b, c, d, e, f] = transform.coefficients();
    Affine::new([a, b, c, d, e, f])
}

fn to_alpha_color(color: swotvibe_core::Color) -> AlphaColor<Srgb> {
    AlphaColor::from_rgba8(color.r, color.g, color.b, color.a)
}

/// Converts premultiplied RGBA8 into straight-alpha RGBA8.
///
/// A fully transparent pixel is left at zero: dividing by a zero alpha would
/// invent a colour that no one can see, and a comparison should not depend on
/// which colour was invented.
fn unpremultiply(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    for pixel in bytes.as_chunks::<4>().0 {
        let alpha = pixel[3];
        if alpha == 0 {
            out.extend_from_slice(&[0, 0, 0, 0]);
            continue;
        }
        for channel in &pixel[..3] {
            let value = (u32::from(*channel) * 255 + u32::from(alpha) / 2) / u32::from(alpha);
            out.push(value.min(255) as u8);
        }
        out.push(alpha);
    }
    out
}

/// Compares two images with the documented golden rule.
///
/// Exposed so a reference test and a future diff view use one comparison rather
/// than two approximations of it.
#[must_use]
pub fn compare_images(
    actual: &RenderedImage,
    reference: &RenderedImage,
    tolerance: u8,
) -> PixelDifference {
    actual.compare(reference, tolerance)
}

#[cfg(test)]
mod text_coordinate_tests {
    use super::*;
    use vello_cpu::kurbo::{PathEl, Point};

    #[test]
    fn glyph_outlines_flip_local_y_into_the_layout_coordinate_system() {
        let outline = GlyphOutline {
            commands: vec![
                PathCommand::MoveTo { x: 3.0, y: 5.0 },
                PathCommand::LineTo { x: 7.0, y: 11.0 },
                PathCommand::QuadTo {
                    cx: 13.0,
                    cy: 17.0,
                    x: 19.0,
                    y: 23.0,
                },
                PathCommand::CubicTo {
                    c1x: 29.0,
                    c1y: 31.0,
                    c2x: 37.0,
                    c2y: 41.0,
                    x: 43.0,
                    y: 47.0,
                },
                PathCommand::Close,
            ],
            contours: 1,
        };
        let mut path = BezPath::new();

        append_glyph(&mut path, &outline, 100.0, 200.0);

        assert_eq!(
            path.elements(),
            &[
                PathEl::MoveTo(Point::new(103.0, 195.0)),
                PathEl::LineTo(Point::new(107.0, 189.0)),
                PathEl::QuadTo(Point::new(113.0, 183.0), Point::new(119.0, 177.0)),
                PathEl::CurveTo(
                    Point::new(129.0, 169.0),
                    Point::new(137.0, 159.0),
                    Point::new(143.0, 153.0)
                ),
                PathEl::ClosePath,
            ]
        );
    }
}
