//! # swotvibe-layout
//!
//! The layout adapter contract.
//!
//! Ownership: translating product layout semantics (fixed/fill/hug) to and from
//! an external layout engine, and the layout result fingerprint.
//! Not owned: treating Taffy/Yoga as the document source of truth.
//!
//! An external layout engine does not own product semantics such as
//! fixed/fill/hug or text measurement. The adapter is defined and tested against
//! its contract, and the library behind it is a temporary M0 choice.
//!
//! ## Service contract
//!
//! - **Input:** an immutable [`Snapshot`](swotvibe_core::Snapshot) of a page,
//!   container constraints, and design units. Text measurement goes through
//!   [`swotvibe_text::TextLayoutEngine`], never through a second opinion about
//!   how wide a string is.
//! - **Output:** bounds per [`NodeId`](swotvibe_core::NodeId), unsupported-case
//!   diagnostics, and a result fingerprint that includes the fonts the text was
//!   measured with.
//! - **Dependency rule:** the adapter never returns a reverse reference to
//!   `DocumentEngine`, and never mutates the document. A layout result is
//!   derived and rebuildable.
//!
//! ## What layout decides, and what the transform decides
//!
//! A node's box comes from layout; the linear part of its stored transform
//! (rotation, scale, skew) is applied to that box. The translation is only used
//! where layout does not decide a position, which is inside a container with no
//! layout rule. This is the rule `NodeProps` documents, stated once so the
//! layout adapter and the renderer cannot disagree about it.
//!
//! Every number is in **design units**, and nothing is rounded: rounding is a
//! renderer's decision at its own pixel boundary.
//!
//! ## Status (temporary M0 backend)
//!
//! [`TaffyLayoutEngine`] wraps Taffy 0.14 behind [`LayoutEngine`]. The choice is
//! **provisional** and is recorded in `docs/adr/0007-layout-backend-taffy.md`
//! with its build result, license, and known limits. Engine selection
//! (`DEC-LAYOUT`) stays open.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod contract;
mod taffy_engine;

pub use contract::{
    LayoutDiagnostic, LayoutEngine, LayoutError, LayoutFingerprint, LayoutOptions, LayoutResult,
    NodeLayout, Rect, engine_id,
};
pub use taffy_engine::{TaffyLayoutEngine, UNITS};

/// The layout engine and version the temporary M0 backend uses.
///
/// Kept as a constant so a fingerprint states the engine it was measured with
/// rather than an inferred crate version. The dependency is pinned exactly in
/// `Cargo.toml`, so this string and the library cannot drift without a
/// deliberate edit here.
pub const TAFFY_ENGINE_ID: &str = "taffy 0.14.0";
