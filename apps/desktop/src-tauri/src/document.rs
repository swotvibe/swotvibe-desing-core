//! Document input and output.
//!
//! ## Why the paths never leave Rust
//!
//! The interface asks to open or save; the host shows the dialog, learns the
//! path, and keeps it. A path that crosses to the WebView is a fact about the
//! user's machine that the page has no use for, and one more thing a compromised
//! page could act on. The commands here therefore return a document or an error,
//! never a location.
//!
//! ## Why a save is staged
//!
//! A write that fails halfway leaves a truncated file, which is worse than no
//! save at all. The bytes go to a temporary file in the destination's own
//! directory first, are flushed, and only then replace the target. The temporary
//! file is in the same directory on purpose: a rename across volumes is a copy,
//! which is not atomic.
//!
//! This is deliberately smaller than the bundle CLI's replacement and recovery
//! flow. That flow exists to preserve a previous good file and to recover from a
//! crash mid-replacement; it belongs to the headless tooling, which owns
//! backups. A host editor's job is to not corrupt the document it was given.

use std::io::Write;
use std::path::{Path, PathBuf};

use swotvibe_app::{AppError, AppErrorCode, DocumentView};
use tauri::{State, Window};
use tauri_plugin_dialog::DialogExt;

use crate::commands::{EditorState, IpcError};

/// The filters the native dialogs offer.
///
/// The extension is not published as a stable product format; it is what the
/// current schema's JSON is written as, and `ADR-0003` keeps the container
/// question open.
const DOCUMENT_EXTENSION: &str = "json";

/// Asks for a file, reads it, and opens it.
///
/// Returns `None` when the dialog is dismissed. That is not an error: cancelling
/// is a normal answer, and reporting it as a failure would train a caller to
/// ignore failures.
///
/// # Errors
///
/// An [`IpcError`] when the file cannot be read, or when it is read but cannot be
/// opened — a malformed payload, an unsupported schema version, or a document
/// that breaks an integrity rule. A failed open leaves the current document
/// alone.
#[tauri::command]
pub async fn open_document(
    window: Window,
    state: State<'_, EditorState>,
) -> Result<Option<DocumentView>, IpcError> {
    let Some(path) = pick_open_path(&window) else {
        return Ok(None);
    };

    let bytes = std::fs::read(&path).map_err(|error| {
        AppError::new(
            AppErrorCode::DocumentRead,
            format!("cannot read {}: {error}", file_label(&path)),
        )
    })?;

    // Only the file name is reported. It is what a title bar shows, and it is the
    // one part of a location a person expects to see.
    let display_name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned());
    let view = state.lock_document().open_json(&bytes, display_name)?;
    Ok(Some(view))
}

/// Asks for a destination and writes the document there.
///
/// Returns `None` when the dialog is dismissed.
///
/// # Errors
///
/// An [`IpcError`] when the document cannot be projected, or when the write
/// fails. A failed write does not mark the document as saved.
#[tauri::command]
pub async fn save_document(
    window: Window,
    state: State<'_, EditorState>,
) -> Result<Option<String>, IpcError> {
    let Some(path) = pick_save_path(&window) else {
        return Ok(None);
    };

    // Export and write under one lock: two concurrent saves would otherwise
    // interleave a read of the bytes with a change to the document, and the file
    // would not match the revision it claims to be.
    let mut session = state.lock_document();
    let bytes = session.export_bytes()?;
    write_atomically(&path, &bytes).map_err(|error| {
        AppError::new(
            AppErrorCode::DocumentWrite,
            format!("cannot write {}: {error}", file_label(&path)),
        )
    })?;
    session.note_saved()?;

    Ok(path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned()))
}

fn pick_open_path(window: &Window) -> Option<PathBuf> {
    let chosen = window
        .dialog()
        .file()
        .add_filter("Design document", &[DOCUMENT_EXTENSION])
        .blocking_pick_file();
    chosen.and_then(|path| path.into_path().ok())
}

fn pick_save_path(window: &Window) -> Option<PathBuf> {
    let chosen = window
        .dialog()
        .file()
        .add_filter("Design document", &[DOCUMENT_EXTENSION])
        .set_file_name("design.json")
        .blocking_save_file();
    chosen.and_then(|path| path.into_path().ok())
}

/// The name a message may mention, never the whole path.
fn file_label(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "the document".to_owned())
}

/// Writes `bytes` to `path` by staging them beside it and replacing the target.
///
/// # Errors
///
/// Returns the operating system's error when the staging file cannot be created,
/// written, flushed, or renamed.
pub fn write_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let directory = path.parent().unwrap_or_else(|| Path::new("."));
    let staging = directory.join(format!(
        ".{}.staged",
        path.file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "document".to_owned())
    ));

    {
        let mut file = std::fs::File::create(&staging)?;
        file.write_all(bytes)?;
        // Flush before the rename: a rename that wins the race against the data
        // would publish an empty file.
        file.sync_all()?;
    }

    match std::fs::rename(&staging, path) {
        Ok(()) => Ok(()),
        Err(error) => {
            // Leaving the staging file behind would turn one failed save into a
            // directory that slowly fills with half-written documents.
            let _ = std::fs::remove_file(&staging);
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::state_from_font_directory;

    #[test]
    fn a_write_replaces_the_target_and_leaves_no_staging_file() {
        let directory = std::env::temp_dir().join(format!("swotvibe-host-{}", std::process::id()));
        std::fs::create_dir_all(&directory).expect("the scratch directory should be creatable");
        let target = directory.join("document.json");

        write_atomically(&target, b"first").expect("the first write should succeed");
        assert_eq!(std::fs::read(&target).unwrap(), b"first");

        write_atomically(&target, b"second").expect("the second write should succeed");
        assert_eq!(
            std::fs::read(&target).unwrap(),
            b"second",
            "the target holds the new bytes"
        );

        let leftovers: Vec<_> = std::fs::read_dir(&directory)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".staged"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "staging files are cleaned up: {leftovers:?}"
        );

        std::fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn a_message_names_the_file_and_not_the_directory() {
        let path = Path::new("/somewhere/private/design.json");
        assert_eq!(file_label(path), "design.json");
    }

    /// The host's save-then-reopen path, without the dialog.
    ///
    /// The dialog is the one part a test cannot drive, so everything behind it is
    /// driven here: export the bytes, write them the way a save writes them, read
    /// them back the way an open reads them, and confirm the document that comes
    /// back carries the edit and the structure. Without this, the acceptance item
    /// "save to a file and reopen it" would rest on reading the code.
    #[test]
    fn a_saved_document_reopens_with_its_edit_intact() {
        let directory =
            std::env::temp_dir().join(format!("swotvibe-host-roundtrip-{}", std::process::id()));
        std::fs::create_dir_all(&directory).expect("the scratch directory should be creatable");
        let target = directory.join("design.json");

        let fonts = crate::font_directory();
        let state = state_from_font_directory(&fonts).expect("the pinned fonts should load");
        let sample = std::fs::read(
            fonts
                .parent()
                .expect("the font directory has a parent")
                .join("..")
                .join("tests/fixtures/m0-sample-v2.json"),
        )
        .expect("the M0 sample should be readable");

        let header = "018f0000-0000-7000-8000-000000020002";
        let new_fill = swotvibe_app::Rgba::opaque(200, 30, 90);

        {
            let mut session = state.lock_document();
            session
                .open_json(&sample, Some("m0-sample-v2.json".to_owned()))
                .expect("the sample opens");
            let revision = session.revision().as_u64();
            session
                .apply(&swotvibe_app::EditRequest {
                    expected_revision: revision,
                    commands: vec![swotvibe_app::EditCommand::SetNodeFill {
                        node: header.to_owned(),
                        fill: Some(new_fill),
                    }],
                })
                .expect("the edit applies");
            assert!(session.is_dirty(), "an edit makes the document unsaved");

            let bytes = session.export_bytes().expect("the document exports");
            write_atomically(&target, &bytes).expect("the write succeeds");
            session.note_saved().expect("the save is recorded");
            assert!(!session.is_dirty(), "a written document is saved");
        }

        // A second session stands in for reopening the file after a restart.
        let reopened = state_from_font_directory(&fonts).expect("the pinned fonts should load");
        let bytes = std::fs::read(&target).expect("the written file is readable");
        let mut session = reopened.lock_document();
        let view = session
            .open_json(&bytes, Some("design.json".to_owned()))
            .expect("the written file opens");

        assert!(!view.dirty, "a freshly opened document is not modified");
        assert_eq!(view.pages.len(), 1);
        assert_eq!(
            session
                .node_props(header)
                .expect("the node is present")
                .fill,
            Some(new_fill),
            "the edit survived the round trip through the file system"
        );
        assert!(
            !session.can_undo(),
            "session history is not part of the file"
        );

        std::fs::remove_dir_all(&directory).ok();
    }
}
