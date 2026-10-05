//! # swotvibe-text
//!
//! The text adapter contract.
//!
//! Ownership: the measurement and shaping contract, and the glyph information
//! the editor needs.
//! Not owned: owning an OS-specific font file inside the document.
//!
//! ## Service contract
//!
//! - **Input:** text, styles, direction/language, wrap constraints, and host
//!   font faces.
//! - **Output:** line metrics, baselines, and cluster/ligature positions needed
//!   for editing.
//! - **Dependency rule:** reference tests never depend on implicit system fonts;
//!   a font or text change invalidates the result.
//!
//! Parley and the Linebender stack are candidates behind `TextLayoutEngine`, not
//! a final API decision. Arabic, RTL, IME, and fallback fonts must be verified
//! first (`DEC-TEXT-AR`). No concrete trait is committed in this scaffold.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
