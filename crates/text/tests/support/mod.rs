//! Shared helpers for the reference tests of the M0 slice.
//!
//! This file is included by several test crates with `#[path = ...] mod support;`
//! so that one definition of "where the repository is" and "which fonts are
//! pinned" is used everywhere. It is included rather than depended on because a
//! test-only helper does not belong in any crate's public surface.
//!
//! Nothing here discovers a font from the host. The set is exactly the pinned
//! files under `assets/fonts`, registered under the family names the sample
//! documents use.

#![allow(dead_code)]

use std::path::PathBuf;

use swotvibe_text::FontSet;

/// The family name of the pinned Latin font.
pub const LATIN_FAMILY: &str = "Inter";

/// The family name of the pinned Arabic font.
pub const ARABIC_FAMILY: &str = "Noto Sans Arabic";

/// The repository root, found by walking up from the including crate.
///
/// `CARGO_MANIFEST_DIR` belongs to the crate that includes this file, so the
/// search must not assume a fixed depth.
pub fn repository_root() -> PathBuf {
    let mut directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    loop {
        if directory.join("assets").join("fonts").is_dir() {
            return directory;
        }
        assert!(
            directory.pop(),
            "cannot find the repository root above {}",
            env!("CARGO_MANIFEST_DIR")
        );
    }
}

/// Reads a file below `assets/fonts`.
pub fn font_bytes(relative: &str) -> Vec<u8> {
    let path = repository_root()
        .join("assets")
        .join("fonts")
        .join(relative);
    std::fs::read(&path).unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()))
}

/// Reads a file below `tests/fixtures`.
pub fn fixture_bytes(relative: &str) -> Vec<u8> {
    let path = repository_root()
        .join("tests")
        .join("fixtures")
        .join(relative);
    std::fs::read(&path).unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()))
}

/// Reads a UTF-8 file below `tests/fixtures`.
pub fn fixture_text(relative: &str) -> String {
    String::from_utf8(fixture_bytes(relative)).expect("the fixture should be UTF-8")
}

/// The pinned font set: Inter for Latin and Noto Sans Arabic, both OFL.
pub fn pinned_fonts() -> FontSet {
    let mut fonts = FontSet::new();
    fonts
        .register(LATIN_FAMILY, font_bytes("inter/Inter-variable.ttf"))
        .expect("the Latin face should register");
    fonts
        .register(
            ARABIC_FAMILY,
            font_bytes("noto-sans-arabic/NotoSansArabic-variable.ttf"),
        )
        .expect("the Arabic face should register");
    fonts
}
