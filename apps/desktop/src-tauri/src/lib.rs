//! The desktop host.
//!
//! Its whole job is to own a window, own the file system, and hand both to the
//! application service through [`commands`]. Nothing here decides what an edit
//! means: that lives in `swotvibe-app`, so the same behaviour is reachable from a
//! test, a command-line tool, or a future host.
//!
//! ## Where the fonts come from
//!
//! The host reads the repository's pinned font files at start-up and fails to
//! launch without them. Reading a font from the operating system would make the
//! same document measure differently on two machines, which is exactly what the
//! reference tests exist to prevent.

pub mod commands;
pub mod document;

use std::path::PathBuf;

use commands::EditorState;

/// The window's entry point.
///
/// # Panics
///
/// Panics if the pinned fonts cannot be read, or if Tauri cannot start. Both are
/// configuration failures at start-up, where a message is more useful than a
/// running window that cannot draw text.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let state = build_state().unwrap_or_else(|error| panic!("{error}"));

    tauri::Builder::default()
        // The dialog plugin is used for one thing: showing a native open or save
        // dialog. The path it returns is handled in Rust and never crosses to the
        // WebView.
        .plugin(tauri_plugin_dialog::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::get_view,
            commands::apply,
            commands::undo,
            commands::redo,
            commands::node_props,
            commands::layout,
            commands::hit_test,
            commands::preview,
            commands::export_bytes,
            commands::note_saved,
            document::open_document,
            document::open_sample,
            document::save_document,
        ])
        .run(tauri::generate_context!())
        .expect("the desktop host failed to start");
}

/// Builds the session the window edits.
///
/// The asset directory is resolved relative to the executable at run time and to
/// the crate at development time, so a developer build finds the repository's
/// fonts without an environment variable.
///
/// # Errors
///
/// Returns a message naming what could not be read.
pub fn build_state() -> Result<EditorState, String> {
    commands::state_from_font_directory(&font_directory())
}

/// The directory holding `inter/` and `noto-sans-arabic/`.
///
/// `SWOTVIBE_FONT_DIR` overrides it, which is what a packaged build sets. Without
/// it, the repository layout is searched from the crate directory upwards.
#[must_use]
pub fn font_directory() -> PathBuf {
    if let Some(from_environment) = std::env::var_os("SWOTVIBE_FONT_DIR") {
        return PathBuf::from(from_environment);
    }
    search_upwards("assets", "fonts").unwrap_or_else(|| PathBuf::from("assets/fonts"))
}

/// The committed M0 sample, when this build can see the repository it came from.
///
/// This is a development affordance, not a product feature: it gives a first run
/// something to open without hunting for a file. A packaged build has no
/// repository, so it answers `None` and the interface offers nothing.
#[must_use]
pub fn sample_fixture_path() -> Option<PathBuf> {
    if let Some(from_environment) = std::env::var_os("SWOTVIBE_SAMPLE_PATH") {
        let path = PathBuf::from(from_environment);
        return path.is_file().then_some(path);
    }
    search_upwards("tests", "fixtures")
        .map(|directory| directory.join("m0-sample-v2.json"))
        .filter(|path| path.is_file())
}

/// Walks up from the crate directory looking for `<root>/<first>/<second>`.
fn search_upwards(first: &str, second: &str) -> Option<PathBuf> {
    let mut directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    loop {
        let candidate = directory.join(first).join(second);
        if candidate.is_dir() {
            return Some(candidate);
        }
        if !directory.pop() {
            return None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_repository_fonts_are_found_from_the_crate_directory() {
        let directory = font_directory();
        assert!(
            directory.join("inter/Inter-variable.ttf").is_file(),
            "expected the pinned Latin face in {}",
            directory.display()
        );
        assert!(
            directory
                .join("noto-sans-arabic/NotoSansArabic-variable.ttf")
                .is_file(),
            "expected the pinned Arabic face in {}",
            directory.display()
        );
    }

    #[test]
    fn the_repository_sample_is_found_from_the_crate_directory() {
        // A first run has something to open only when this resolves, so a build
        // that cannot see it must say so rather than offer an action that fails.
        let path = sample_fixture_path().expect("the sample should be found");
        assert!(path.is_file(), "{} should exist", path.display());
        assert_eq!(path.file_name().unwrap(), "m0-sample-v2.json");
    }

    #[test]
    fn a_session_can_be_built_without_a_window() {
        // The host's start-up path is testable on its own: nothing here needs a
        // webview, which is the point of keeping the service out of Tauri.
        let state = build_state().expect("the pinned fonts should load");
        let mut session = state.lock_document();
        let view = session.view().expect("a view is available");
        assert!(view.pages.is_empty(), "a host starts on an empty document");
        assert_eq!(view.revision, 0);
        assert!(!view.dirty);
    }
}
