//! The adapter-neutral request, result, font registry, and trait.

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use sha2::{Digest, Sha256};
use swotvibe_core::{Scalar, TextAlign, TextDirection};

use crate::outline::GlyphOutline;

/// Why a text operation failed.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum TextError {
    /// No registered family matches the request.
    UnknownFamily {
        /// The family the request named.
        family: String,
        /// The families that are registered, for diagnostics.
        registered: Vec<String>,
    },
    /// The request carried a value the engine does not accept.
    InvalidRequest {
        /// A human-readable explanation.
        message: String,
    },
    /// The requested paragraph direction is not supported by this backend.
    ///
    /// Returned instead of guessing, because a paragraph laid out in the wrong
    /// base direction produces wrong glyph order rather than a visible error.
    UnsupportedDirection {
        /// The direction that was requested.
        requested: TextDirection,
    },
    /// The backend could not shape or measure the text.
    Backend {
        /// A human-readable explanation from the backend.
        message: String,
    },
}

impl fmt::Display for TextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownFamily { family, registered } => write!(
                f,
                "no registered font family named `{family}` (registered: {})",
                registered.join(", ")
            ),
            Self::InvalidRequest { message } => write!(f, "invalid text request: {message}"),
            Self::UnsupportedDirection { requested } => {
                write!(
                    f,
                    "the backend cannot force the base direction {requested:?}"
                )
            }
            Self::Backend { message } => write!(f, "the text backend failed: {message}"),
        }
    }
}

impl std::error::Error for TextError {}

/// A stable handle for one registered face: its family name plus the file's
/// bytes and face index.
///
/// The family is part of the identity because registering the same file under
/// two names produces two registrations, and a run report has to name the one
/// the request asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FontKey(u64);

impl FontKey {
    /// The raw key, for diagnostics and fingerprints.
    #[must_use]
    pub const fn as_u64(self) -> u64 {
        self.0
    }
}

impl fmt::Display for FontKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:016x}", self.0)
    }
}

/// One font face registered from bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontSource {
    /// The family name the document refers to.
    pub family: String,
    /// The face index inside the file, for a collection.
    pub index: u32,
    /// The font file's bytes.
    pub bytes: Vec<u8>,
    /// The face's hash, used as its identity.
    key: FontKey,
}

impl FontSource {
    /// The face's stable key.
    #[must_use]
    pub const fn key(&self) -> FontKey {
        self.key
    }

    /// The SHA-256 of the file's bytes, in lowercase hex.
    #[must_use]
    pub fn sha256(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(&self.bytes);
        format!("{:x}", hasher.finalize())
    }
}

/// What a result was produced from, for a reference-image fingerprint.
///
/// A golden image is only comparable against the fonts it was rendered with, so
/// every reference run records these values beside the image. A change to any
/// field is a change of fingerprint, not a regression to be accepted silently.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontFingerprint {
    /// The family name.
    pub family: String,
    /// The face index inside the file.
    pub index: u32,
    /// The number of bytes in the file.
    pub bytes: usize,
    /// The SHA-256 of the file's bytes, in lowercase hex.
    pub sha256: String,
}

/// The set of fonts an engine may use, in priority order.
///
/// A set is built explicitly, from bytes. Nothing here reads a font directory,
/// so the same set produces the same shaping on every machine.
#[derive(Debug, Clone, Default)]
pub struct FontSet {
    sources: Vec<FontSource>,
    by_key: HashMap<FontKey, usize>,
    by_family: HashMap<String, Vec<usize>>,
}

impl FontSet {
    /// An empty set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a face from bytes under `family`.
    ///
    /// Registering the same family, file, and face index twice is idempotent and
    /// returns the existing key, so a test can build a set without tracking what
    /// it already added. Registering the same file under a *different* family
    /// adds a second registration with its own key, which is what lets a
    /// document refer to one file by two names.
    ///
    /// # Errors
    ///
    /// Returns [`TextError::InvalidRequest`] when the family is empty or the
    /// bytes are empty.
    pub fn register(
        &mut self,
        family: impl Into<String>,
        bytes: impl Into<Vec<u8>>,
    ) -> Result<FontKey, TextError> {
        self.register_indexed(family, bytes, 0)
    }

    /// Registers a face from a specific index inside a font file.
    ///
    /// # Errors
    ///
    /// As [`FontSet::register`].
    pub fn register_indexed(
        &mut self,
        family: impl Into<String>,
        bytes: impl Into<Vec<u8>>,
        index: u32,
    ) -> Result<FontKey, TextError> {
        let family = family.into();
        let bytes = bytes.into();
        if family.is_empty() {
            return Err(TextError::InvalidRequest {
                message: "the family name is empty".to_owned(),
            });
        }
        if bytes.is_empty() {
            return Err(TextError::InvalidRequest {
                message: format!("the bytes for family `{family}` are empty"),
            });
        }

        let mut hasher = Sha256::new();
        hasher.update(family.as_bytes());
        hasher.update([0]);
        hasher.update(index.to_le_bytes());
        hasher.update(&bytes);
        let digest = hasher.finalize();
        let mut head = [0u8; 8];
        head.copy_from_slice(&digest[..8]);
        let key = FontKey(u64::from_le_bytes(head));

        if let Some(existing) = self.by_key.get(&key) {
            let existing = &self.sources[*existing];
            if existing.family == family {
                return Ok(key);
            }
        }

        let position = self.sources.len();
        self.sources.push(FontSource {
            family: family.clone(),
            index,
            bytes,
            key,
        });
        self.by_key.insert(key, position);
        self.by_family.entry(family).or_default().push(position);
        Ok(key)
    }

    /// The number of registered faces.
    #[must_use]
    pub fn len(&self) -> usize {
        self.sources.len()
    }

    /// Whether no face is registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }

    /// Every registered face, in registration order.
    #[must_use]
    pub fn sources(&self) -> &[FontSource] {
        &self.sources
    }

    /// The face behind a key.
    #[must_use]
    pub fn source(&self, key: FontKey) -> Option<&FontSource> {
        self.by_key.get(&key).map(|index| &self.sources[*index])
    }

    /// The registered faces of a family, in registration order.
    #[must_use]
    pub fn faces_of(&self, family: &str) -> Vec<&FontSource> {
        self.by_family
            .get(family)
            .map(|positions| {
                positions
                    .iter()
                    .map(|index| &self.sources[*index])
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The family names, in registration order, without duplicates.
    #[must_use]
    pub fn families(&self) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        for source in &self.sources {
            if !names.contains(&source.family) {
                names.push(source.family.clone());
            }
        }
        names
    }

    /// The fingerprint of every registered face.
    #[must_use]
    pub fn fingerprint(&self) -> Vec<FontFingerprint> {
        self.sources
            .iter()
            .map(|source| FontFingerprint {
                family: source.family.clone(),
                index: source.index,
                bytes: source.bytes.len(),
                sha256: source.sha256(),
            })
            .collect()
    }

    /// Fails with [`TextError::UnknownFamily`] unless `family` is registered.
    ///
    /// # Errors
    ///
    /// Returns [`TextError::UnknownFamily`] when nothing is registered under the
    /// name.
    pub fn require_family(&self, family: &str) -> Result<(), TextError> {
        if self.by_family.contains_key(family) {
            Ok(())
        } else {
            Err(TextError::UnknownFamily {
                family: family.to_owned(),
                registered: self.families(),
            })
        }
    }
}

/// A paragraph to measure or shape.
#[derive(Debug, Clone, PartialEq)]
pub struct TextRequest<'a> {
    /// The text. Line breaks are significant.
    pub text: &'a str,
    /// The resolved font family name.
    pub font_family: &'a str,
    /// The font size in design units. Must be greater than zero.
    pub font_size: f64,
    /// The font weight, `1..=1000`.
    pub font_weight: u16,
    /// The paragraph's base direction.
    pub direction: TextDirection,
    /// The paragraph alignment.
    pub align: TextAlign,
    /// The wrap width in design units, or `None` to keep one line per break.
    pub max_width: Option<f64>,
}

impl TextRequest<'_> {
    /// Checks the values the engine cannot express in its own types.
    ///
    /// # Errors
    ///
    /// Returns [`TextError::InvalidRequest`] for a non-finite or non-positive
    /// font size, a non-finite wrap width, or a weight outside `1..=1000`.
    pub fn check(&self) -> Result<(), TextError> {
        if !self.font_size.is_finite() || self.font_size <= 0.0 {
            return Err(TextError::InvalidRequest {
                message: format!("the font size {} is not greater than zero", self.font_size),
            });
        }
        if !(1..=1000).contains(&self.font_weight) {
            return Err(TextError::InvalidRequest {
                message: format!("the font weight {} is outside 1..=1000", self.font_weight),
            });
        }
        if let Some(width) = self.max_width
            && (!width.is_finite() || width < 0.0)
        {
            return Err(TextError::InvalidRequest {
                message: format!("the wrap width {width} is not a finite, non-negative length"),
            });
        }
        if self.font_family.is_empty() {
            return Err(TextError::InvalidRequest {
                message: "the font family is empty".to_owned(),
            });
        }
        Ok(())
    }

    /// The request as a cache key.
    pub(crate) fn cache_key(&self) -> RequestKey {
        RequestKey {
            text: self.text.to_owned(),
            font_family: self.font_family.to_owned(),
            font_size: self.font_size.to_bits(),
            font_weight: self.font_weight,
            direction: self.direction,
            align: self.align,
            max_width: self.max_width.map(f64::to_bits),
        }
    }
}

/// A cache key that compares by bit pattern, so a request is only reused for an
/// exactly equal call.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct RequestKey {
    text: String,
    font_family: String,
    font_size: u64,
    font_weight: u16,
    direction: TextDirection,
    align: TextAlign,
    max_width: Option<u64>,
}

/// One glyph with its pen position in line-local space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PositionedGlyph {
    /// The glyph identity inside its font.
    pub id: u16,
    /// The pen position along the baseline, in design units.
    pub x: f64,
    /// The baseline position inside the line box, in design units.
    pub y: f64,
}

/// A style the backend had to synthesize because the face does not ship it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TextSynthesis {
    /// Whether the run's weight was faked by emboldening.
    pub bold: bool,
    /// Whether the run's slope was faked by skewing.
    pub italic: bool,
}

/// A shaped sequence of glyphs that share one font and one style.
#[derive(Debug, Clone, PartialEq)]
pub struct ShapedRun {
    /// The face this run was shaped with.
    pub font: FontKey,
    /// The size the run was shaped at, in design units.
    pub font_size: f64,
    /// Whether the run's text is right-to-left.
    pub is_rtl: bool,
    /// The styles the backend synthesized for this run.
    ///
    /// Reported rather than applied: the outlines this crate returns are the
    /// font's own, so a caller that draws them draws the shipped face. A caller
    /// that wants the synthesized look has to apply it, and knowing that it is
    /// needed is what keeps a reference image honest.
    pub synthesis: TextSynthesis,
    /// The run's glyphs, in visual order.
    pub glyphs: Vec<PositionedGlyph>,
}

/// One line of a shaped paragraph.
#[derive(Debug, Clone, PartialEq)]
pub struct ShapedLine {
    /// The byte range of the text on this line.
    pub start: usize,
    /// The end of the byte range, exclusive.
    pub end: usize,
    /// The distance from the line box's top to the baseline, in design units.
    pub baseline: f64,
    /// The line box height, in design units.
    pub height: f64,
    /// The width a box should take for this line: the pen advance with any
    /// trailing whitespace removed, in design units.
    ///
    /// A trailing space moves the pen but does not ask for room, so it is
    /// excluded here. This is the width a hug-sized text node fits to.
    pub width: f64,
    /// The pen advance after the line's text, including trailing whitespace.
    ///
    /// A caller laying text out in a line of its own needs this value rather
    /// than [`Self::width`], because the next run starts after the space.
    pub advance: f64,
    /// The line's horizontal offset from alignment, in design units.
    pub offset: f64,
    /// The shaped runs, in visual order.
    pub runs: Vec<ShapedRun>,
}

/// A shaped paragraph, ready to measure and to draw.
///
/// The measured extents are **ink extents**: a trailing space advances the pen
/// but does not widen the line, so a hug-sized node does not grow for
/// whitespace at the end of a paragraph. A caller that needs the pen advance of
/// a line has to add the trailing whitespace itself, which is why this type
/// reports what it measured rather than pretending the two are the same.
#[derive(Debug, Clone, PartialEq)]
pub struct ShapedText {
    /// The lines, in order.
    pub lines: Vec<ShapedLine>,
    /// The width of the widest line, in design units.
    pub width: f64,
    /// The total height of the line boxes, in design units.
    pub height: f64,
    /// The largest ascent over the lines, in design units.
    pub ascent: f64,
    /// The largest descent over the lines, in design units.
    pub descent: f64,
}

impl ShapedText {
    /// The measured size, as `(width, height)` in design units.
    #[must_use]
    pub fn measured_size(&self) -> (f64, f64) {
        (self.width, self.height)
    }

    /// The measured size as a validated [`Scalar`] pair, for a caller that wants
    /// the same finite-value rules the document obeys.
    ///
    /// # Errors
    ///
    /// Returns the geometry error when a measurement is not finite or leaves the
    /// supported range, which no engine built on this contract should produce.
    pub fn measured_scalars(&self) -> Result<(Scalar, Scalar), swotvibe_core::GeometryError> {
        Ok((Scalar::new(self.width)?, Scalar::new(self.height)?))
    }
}

/// Measurement and shaping, behind one adapter-neutral contract.
///
/// Implementations own their backend. The trait is deliberately expressed in
/// design units and in [`PathCommand`] values, so nothing above it has to know
/// which shaping library is behind it.
pub trait TextLayoutEngine {
    /// The fonts this engine may use.
    fn fonts(&self) -> &FontSet;

    /// Shapes and measures one paragraph.
    ///
    /// # Errors
    ///
    /// Returns [`TextError`] for an invalid request, an unregistered family, a
    /// direction the backend cannot force, or a backend failure.
    fn shape(&self, request: &TextRequest<'_>) -> Result<Arc<ShapedText>, TextError>;

    /// The outline of one glyph at one size, in design units.
    ///
    /// The outline is in the glyph's own space: the caller positions it with the
    /// [`PositionedGlyph`] coordinates from a shaped run. Returns `Ok(None)` for
    /// glyph 0 and for any glyph the font does not draw, which is what a
    /// whitespace or an empty glyph is.
    ///
    /// # Errors
    ///
    /// Returns [`TextError`] when the key is not a registered face, the size is
    /// not usable, or the outline could not be read.
    fn outline(
        &self,
        font: FontKey,
        glyph: u16,
        font_size: f64,
    ) -> Result<Option<Arc<GlyphOutline>>, TextError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registering_the_same_face_twice_returns_the_same_key() {
        let mut fonts = FontSet::new();
        let first = fonts
            .register("Noto Sans", vec![1, 2, 3])
            .expect("register");
        let second = fonts
            .register("Noto Sans", vec![1, 2, 3])
            .expect("register");
        assert_eq!(first, second);
        assert_eq!(fonts.len(), 1);
    }

    #[test]
    fn a_different_family_or_face_gets_a_different_key() {
        let mut fonts = FontSet::new();
        let a = fonts.register("A", vec![1, 2, 3]).expect("register");
        let b = fonts.register("B", vec![1, 2, 3]).expect("register");
        let c = fonts.register("A", vec![4, 5, 6]).expect("register");
        assert_ne!(a, b);
        assert_ne!(a, c);
        assert_eq!(fonts.len(), 3);
        assert_eq!(fonts.families(), vec!["A".to_owned(), "B".to_owned()]);
    }

    #[test]
    fn an_empty_family_or_empty_bytes_is_refused() {
        let mut fonts = FontSet::new();
        assert!(fonts.register("", vec![1]).is_err());
        assert!(fonts.register("A", Vec::new()).is_err());
        assert!(fonts.is_empty());
    }

    #[test]
    fn the_fingerprint_records_the_bytes_and_the_hash() {
        let mut fonts = FontSet::new();
        fonts.register("A", vec![1, 2, 3]).expect("register");
        let fingerprint = fonts.fingerprint();
        assert_eq!(fingerprint.len(), 1);
        assert_eq!(fingerprint[0].family, "A");
        assert_eq!(fingerprint[0].bytes, 3);
        // SHA-256 of the bytes 01 02 03.
        assert_eq!(
            fingerprint[0].sha256,
            "039058c6f2c0cb492c533b0a4d14ef77cc0f78abccced5287d84a1a2011cfb81"
        );
    }

    #[test]
    fn requiring_an_unregistered_family_lists_what_is_registered() {
        let mut fonts = FontSet::new();
        fonts.register("A", vec![1]).expect("register");
        let error = fonts.require_family("B").expect_err("B is not registered");
        assert_eq!(
            error,
            TextError::UnknownFamily {
                family: "B".to_owned(),
                registered: vec!["A".to_owned()],
            }
        );
    }

    #[test]
    fn an_invalid_request_is_refused() {
        let mut request = TextRequest {
            text: "x",
            font_family: "A",
            font_size: 16.0,
            font_weight: 400,
            direction: TextDirection::Auto,
            align: TextAlign::Start,
            max_width: None,
        };
        assert_eq!(request.check(), Ok(()));

        request.font_size = 0.0;
        assert!(request.check().is_err());
        request.font_size = 16.0;
        request.font_weight = 0;
        assert!(request.check().is_err());
        request.font_weight = 400;
        request.max_width = Some(f64::NAN);
        assert!(request.check().is_err());
    }
}
