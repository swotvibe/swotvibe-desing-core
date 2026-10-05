//! # swotvibe-render
//!
//! The render adapter contract.
//!
//! Ownership: scene extraction from a document and a layout result, the pixel
//! output, bounded PNG/SVG asset decoding, and the reference-image fingerprint.
//! It never writes back to the document. The concrete Vello CPU backend remains
//! a temporary M0 choice, not a final renderer decision.
//!
//! ## Service contract
//!
//! - **Input:** an immutable [`Snapshot`](swotvibe_core::Snapshot), a
//!   [`LayoutResult`](swotvibe_layout::LayoutResult), a pixel scale, and a
//!   colour configuration.
//! - **Output:** an [`RenderedImage`] in straight-alpha RGBA8, the omissions and
//!   substitutions it made as [`RenderDiagnostic`]s, and a
//!   [`RenderFingerprint`] naming the backend, the fonts, and the scale.
//! - **Dependency rule:** there is no reverse dependency from the model to the
//!   renderer, and a backend, font, or colour difference is part of the
//!   reference-image fingerprint rather than a detail of the comparison.
//!
//! ## Determinism
//!
//! A reference image is only useful if the same document produces the same
//! pixels. This crate pins what it can: the SIMD level, the render mode, a
//! fixed path tolerance, unhinted glyph outlines, and a single-threaded
//! pipeline. What it cannot pin — a different backend version — is recorded in
//! the fingerprint instead of being silently absorbed.
//!
//! ## Status (temporary M0 backend)
//!
//! [`VelloCpuRenderer`] wraps vello_cpu 0.3 behind [`Renderer`]. The choice is
//! **provisional** and is recorded in `docs/adr/0008-render-backend-vello-cpu.md`
//! with its build result, license, and known limits. The final backend decision
//! (`DEC-RENDERER`: Skia vs Vello CPU vs Vello GPU/wgpu) stays open.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod contract;
mod vello_engine;

pub use contract::{
    MAX_PIXELS_PER_SIDE, PixelDifference, PngDecodeLimits, RenderAssetLimits, RenderAssets,
    RenderConfig, RenderDiagnostic, RenderError, RenderFingerprint, RenderedImage, RenderedPage,
    Renderer, engine_id,
};
pub use vello_engine::{VelloCpuRenderer, compare_images};

/// The renderer and version the temporary M0 backend uses, with the pipeline it
/// is configured for.
///
/// Kept as a constant so a fingerprint states the backend it was produced with
/// rather than an inferred crate version. The dependency is pinned exactly in
/// `Cargo.toml`, so this string and the library cannot drift without a
/// deliberate edit here.
pub const VELLO_ENGINE_ID: &str = "vello_cpu 0.3.0 (f32 pipeline, baseline SIMD)";
