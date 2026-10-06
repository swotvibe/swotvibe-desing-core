//! The IPC adapter: a thin, typed surface over the application service.
//!
//! ## What belongs here, and what does not
//!
//! This module translates. It converts a request into a call on
//! [`EditorSession`], and a failure into a serializable error. It holds no
//! document rules, no second copy of the document, and no state of its own
//! beyond the session it guards.
//!
//! ## Why the command set is small
//!
//! Every command is a hole in the wall between a WebView and the operating
//! system. A general "run this command" endpoint would be simpler to write and
//! would also hand a compromised page the whole edit surface. The set below is
//! therefore explicit: seven operations, each with a typed request.
//!
//! ## Locking
//!
//! The session is behind a mutex, and a command that can take measurable time —
//! layout, render, a write — works from a snapshot instead of holding the lock.
//! `tauri::State` gives a reference, so a command takes the lock for as long as
//! it needs and no longer.

use std::sync::{Arc, Mutex, MutexGuard};

use serde::Serialize;
use swotvibe_app::{
    AppError, AppErrorCode, CommitSummary, DocumentView, EditorSession, HitTestResult, LayoutView,
    Preview, PreviewOptions, PropsView,
};
use tauri::State;

/// The session a window is editing.
///
/// One session, not a map: nothing opens two documents at once, and a map would
/// be a guess about a feature that does not exist yet.
pub struct EditorState {
    session: Mutex<EditorSession>,
}

impl EditorState {
    /// Wraps a session for the IPC layer.
    #[must_use]
    pub const fn new(session: EditorSession) -> Self {
        Self {
            session: Mutex::new(session),
        }
    }

    /// Takes the session lock.
    ///
    /// A poisoned lock is recovered rather than reported: the alternative is a
    /// window that can never edit again, and the session's own invariants are
    /// revalidated by every command that reads or writes it.
    pub(crate) fn lock_document(&self) -> MutexGuard<'_, EditorSession> {
        self.session
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// A failure in a shape a frontend can branch on.
///
/// `AppError` already has that shape. This wrapper exists so the *transport*
/// failure — a poisoned lock, a serialization problem — is reported in the same
/// shape instead of arriving as a bare string, which would leave the interface
/// with nothing to match on.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IpcError {
    /// The stable category.
    pub code: AppErrorCode,
    /// A readable explanation.
    pub message: String,
    /// The node the failure concerns, when there is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,
    /// The page the failure concerns, when there is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<String>,
    /// The revision that was current when it failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision: Option<u64>,
}

impl From<AppError> for IpcError {
    fn from(error: AppError) -> Self {
        Self {
            code: error.code,
            message: error.message,
            node: error.node,
            page: error.page,
            revision: error.revision,
        }
    }
}

impl IpcError {
    fn transport(message: impl Into<String>) -> Self {
        Self {
            code: AppErrorCode::InvalidRequest,
            message: message.into(),
            node: None,
            page: None,
            revision: None,
        }
    }
}

/// The result type every command returns.
type IpcResult<T> = Result<T, IpcError>;

/// The open document, with no session state in it.
///
/// # Errors
///
/// A mapped [`IpcError`] when the document cannot be projected.
#[tauri::command]
pub fn get_view(state: State<'_, EditorState>) -> IpcResult<DocumentView> {
    state.lock_document().view().map_err(IpcError::from)
}

/// Opens a document from bytes a host obtained through a file dialog.
///
/// The interface never sends a path. Choosing a file is the host's job, and
/// handing the WebView a filesystem path would widen what it can reach.
///
/// # Errors
///
/// A mapped [`IpcError`] for a malformed payload, an unsupported schema, or a
/// document that breaks an integrity rule. The open document is left untouched.
#[tauri::command]
pub fn open_bytes(
    state: State<'_, EditorState>,
    bytes: Vec<u8>,
    display_name: Option<String>,
) -> IpcResult<DocumentView> {
    state
        .lock_document()
        .open_json(&bytes, display_name)
        .map_err(IpcError::from)
}

/// Applies one atomic batch at the caller's revision.
///
/// # Errors
///
/// A mapped [`IpcError`], including `revision-conflict` when the caller is stale.
#[tauri::command]
pub fn apply(
    state: State<'_, EditorState>,
    request: swotvibe_app::EditRequest,
) -> IpcResult<CommitSummary> {
    state
        .lock_document()
        .apply(&request)
        .map_err(IpcError::from)
}

/// Undoes the most recent step.
///
/// # Errors
///
/// A mapped [`IpcError`], including `history` when there is nothing to undo.
#[tauri::command]
pub fn undo(state: State<'_, EditorState>, expected_revision: u64) -> IpcResult<CommitSummary> {
    state
        .lock_document()
        .undo(expected_revision)
        .map_err(IpcError::from)
}

/// Redoes the most recently undone step.
///
/// # Errors
///
/// A mapped [`IpcError`], including `history` when there is nothing to redo.
#[tauri::command]
pub fn redo(state: State<'_, EditorState>, expected_revision: u64) -> IpcResult<CommitSummary> {
    state
        .lock_document()
        .redo(expected_revision)
        .map_err(IpcError::from)
}

/// The node's properties on their own.
///
/// # Errors
///
/// A mapped [`IpcError`] for an unparsable identity or a node that is absent.
#[tauri::command]
pub fn node_props(state: State<'_, EditorState>, node: String) -> IpcResult<PropsView> {
    state
        .lock_document()
        .node_props(&node)
        .map_err(IpcError::from)
}

/// The resolved geometry of one page.
///
/// # Errors
///
/// A mapped [`IpcError`] for an unknown page or an unusable artboard size.
#[tauri::command]
pub fn layout(
    state: State<'_, EditorState>,
    page: String,
    options: PreviewOptions,
) -> IpcResult<LayoutView> {
    let page = parse_page(&page)?;
    state
        .lock_document()
        .layout(page, options)
        .map_err(IpcError::from)
}

/// The topmost node under a point in page units.
///
/// # Errors
///
/// A mapped [`IpcError`] for an unknown page, a non-finite point, or a failure
/// in the layout pass the hit test depends on.
#[tauri::command]
pub fn hit_test(
    state: State<'_, EditorState>,
    page: String,
    x: f64,
    y: f64,
    options: PreviewOptions,
) -> IpcResult<HitTestResult> {
    let page = parse_page(&page)?;
    state
        .lock_document()
        .hit_test(page, (x, y), options)
        .map_err(IpcError::from)
}

/// Renders one page to a PNG.
///
/// The image crosses as bytes, not as JSON: an RGBA array for one page is several
/// megabytes of numbers, which is the wrong thing to put on a message bus.
///
/// # Errors
///
/// A mapped [`IpcError`] for an unknown page, an unusable scale, or a failure in
/// the layout or render pass.
#[tauri::command]
pub fn preview(
    state: State<'_, EditorState>,
    page: String,
    options: PreviewOptions,
) -> IpcResult<Preview> {
    let page = parse_page(&page)?;
    state
        .lock_document()
        .preview(page, options)
        .map_err(IpcError::from)
}

/// The bytes a save would write, without marking anything as saved.
///
/// A host writes these to a destination it chose, then calls [`note_saved`].
/// Separating the two means a failed write cannot leave the document looking
/// saved.
///
/// # Errors
///
/// A mapped [`IpcError`] when the document cannot be projected.
#[tauri::command]
pub fn export_bytes(state: State<'_, EditorState>) -> IpcResult<Vec<u8>> {
    state.lock_document().export_bytes().map_err(IpcError::from)
}

/// Records that the last exported bytes were written successfully.
///
/// # Errors
///
/// A mapped [`IpcError`] when the document cannot be projected.
#[tauri::command]
pub fn note_saved(state: State<'_, EditorState>) -> IpcResult<()> {
    state.lock_document().note_saved().map_err(IpcError::from)
}

fn parse_page(value: &str) -> IpcResult<swotvibe_core::PageId> {
    value
        .parse()
        .map_err(|error| IpcError::transport(format!("`{value}` is not a page identity: {error}")))
}

/// Builds the state a window runs against.
///
/// The fonts are the repository's pinned files, read once at start-up. A host
/// that ships to customers would load them from its own resources; what matters
/// is that the same bytes are used for measurement and for drawing, and that they
/// are not read from the operating system.
///
/// # Errors
///
/// Returns a message naming the missing file when a font cannot be read or
/// registered. A host that cannot measure text cannot render a document, so this
/// is fatal rather than degraded.
pub fn state_from_font_directory(directory: &std::path::Path) -> Result<EditorState, String> {
    let fonts = load_pinned_fonts(directory)?;
    Ok(EditorState::new(EditorSession::new(Arc::new(
        swotvibe_text::ParleyTextEngine::new(fonts),
    ))))
}

/// The families the editor resolves against, and the file each one comes from.
///
/// Fixed on purpose: a document names a family, and the host decides which file
/// that is. A missing file is an error rather than a fallback, because a silent
/// substitution would change every measurement in the document.
const PINNED_FONTS: [(&str, &str); 2] = [
    ("Inter", "inter/Inter-variable.ttf"),
    (
        "Noto Sans Arabic",
        "noto-sans-arabic/NotoSansArabic-variable.ttf",
    ),
];

fn load_pinned_fonts(directory: &std::path::Path) -> Result<swotvibe_text::FontSet, String> {
    let mut fonts = swotvibe_text::FontSet::new();
    for (family, relative) in PINNED_FONTS {
        let path = directory.join(relative);
        let bytes = std::fs::read(&path)
            .map_err(|error| format!("cannot read the font {}: {error}", path.display()))?;
        fonts
            .register(family, bytes)
            .map_err(|error| format!("cannot register the font {family}: {error}"))?;
    }
    Ok(fonts)
}
