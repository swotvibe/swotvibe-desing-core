//! The text adapter contract: measurement, shaping, and glyph outlines.
//!
//! Ownership: the measurement and shaping contract, and the glyph information
//! the editor and the renderer need. Not owned: rasterization, and never
//! owning an OS-specific font file inside the document.
//!
//! ## Service contract
//!
//! - **Input:** text, resolved style (family, size, weight, direction,
//!   alignment), and a wrap constraint in design units.
//! - **Output:** line metrics, baselines, and positioned glyphs, plus outlines
//!   for the renderer.
//! - **Dependency rule:** reference tests never depend on implicit system
//!   fonts. [`ParleyTextEngine`] is built from an explicit [`FontSet`], and the
//!   font discovery that would read the host's font directories is off. A font
//!   or text change changes the result, which is why a result carries the font
//!   fingerprint it was produced from ([`FontFingerprint`]).
//!
//! ## Boundaries
//!
//! The engine speaks design units only. It never converts to pixels: a renderer
//! applies the device scale, and the layout adapter only asks for a measurement
//! in the same units it lays out in.
//!
//! Outlines cross the boundary as [`PathCommand`] values rather than as a
//! backend path type, so the renderer can translate them into whatever curve
//! representation its backend uses without this crate knowing about it.
//!
//! ## Status (temporary M0 backend)
//!
//! [`ParleyTextEngine`] wraps Parley 0.11 behind [`TextLayoutEngine`]. The
//! choice is **provisional**: it exists so the M0 vertical slice can measure and
//! render real text, and it is recorded in
//! `docs/adr/0006-text-backend-parley.md` with its build result, license, and
//! known limits.
//!
//! Known limits of this adapter, which the M0 slice works around rather than
//! hides:
//!
//! - A paragraph's base direction cannot be forced: Parley resolves it from the
//!   text's first strong character. [`TextDirection::Ltr`] and
//!   [`TextDirection::Rtl`] therefore return
//!   [`TextError::UnsupportedDirection`] instead of being silently treated as
//!   [`TextDirection::Auto`]. [`TextDirection::Auto`] already handles the
//!   Arabic case, because an RTL paragraph resolves from its own content.
//! - Font fallback is limited to families registered in the [`FontSet`]. There
//!   are no system fonts by construction, so an unregistered family is a typed
//!   error rather than a surprise substitute.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod contract;
mod outline;
mod parley_engine;

pub use contract::{
    FontFingerprint, FontKey, FontSet, FontSource, PositionedGlyph, ShapedLine, ShapedRun,
    ShapedText, TextError, TextLayoutEngine, TextRequest, TextSynthesis,
};
pub use outline::{GlyphOutline, PathCommand, PathRecorder};
pub use parley_engine::ParleyTextEngine;
/// Re-exported because a [`TextRequest`] is expressed in these terms, so a
/// caller of this crate never has to name `swotvibe-core` to build one.
pub use swotvibe_core::{TextAlign, TextDirection};
