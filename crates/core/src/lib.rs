//! # swotvibe-core
//!
//! The core kernel of the design editor: it owns the meaning of the design
//! document, validates its integrity, applies reversible edits, and produces
//! changes that can be displayed or persisted.
//!
//! ## Scope
//!
//! The kernel is independent of the user interface, the rendering engine, the
//! filesystem, and AI providers. It must not depend on application-layer
//! crates, and it must not call Vue, Tauri, or any renderer.
//!
//! ## Module map
//!
//! Aligned with the logical modules defined in the technical specification:
//!
//! - [`ids`] — permanent identity types and their generators.
//! - [`geometry`] — finite scalars, sizes, and affine transforms (ADR-0002).
//! - [`props`] — node geometry, paint, text, image, and frame-layout properties.
//! - [`model`] — documents, pages, nodes, styles, and relations.
//! - [`commands`] — typed commands and pre-application validation.
//! - [`transaction`] — atomic application and change-set computation.
//! - [`history`] — inverse operations, undo/redo, and batch grouping.
//! - [`validation`] — document integrity rules and error reports.
//! - [`snapshot`] — immutable read views of the document.
//! - [`engine`] — the public `DocumentEngine` entry point.
//!
//! ## Status
//!
//! The M0 vertical slice is implemented and tested:
//!
//! - [`ids`] — identity newtypes and their generators.
//! - [`model`] — the document model with a canonical children/roots
//!   representation and derived indexes.
//! - [`validation`] — integrity rules, typed errors, and resource limits.
//! - [`commands`] — typed command vocabulary and stable error codes.
//! - [`transaction`] — atomic batch application and change sets.
//! - [`history`] — inverse commands, undo/redo, and batch grouping.
//! - [`snapshot`] — immutable, revision-tagged read views for adapters.
//! - [`engine`] — the public `DocumentEngine` single-writer entry point.
//!
//! The persisted schema lives in `swotvibe-format`; no layout, text, or render
//! adapter is committed until those backend decisions are made.
//!
//! ## License
//!
//! MIT. See the repository `LICENSE` file.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod commands;
pub mod engine;
pub mod geometry;
pub mod history;
pub mod ids;
pub mod model;
pub mod props;
pub mod snapshot;
pub mod transaction;
pub mod validation;

pub use commands::{Command, CommandError, CommandErrorCode, NodePlacement, Position};
pub use engine::DocumentEngine;
pub use geometry::{GeometryError, Scalar, Size, Transform};
pub use history::{
    DEFAULT_HISTORY_CAPACITY, History, HistoryEntry, HistoryError, inverse_entry,
    inverse_with_subtree,
};
pub use ids::{AssetId, DocumentId, IdError, IdKind, NodeId, PageId};
pub use model::{Asset, Document, Extensions, Node, NodeKind, Page, Revision};
pub use props::{
    Axis, Color, Content, CrossAlign, FlexLayout, FrameLayout, FrameProps, ImageProps, Insets,
    MainAlign, NodeProps, PropsError, ShapeGeometry, ShapeProps, Sizing, Stroke, TextAlign,
    TextDirection, TextProps,
};
pub use snapshot::Snapshot;
pub use transaction::{BatchError, ChangeSet, Commit, Effect, apply_batch, apply_batch_at_current};
pub use validation::{
    ElementRef, ErrorCode, ResourceLimits, RevisionConflict, ValidationError, ValidationErrors,
    validate, validate_shape, validate_untrusted,
};
