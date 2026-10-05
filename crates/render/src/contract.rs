//! The render adapter contract: the configuration, the pixels, and the trait.
//!
//! Ownership: scene extraction from a layout result, invalidation inputs, and
//! the reference-image fingerprint. Not owned: building a rasterizer, and never
//! writing back to the document.

use std::fmt;

use swotvibe_core::{Color, NodeId, PageId, Snapshot};
use swotvibe_layout::LayoutResult;
use swotvibe_text::FontFingerprint;

use crate::VELLO_ENGINE_ID;

/// The largest pixel dimension a render may ask for.
///
/// A bound exists so a mistaken scale cannot allocate an unbounded buffer. It is
/// far above any preview or export the product needs today.
pub const MAX_PIXELS_PER_SIDE: u32 = 8192;

/// Resource budget for decoding one PNG image.
///
/// The caller supplies every limit because a golden image, a toolbar icon, and
/// an imported product image have different legitimate sizes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PngDecodeLimits {
    /// Maximum compressed input size in bytes.
    pub max_input_bytes: usize,
    /// Maximum width in pixels.
    pub max_width: u32,
    /// Maximum height in pixels.
    pub max_height: u32,
    /// Maximum decoded pixel count.
    pub max_pixels: u64,
    /// Maximum output allocation in bytes, including RGBA conversion.
    pub max_output_bytes: usize,
    /// Maximum temporary decoder allocations. The PNG crate treats this as a
    /// best-effort bound and does not include caller-owned buffers.
    pub max_decoder_bytes: usize,
}

impl PngDecodeLimits {
    fn check(self) -> Result<Self, RenderError> {
        if self.max_input_bytes == 0
            || self.max_width == 0
            || self.max_height == 0
            || self.max_pixels == 0
            || self.max_output_bytes == 0
            || self.max_decoder_bytes == 0
        {
            return Err(RenderError::InvalidConfig {
                message: "PNG decode limits must all be greater than zero".to_owned(),
            });
        }
        Ok(self)
    }
}

/// Budgets for decoding bitmap and SVG resources used by a render pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderAssetLimits {
    /// Maximum encoded size of one asset in bytes.
    pub max_asset_bytes: usize,
    /// Maximum SVG output width in pixels.
    pub max_svg_width: u32,
    /// Maximum SVG output height in pixels.
    pub max_svg_height: u32,
    /// Maximum SVG output pixel count.
    pub max_svg_pixels: u64,
    /// Explicit bounds for embedded PNG decoding.
    pub png: PngDecodeLimits,
}

impl RenderAssetLimits {
    fn check(self) -> Result<Self, RenderError> {
        if self.max_asset_bytes == 0
            || self.max_svg_width == 0
            || self.max_svg_height == 0
            || self.max_svg_pixels == 0
            || self.png.max_input_bytes > self.max_asset_bytes
        {
            return Err(RenderError::InvalidConfig {
                message: "render asset limits are zero or inconsistent".to_owned(),
            });
        }
        self.png.check()?;
        Ok(self)
    }
}

/// Encoded image bytes available to one renderer instance, keyed by asset ID.
#[derive(Debug, Clone)]
pub struct RenderAssets {
    limits: RenderAssetLimits,
    payloads: std::collections::BTreeMap<swotvibe_core::AssetId, Vec<u8>>,
}

impl RenderAssets {
    /// Creates an empty asset map with explicit decoding limits.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::InvalidConfig`] when any limit is zero or the PNG
    /// input limit exceeds the per-asset limit.
    pub fn new(limits: RenderAssetLimits) -> Result<Self, RenderError> {
        Ok(Self {
            limits: limits.check()?,
            payloads: std::collections::BTreeMap::new(),
        })
    }

    /// Adds one encoded PNG or SVG resource.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::AssetDecode`] if the encoded bytes exceed the
    /// configured per-asset bound.
    pub fn insert(
        &mut self,
        id: swotvibe_core::AssetId,
        bytes: Vec<u8>,
    ) -> Result<(), RenderError> {
        if bytes.len() > self.limits.max_asset_bytes {
            return Err(RenderError::AssetDecode {
                asset: id,
                message: format!(
                    "encoded asset has {} bytes, above the {}-byte limit",
                    bytes.len(),
                    self.limits.max_asset_bytes
                ),
            });
        }
        self.payloads.insert(id, bytes);
        Ok(())
    }

    pub(crate) const fn limits(&self) -> RenderAssetLimits {
        self.limits
    }

    pub(crate) fn get(&self, id: swotvibe_core::AssetId) -> Option<&[u8]> {
        self.payloads.get(&id).map(Vec::as_slice)
    }
}

/// How to rasterize a page.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderConfig {
    /// The output width in pixels.
    pub width: u32,
    /// The output height in pixels.
    pub height: u32,
    /// Pixels per design unit.
    pub scale: f64,
    /// The opaque backdrop, composited under the scene.
    pub background: Color,
}

impl RenderConfig {
    /// A configuration for a page of `page_size` design units at `scale`.
    #[must_use]
    pub fn for_page(page_size: (f64, f64), scale: f64, background: Color) -> Self {
        let width = (page_size.0 * scale).round().max(1.0) as u32;
        let height = (page_size.1 * scale).round().max(1.0) as u32;
        Self {
            width,
            height,
            scale,
            background,
        }
    }

    /// Checks the configuration.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::InvalidConfig`] for a zero or oversized pixel
    /// dimension, or a scale that is not finite and positive.
    pub fn check(&self) -> Result<(), RenderError> {
        if self.width == 0 || self.height == 0 {
            return Err(RenderError::InvalidConfig {
                message: format!(
                    "the output size {}x{} has a zero side",
                    self.width, self.height
                ),
            });
        }
        if self.width > MAX_PIXELS_PER_SIDE || self.height > MAX_PIXELS_PER_SIDE {
            return Err(RenderError::InvalidConfig {
                message: format!(
                    "the output size {}x{} exceeds the {MAX_PIXELS_PER_SIDE}-pixel limit",
                    self.width, self.height
                ),
            });
        }
        if !self.scale.is_finite() || self.scale <= 0.0 {
            return Err(RenderError::InvalidConfig {
                message: format!("the scale {} is not finite and positive", self.scale),
            });
        }
        Ok(())
    }

    /// The largest design-unit extent this configuration covers.
    #[must_use]
    pub fn design_extent(&self) -> (f64, f64) {
        (
            f64::from(self.width) / self.scale,
            f64::from(self.height) / self.scale,
        )
    }
}

/// An image in **straight-alpha** RGBA8, row-major from the top-left.
///
/// The backend rasterizes premultiplied, and this type converts at the boundary
/// because that is the form a file format, a pixel comparison, and a human all
/// expect. The conversion is lossy for a partly transparent pixel and is done
/// once, here, rather than being re-derived by each caller.
#[derive(Clone, PartialEq, Eq)]
pub struct RenderedImage {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

impl RenderedImage {
    /// Builds an image from straight-alpha RGBA8 bytes.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::InvalidConfig`] when the buffer length does not
    /// match `width * height * 4`.
    pub fn from_rgba(width: u32, height: u32, rgba: Vec<u8>) -> Result<Self, RenderError> {
        let expected = (width as usize) * (height as usize) * 4;
        if rgba.len() != expected {
            return Err(RenderError::InvalidConfig {
                message: format!(
                    "a {width}x{height} image needs {expected} bytes, found {}",
                    rgba.len()
                ),
            });
        }
        Ok(Self {
            width,
            height,
            rgba,
        })
    }

    /// The width in pixels.
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// The height in pixels.
    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// The straight-alpha RGBA8 bytes.
    #[must_use]
    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }

    /// The colour of one pixel, or `None` when the coordinates are outside.
    #[must_use]
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let offset = ((y as usize) * (self.width as usize) + (x as usize)) * 4;
        Some([
            self.rgba[offset],
            self.rgba[offset + 1],
            self.rgba[offset + 2],
            self.rgba[offset + 3],
        ])
    }

    /// Encodes the image as an 8-bit RGBA PNG.
    ///
    /// The encoder is deterministic: the same bytes always produce the same
    /// file, so a committed reference image is comparable as bytes as well as by
    /// pixel.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::Png`] when the encoder refuses the data.
    pub fn to_png(&self) -> Result<Vec<u8>, RenderError> {
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, self.width, self.height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().map_err(png_error)?;
            writer.write_image_data(&self.rgba).map_err(png_error)?;
        }
        Ok(bytes)
    }

    /// Decodes an 8- or 16-bit PNG into straight-alpha RGBA8 under caller limits.
    ///
    /// A greyscale or palette input is expanded, so a reference image can be
    /// authored by any tool. Input length, declared dimensions, pixel count,
    /// output allocation, and decoder working allocation are checked before
    /// the corresponding image buffer is allocated.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::Png`] for malformed or over-budget data and
    /// [`RenderError::InvalidConfig`] for a zero-valued limit.
    pub fn from_png(bytes: &[u8], limits: PngDecodeLimits) -> Result<Self, RenderError> {
        let limits = limits.check()?;
        if bytes.len() > limits.max_input_bytes {
            return Err(png_error(format!(
                "the input has {} bytes, above the {}-byte limit",
                bytes.len(),
                limits.max_input_bytes
            )));
        }
        let mut decoder = png::Decoder::new_with_limits(
            std::io::Cursor::new(bytes),
            png::Limits {
                bytes: limits.max_decoder_bytes,
            },
        );
        let header = decoder.read_header_info().map_err(png_error)?;
        let width = header.width;
        let height = header.height;
        let pixels = u64::from(width) * u64::from(height);
        if width == 0
            || height == 0
            || width > limits.max_width
            || height > limits.max_height
            || pixels > limits.max_pixels
        {
            return Err(png_error(format!(
                "the image dimensions {width}x{height} exceed the configured limits"
            )));
        }
        let rgba_size = usize::try_from(pixels)
            .ok()
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or_else(|| png_error("the decoded image size overflows this platform"))?;
        if rgba_size > limits.max_output_bytes {
            return Err(png_error(format!(
                "the RGBA output needs {rgba_size} bytes, above the {}-byte limit",
                limits.max_output_bytes
            )));
        }
        let mut reader = decoder.read_info().map_err(png_error)?;
        let Some(buffer_size) = reader.output_buffer_size() else {
            return Err(RenderError::Png {
                message: "the PNG holds no image data".to_owned(),
            });
        };
        if buffer_size > limits.max_output_bytes {
            return Err(png_error(format!(
                "the decoder buffer needs {buffer_size} bytes, above the {}-byte limit",
                limits.max_output_bytes
            )));
        }
        let mut buffer = vec![0u8; buffer_size];
        let info = reader.next_frame(&mut buffer).map_err(png_error)?;
        let width = info.width;
        let height = info.height;
        let source = &buffer[..info.buffer_size()];

        let rgba = match info.color_type {
            png::ColorType::Rgba => source.to_vec(),
            png::ColorType::Rgb => source
                .as_chunks::<3>()
                .0
                .iter()
                .flat_map(|pixel| [pixel[0], pixel[1], pixel[2], 255])
                .collect(),
            png::ColorType::Grayscale => source
                .as_chunks::<1>()
                .0
                .iter()
                .flat_map(|pixel| [pixel[0], pixel[0], pixel[0], 255])
                .collect(),
            png::ColorType::GrayscaleAlpha => source
                .as_chunks::<2>()
                .0
                .iter()
                .flat_map(|pixel| [pixel[0], pixel[0], pixel[0], pixel[1]])
                .collect(),
            png::ColorType::Indexed => {
                return Err(RenderError::Png {
                    message: "an indexed PNG must be expanded before it is read".to_owned(),
                });
            }
        };
        // A 16-bit source is widened to 8 bits by dropping the low byte, which
        // is what an 8-bit reference image is compared at.
        let rgba = match info.bit_depth {
            png::BitDepth::Eight => rgba,
            png::BitDepth::Sixteen => rgba
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| pair[0])
                .collect::<Vec<u8>>(),
            other => {
                return Err(RenderError::Png {
                    message: format!("the PNG bit depth {other:?} is not supported"),
                });
            }
        };
        Self::from_rgba(width, height, rgba)
    }

    /// The number of pixels that differ by more than `tolerance` in any channel,
    /// with the largest observed channel difference.
    ///
    /// A comparison is never exact: an anti-aliased edge differs by a unit or two
    /// between two runs of the same pipeline, and the documented rule is a
    /// tolerance with a count, not byte equality (see `tests/golden/README.md`).
    #[must_use]
    pub fn compare(&self, other: &Self, tolerance: u8) -> PixelDifference {
        if self.width != other.width || self.height != other.height {
            return PixelDifference {
                size_mismatch: true,
                deviating_pixels: 0,
                worst_channel_delta: 255,
            };
        }
        let mut deviating_pixels = 0usize;
        let mut worst_channel_delta = 0u8;
        for (a, b) in self
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .zip(other.rgba.as_chunks::<4>().0)
        {
            let mut worst = 0u8;
            for channel in 0..4 {
                let delta = a[channel].abs_diff(b[channel]);
                worst = worst.max(delta);
            }
            if worst > tolerance {
                deviating_pixels += 1;
            }
            worst_channel_delta = worst_channel_delta.max(worst);
        }
        PixelDifference {
            size_mismatch: false,
            deviating_pixels,
            worst_channel_delta,
        }
    }
}

/// The result of comparing two images.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelDifference {
    /// Whether the two images had different dimensions.
    pub size_mismatch: bool,
    /// The number of pixels that differed by more than the tolerance.
    pub deviating_pixels: usize,
    /// The largest difference found in any channel, whatever the tolerance.
    pub worst_channel_delta: u8,
}

impl PixelDifference {
    /// Whether the images match within the tolerance.
    #[must_use]
    pub const fn is_match(&self) -> bool {
        !self.size_mismatch && self.deviating_pixels == 0
    }
}

impl fmt::Debug for RenderedImage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The pixel buffer is not printed: a failing assertion about a
        // rasterized page is unreadable if it dumps megabytes of bytes.
        f.debug_struct("RenderedImage")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("bytes", &self.rgba.len())
            .finish()
    }
}

/// Something the renderer did not draw exactly.
///
/// As with layout, a diagnostic is not a failure: it names what the renderer
/// left out or substituted, so a reference image never hides a missing feature.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum RenderDiagnostic {
    /// A node's style asks for a face the pinned font does not ship, so the
    /// backend would have to synthesize it. The M0 renderer draws the shipped
    /// outlines and reports the synthesis instead of pretending it applied it.
    SynthesizedStyle {
        /// The node.
        node: NodeId,
        /// The family the run was shaped with.
        family: String,
        /// Whether a synthesized bold was requested.
        bold: bool,
        /// Whether a synthesized italic was requested.
        italic: bool,
    },
    /// An image node refers to an asset this renderer cannot resolve to pixels.
    UnresolvedImage {
        /// The node.
        node: NodeId,
        /// The asset identity, when the node names one.
        asset: Option<String>,
    },
    /// A node has no fill and no stroke, so it contributes no geometry.
    NoPaint {
        /// The node.
        node: NodeId,
    },
}

impl fmt::Display for RenderDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SynthesizedStyle {
                node,
                family,
                bold,
                italic,
            } => write!(
                f,
                "{node} asks for a synthesized style of `{family}` (bold: {bold}, italic: {italic})"
            ),
            Self::UnresolvedImage { node, asset } => match asset {
                Some(asset) => write!(
                    f,
                    "{node} refers to asset {asset}, which has no pixels in M0"
                ),
                None => write!(f, "{node} is an image with no asset"),
            },
            Self::NoPaint { node } => write!(f, "{node} has neither a fill nor a stroke"),
        }
    }
}

/// Why a render failed.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum RenderError {
    /// The configuration is not usable.
    InvalidConfig {
        /// A human-readable explanation.
        message: String,
    },
    /// The page is not in the snapshot.
    PageNotFound {
        /// The page that was requested.
        page: PageId,
    },
    /// The layout result does not cover a node the scene needs.
    MissingLayout {
        /// The node without geometry.
        node: NodeId,
    },
    /// Shaping a node's text failed.
    Text {
        /// The node whose text could not be shaped.
        node: NodeId,
        /// A human-readable explanation from the text engine.
        message: String,
    },
    /// A glyph outline could not be read.
    Outline {
        /// The node whose glyph failed.
        node: NodeId,
        /// A human-readable explanation.
        message: String,
    },
    /// Encoding or decoding an image failed.
    Png {
        /// A human-readable explanation.
        message: String,
    },
    /// An embedded PNG or SVG could not be decoded within its resource budget.
    AssetDecode {
        /// The asset identity.
        asset: swotvibe_core::AssetId,
        /// A human-readable explanation.
        message: String,
    },
    /// The backend refused the scene.
    Backend {
        /// A human-readable explanation.
        message: String,
    },
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfig { message } => write!(f, "invalid render configuration: {message}"),
            Self::PageNotFound { page } => write!(f, "page {page} is not in the document"),
            Self::MissingLayout { node } => write!(f, "node {node} has no layout result"),
            Self::Text { node, message } => {
                write!(f, "text on {node} could not be shaped: {message}")
            }
            Self::Outline { node, message } => {
                write!(f, "a glyph on {node} could not be drawn: {message}")
            }
            Self::Png { message } => write!(f, "the image codec failed: {message}"),
            Self::AssetDecode { asset, message } => {
                write!(f, "image asset {asset} could not be decoded: {message}")
            }
            Self::Backend { message } => write!(f, "the render backend failed: {message}"),
        }
    }
}

impl std::error::Error for RenderError {}

fn png_error(error: impl fmt::Display) -> RenderError {
    RenderError::Png {
        message: error.to_string(),
    }
}

/// What an image was produced from.
///
/// A reference image is only comparable against the backend, the fonts, the
/// scale, and the colour handling it was produced with. Every field here is part
/// of the comparison, which is why the fingerprint is written beside the image
/// rather than inferred later.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderFingerprint {
    /// The renderer and its version, including the pipeline it uses.
    pub engine: String,
    /// The layout engine the geometry came from.
    pub layout_engine: String,
    /// The output size in pixels.
    pub pixels: (u32, u32),
    /// Pixels per design unit.
    pub scale_bits: u64,
    /// The scale as a number, for a readable report.
    pub scale: f64,
    /// The opaque backdrop, as `[r, g, b, a]`.
    pub background: [u8; 4],
    /// The fonts the text was shaped and drawn with.
    pub fonts: Vec<FontFingerprint>,
}

/// A rendered page.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderedPage {
    /// The pixels.
    pub image: RenderedImage,
    /// What the renderer left out or substituted.
    pub diagnostics: Vec<RenderDiagnostic>,
    /// What the image was produced from.
    pub fingerprint: RenderFingerprint,
}

/// Turning a laid out page into pixels, behind one adapter-neutral contract.
pub trait Renderer {
    /// The identifier of this renderer, including its version and the pipeline
    /// it uses, for a fingerprint.
    fn engine_id(&self) -> &'static str;

    /// Rasterizes one page.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError`] for an unusable configuration, a page or node the
    /// inputs do not describe, a text or outline failure, or a backend failure.
    fn render(
        &self,
        snapshot: &Snapshot,
        page: PageId,
        layout: &LayoutResult,
        config: &RenderConfig,
    ) -> Result<RenderedPage, RenderError>;
}

/// The renderer identifier a fingerprint records for the temporary M0 backend.
#[must_use]
pub fn engine_id() -> &'static str {
    VELLO_ENGINE_ID
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(fill: [u8; 4]) -> RenderedImage {
        RenderedImage::from_rgba(2, 1, fill.repeat(2)).expect("a 2x1 image")
    }

    #[test]
    fn an_image_of_the_wrong_length_is_refused() {
        assert!(RenderedImage::from_rgba(2, 2, vec![0; 8]).is_err());
        assert!(RenderedImage::from_rgba(2, 2, vec![0; 16]).is_ok());
    }

    #[test]
    fn pixels_are_readable_by_coordinate() {
        let image = RenderedImage::from_rgba(2, 1, vec![1, 2, 3, 4, 5, 6, 7, 8]).expect("image");
        assert_eq!(image.pixel(0, 0), Some([1, 2, 3, 4]));
        assert_eq!(image.pixel(1, 0), Some([5, 6, 7, 8]));
        assert_eq!(image.pixel(2, 0), None);
        assert_eq!(image.pixel(0, 1), None);
    }

    #[test]
    fn comparison_counts_only_pixels_beyond_the_tolerance() {
        let a = image([10, 10, 10, 255]);
        let b = image([12, 10, 10, 255]);
        assert!(a.compare(&b, 3).is_match());
        assert!(!a.compare(&b, 1).is_match());
        let difference = a.compare(&b, 1);
        assert_eq!(difference.deviating_pixels, 2);
        assert_eq!(difference.worst_channel_delta, 2);
    }

    #[test]
    fn a_size_mismatch_is_never_a_match() {
        let a = image([0, 0, 0, 255]);
        let b = RenderedImage::from_rgba(1, 1, vec![0, 0, 0, 255]).expect("image");
        let difference = a.compare(&b, 255);
        assert!(difference.size_mismatch);
        assert!(!difference.is_match());
    }

    #[test]
    fn a_png_round_trip_preserves_every_pixel() {
        let original = RenderedImage::from_rgba(
            2,
            2,
            vec![
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 10, 20, 30, 40,
            ],
        )
        .expect("image");
        let bytes = original.to_png().expect("encode");
        let decoded = RenderedImage::from_png(
            &bytes,
            PngDecodeLimits {
                max_input_bytes: 1024,
                max_width: 2,
                max_height: 2,
                max_pixels: 4,
                max_output_bytes: 16,
                max_decoder_bytes: 1024,
            },
        )
        .expect("decode");
        assert_eq!(decoded, original);
        // Encoding is deterministic, so a reference file is comparable as bytes.
        assert_eq!(decoded.to_png().expect("encode"), bytes);
    }

    fn png_limits() -> PngDecodeLimits {
        PngDecodeLimits {
            max_input_bytes: 1024,
            max_width: 2,
            max_height: 2,
            max_pixels: 4,
            max_output_bytes: 16,
            max_decoder_bytes: 1024,
        }
    }

    #[test]
    fn png_decode_refuses_input_over_the_callers_byte_limit() {
        let image = RenderedImage::from_rgba(2, 2, vec![255; 16]).expect("image");
        let bytes = image.to_png().expect("encode");
        let error = RenderedImage::from_png(
            &bytes,
            PngDecodeLimits {
                max_input_bytes: bytes.len() - 1,
                ..png_limits()
            },
        )
        .expect_err("an oversized input must be rejected");
        assert!(matches!(error, RenderError::Png { .. }));
    }

    #[test]
    fn png_decode_refuses_dimensions_over_the_callers_limit_before_allocating_pixels() {
        let image = RenderedImage::from_rgba(2, 2, vec![255; 16]).expect("image");
        let bytes = image.to_png().expect("encode");
        let error = RenderedImage::from_png(
            &bytes,
            PngDecodeLimits {
                max_width: 1,
                ..png_limits()
            },
        )
        .expect_err("an oversized dimension must be rejected");
        assert!(matches!(error, RenderError::Png { .. }));
    }

    #[test]
    fn png_decode_refuses_output_over_the_callers_memory_limit() {
        let image = RenderedImage::from_rgba(2, 2, vec![255; 16]).expect("image");
        let bytes = image.to_png().expect("encode");
        let error = RenderedImage::from_png(
            &bytes,
            PngDecodeLimits {
                max_output_bytes: 15,
                ..png_limits()
            },
        )
        .expect_err("the RGBA buffer would exceed the budget");
        assert!(matches!(error, RenderError::Png { .. }));
    }

    #[test]
    fn png_decode_refuses_zero_limits() {
        let image = RenderedImage::from_rgba(2, 2, vec![255; 16]).expect("image");
        let bytes = image.to_png().expect("encode");
        let error = RenderedImage::from_png(
            &bytes,
            PngDecodeLimits {
                max_pixels: 0,
                ..png_limits()
            },
        )
        .expect_err("zero limits are ambiguous");
        assert!(matches!(error, RenderError::InvalidConfig { .. }));
    }

    #[test]
    fn a_configuration_is_validated() {
        let good = RenderConfig {
            width: 100,
            height: 100,
            scale: 1.0,
            background: Color::rgb(255, 255, 255),
        };
        assert_eq!(good.check(), Ok(()));
        assert!(RenderConfig { width: 0, ..good }.check().is_err());
        assert!(
            RenderConfig {
                width: MAX_PIXELS_PER_SIDE + 1,
                ..good
            }
            .check()
            .is_err()
        );
        assert!(RenderConfig { scale: 0.0, ..good }.check().is_err());
    }

    #[test]
    fn the_design_extent_is_the_pixel_size_divided_by_the_scale() {
        let config = RenderConfig {
            width: 480,
            height: 320,
            scale: 2.0,
            background: Color::rgb(0, 0, 0),
        };
        assert_eq!(config.design_extent(), (240.0, 160.0));
        assert_eq!(
            RenderConfig::for_page((400.0, 240.0), 2.0, Color::rgb(0, 0, 0)).width,
            800
        );
    }
}
