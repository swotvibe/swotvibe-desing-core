//! # swotvibe-app
//!
//! The application service: one editor session over the core kernel, plus the
//! request and response contract an interface talks to.
//!
//! ## Where this sits
//!
//! ```text
//! interface (Vue, CLI, tests)
//!      │  request/response DTOs
//!      ▼
//! swotvibe-app          <- this crate
//!      ├── swotvibe-format   persisted schema, import/export, migrations
//!      ├── swotvibe-core     document, commands, transactions, history
//!      ├── swotvibe-layout   page geometry
//!      └── swotvibe-render   pixels
//! ```
//!
//! A host — a desktop shell, a command-line tool, a test — depends on this crate
//! and never reimplements the read-modify-write around a command. Which host is
//! running does not change what an edit means.
//!
//! ## What this crate deliberately does not do
//!
//! - It does not open files. Paths, dialogs, atomic replacement, and recovery
//!   belong to a host, which owns the platform. This crate reads and produces
//!   **bytes**.
//! - It does not depend on a user-interface framework, a windowing library, or
//!   an IPC mechanism. Those are host choices.
//! - It does not store a second copy of the document. Derived results — layout
//!   and pixels — are computed per request and tagged with the revision they
//!   came from.
//! - It does not decide the renderer. [`EditorSession::renderer_id`] reports the
//!   backend the preview uses, which is a temporary M0 choice (ADR-0008), so a
//!   fingerprint records it instead of implying it is final.
//!
//! ## Errors
//!
//! Every fallible call returns [`AppError`], which carries a stable
//! [`AppErrorCode`] to branch on and a message for a person. A rejected
//! operation leaves the session exactly as it was: no partial edit, no advanced
//! revision, and no change to what counts as saved.
//!
//! ## Example
//!
//! ```
//! use std::sync::Arc;
//!
//! use swotvibe_app::{EditorSession, PreviewOptions, SharedTextEngine};
//! use swotvibe_text::{FontSet, ParleyTextEngine};
//!
//! // A session needs a text engine, because layout measures text. The fonts are
//! // supplied by the host; this crate never reads a font from the system.
//! let text: SharedTextEngine = Arc::new(ParleyTextEngine::new(FontSet::new()));
//! let mut session = EditorSession::new(text);
//!
//! // A new document is empty but not dirty: it matches what the session would
//! // write. There is nothing to undo yet.
//! assert!(!session.is_dirty());
//! assert!(!session.can_undo());
//!
//! let view = session.view().expect("an empty document has a view");
//! assert_eq!(view.revision, 0);
//! assert!(view.pages.is_empty());
//!
//! // A preview of a page that does not exist is a typed error, not a panic.
//! let page = swotvibe_core::PageId::new();
//! let error = session
//!     .preview(page, PreviewOptions::default())
//!     .expect_err("the page is not in the document");
//! assert_eq!(error.code, swotvibe_app::AppErrorCode::UnknownPage);
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod dto;
mod error;
mod session;

pub use dto::{
    CommitSummary, DocumentView, EditCommand, EditRequest, HitTestResult, LayoutNodeView,
    LayoutView, NodeIdString, NodeView, PageView, Preview, PreviewOptions, PropsView, Rgba,
    StrokeView,
};
pub use error::{AppError, AppErrorCode};
pub use session::{
    EditorSession, SharedTextEngine, asset_is_registered, checked_scalar, is_selectable,
};
