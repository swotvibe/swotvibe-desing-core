//! Typed application errors, and the stable codes a host branches on.
//!
//! An error that crosses to a user interface must be actionable without parsing
//! a sentence: the [`AppErrorCode`] is the branch key, and the message is for a
//! human. A code never changes meaning, and a new failure mode gets a new code
//! rather than overloading an existing one.
//!
//! ## Why an application-level error type
//!
//! The kernel's errors describe kernel rules (`BatchError`, `HistoryError`), and
//! the adapters' errors describe their own domain (`LayoutError`,
//! `RenderError`). A host would otherwise have to know all of them and invent
//! its own taxonomy at the boundary. This module performs that mapping once, and
//! records where the failure was attached so the interface can point at a node
//! or a page instead of showing a bare sentence.

use std::fmt;

use serde::{Deserialize, Serialize};
use swotvibe_core::{
    BatchError, CommandError, CommandErrorCode, HistoryError, NodeId, PageId, Revision,
    ValidationErrors,
};
use swotvibe_format::{ImportError, JsonError};
use swotvibe_layout::LayoutError;
use swotvibe_render::RenderError;

/// A stable, machine-readable failure category.
///
/// `#[non_exhaustive]` so a new category can be added without breaking a host's
/// match, which is the same rule the kernel applies to its own error enums.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum AppErrorCode {
    /// The request itself was malformed: an unparsable identity, a missing
    /// field, or a value outside the accepted range.
    InvalidRequest,
    /// A command named a node the document does not contain.
    UnknownNode,
    /// A command or query named a page the document does not contain.
    UnknownPage,
    /// The persisted payload did not match a schema this build understands.
    Schema,
    /// The document broke an integrity rule.
    Validation,
    /// A command was refused by state-level validation.
    CommandRejected,
    /// The caller's expected revision is not the document's revision, so
    /// nothing was attempted.
    RevisionConflict,
    /// There is nothing to undo or redo, or the timeline is out of sync with the
    /// document.
    History,
    /// A layout pass failed.
    Layout,
    /// A render pass failed.
    Render,
    /// A caller-supplied or configured resource budget was exceeded.
    ResourceLimit,
    /// Reading a document from its source failed. Raised by a host, which owns
    /// file and network access.
    DocumentRead,
    /// Writing a document to its destination failed. Raised by a host, which
    /// owns file and network access.
    DocumentWrite,
}

impl AppErrorCode {
    /// The stable wire name, matching the serialized form.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidRequest => "invalid-request",
            Self::UnknownNode => "unknown-node",
            Self::UnknownPage => "unknown-page",
            Self::Schema => "schema",
            Self::Validation => "validation",
            Self::CommandRejected => "command-rejected",
            Self::RevisionConflict => "revision-conflict",
            Self::History => "history",
            Self::Layout => "layout",
            Self::Render => "render",
            Self::ResourceLimit => "resource-limit",
            Self::DocumentRead => "document-read",
            Self::DocumentWrite => "document-write",
        }
    }
}

impl fmt::Display for AppErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One application-level failure, with the subject it concerns.
///
/// This type is part of the interface contract: it serializes to the shape a
/// host returns to its frontend. It is deliberately not the kernel's error type,
/// so that a kernel change cannot silently alter the wire shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppError {
    /// The branch key.
    pub code: AppErrorCode,
    /// A readable explanation, intended for a person.
    pub message: String,
    /// The node the failure concerns, when there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,
    /// The page the failure concerns, when there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<String>,
    /// The revision that was current when the failure was raised.
    ///
    /// A raw counter, not the kernel's `Revision`: the wire shape must not
    /// change when a kernel type changes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<u64>,
}

impl AppError {
    /// Builds an error with no subject.
    #[must_use]
    pub fn new(code: AppErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            node: None,
            page: None,
            revision: None,
        }
    }

    /// Builds an error about one node.
    #[must_use]
    pub fn on_node(code: AppErrorCode, node: NodeId, message: impl Into<String>) -> Self {
        Self {
            node: Some(node.to_string()),
            ..Self::new(code, message)
        }
    }

    /// Builds an error about one page.
    #[must_use]
    pub fn on_page(code: AppErrorCode, page: PageId, message: impl Into<String>) -> Self {
        Self {
            page: Some(page.to_string()),
            ..Self::new(code, message)
        }
    }

    /// Attaches the revision the failure was raised at.
    #[must_use]
    pub fn at_revision(mut self, revision: Revision) -> Self {
        self.revision = Some(revision.as_u64());
        self
    }

    /// Attaches a node subject to an error that already carries a code.
    #[must_use]
    pub fn with_node(mut self, node: NodeId) -> Self {
        self.node = Some(node.to_string());
        self
    }

    /// Attaches a page subject to an error that already carries a code.
    #[must_use]
    pub fn with_page(mut self, page: PageId) -> Self {
        self.page = Some(page.to_string());
        self
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)?;
        if let Some(node) = &self.node {
            write!(f, " (node {node})")?;
        }
        if let Some(page) = &self.page {
            write!(f, " (page {page})")?;
        }
        Ok(())
    }
}

impl std::error::Error for AppError {}

impl From<CommandError> for AppError {
    /// Maps a kernel command rejection.
    ///
    /// The kernel is the single validator: it checks each command against the
    /// state as the batch applies, so a command may name a node an earlier
    /// command in the same batch created. This mapping only translates the
    /// kernel's stable codes into the interface's coarser ones.
    fn from(error: CommandError) -> Self {
        let code = match error.code {
            CommandErrorCode::NodeNotFound => AppErrorCode::UnknownNode,
            CommandErrorCode::PageNotFound => AppErrorCode::UnknownPage,
            CommandErrorCode::AssetNotFound => AppErrorCode::UnknownNode,
            _ => AppErrorCode::CommandRejected,
        };
        Self {
            code,
            message: error.message.clone(),
            node: error.node.map(|node| node.to_string()),
            page: error.page.map(|page| page.to_string()),
            revision: None,
        }
    }
}

impl From<BatchError> for AppError {
    /// Maps a rejected batch.
    ///
    /// A revision conflict is its own code rather than a command rejection: the
    /// caller's remedy is to reload the view, not to change the command.
    fn from(error: BatchError) -> Self {
        match error {
            BatchError::RevisionConflict { expected, actual } => Self::new(
                AppErrorCode::RevisionConflict,
                format!(
                    "the request expected revision {expected}, but the document is at {actual}"
                ),
            )
            .at_revision(actual),
            BatchError::RevisionExhausted => Self::new(
                AppErrorCode::CommandRejected,
                "the document revision counter is exhausted",
            ),
            BatchError::Command(error) => Self::from(error),
        }
    }
}

impl From<HistoryError> for AppError {
    fn from(error: HistoryError) -> Self {
        match error {
            HistoryError::OutOfSync { expected, actual } => Self::new(
                AppErrorCode::History,
                format!("history is at revision {expected}, but the document is at {actual}"),
            )
            .at_revision(actual),
            other => Self::new(AppErrorCode::History, other.to_string()),
        }
    }
}

impl From<ImportError> for AppError {
    fn from(error: ImportError) -> Self {
        match error {
            ImportError::ResourceLimit { .. } => {
                Self::new(AppErrorCode::ResourceLimit, error.to_string())
            }
            _ => Self::new(AppErrorCode::Schema, error.to_string()),
        }
    }
}

impl From<JsonError> for AppError {
    fn from(error: JsonError) -> Self {
        match error {
            JsonError::TooLarge { .. } => Self::new(AppErrorCode::ResourceLimit, error.to_string()),
            other => Self::new(AppErrorCode::Schema, other.to_string()),
        }
    }
}

impl From<ValidationErrors> for AppError {
    fn from(errors: ValidationErrors) -> Self {
        Self::new(AppErrorCode::Validation, errors.to_string())
    }
}

impl From<LayoutError> for AppError {
    fn from(error: LayoutError) -> Self {
        match error {
            LayoutError::InvalidPageSize { .. } => {
                Self::new(AppErrorCode::InvalidRequest, error.to_string())
            }
            LayoutError::PageNotFound { page } => Self::on_page(
                AppErrorCode::UnknownPage,
                page,
                "the page is not in the document",
            ),
            _ => Self::new(AppErrorCode::Layout, error.to_string()),
        }
    }
}

impl From<RenderError> for AppError {
    fn from(error: RenderError) -> Self {
        match error {
            RenderError::InvalidConfig { .. } => {
                Self::new(AppErrorCode::InvalidRequest, error.to_string())
            }
            RenderError::PageNotFound { page } => Self::on_page(
                AppErrorCode::UnknownPage,
                page,
                "the page is not in the document",
            ),
            RenderError::MissingLayout { node } => Self::on_node(
                AppErrorCode::Render,
                node,
                "the layout result does not cover a node the scene needs",
            ),
            _ => Self::new(AppErrorCode::Render, error.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_have_stable_wire_names() {
        assert_eq!(AppErrorCode::RevisionConflict.as_str(), "revision-conflict");
        assert_eq!(AppErrorCode::UnknownNode.to_string(), "unknown-node");
    }

    #[test]
    fn an_error_serializes_without_empty_subjects() {
        let error = AppError::new(AppErrorCode::InvalidRequest, "bad");
        let json = serde_json::to_value(&error).unwrap();
        assert_eq!(json["code"], serde_json::json!("invalid-request"));
        assert!(json.get("node").is_none(), "an absent subject is omitted");
        assert!(json.get("revision").is_none());
    }

    #[test]
    fn a_revision_conflict_names_both_revisions() {
        let error = AppError::from(BatchError::RevisionConflict {
            expected: Revision::from_raw(4),
            actual: Revision::from_raw(7),
        });
        assert_eq!(error.code, AppErrorCode::RevisionConflict);
        assert_eq!(error.revision, Some(7));
        assert!(error.message.contains("expected revision 4"));
    }

    #[test]
    fn an_empty_timeline_is_a_history_error_not_a_command_error() {
        let error = AppError::from(HistoryError::NothingToUndo);
        assert_eq!(error.code, AppErrorCode::History);
    }

    #[test]
    fn a_missing_page_maps_to_the_unknown_page_code() {
        let page = PageId::new();
        let error = AppError::from(LayoutError::PageNotFound { page });
        assert_eq!(error.code, AppErrorCode::UnknownPage);
        assert_eq!(error.page.as_deref(), Some(page.to_string().as_str()));
    }
}
