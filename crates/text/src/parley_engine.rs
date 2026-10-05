//! The Parley-backed engine: the temporary text backend of the M0 slice.
//!
//! Parley owns shaping, line breaking, and bidirectional ordering; this module
//! owns the translation into the crate's adapter-neutral types, and the rules
//! that keep a reference run reproducible:
//!
//! - **No system fonts.** The collection is built with `system_fonts: false` and
//!   only the faces of the [`FontSet`] are registered, so shaping cannot change
//!   with the machine.
//! - **No quantization.** Parley is asked for unrounded advances, so a
//!   measurement is the arithmetic result and not a pixel-snapped approximation.
//! - **Unhinted outlines.** Glyphs are drawn from their unhinted outlines, so
//!   the result does not depend on a hinting implementation.
//! - **Explicit results.** A family that is not registered, and a paragraph
//!   direction this backend cannot force, are both typed errors.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use parley::fontique::{Blob, Collection, CollectionOptions, FontInfoOverride};
use parley::layout::{Alignment, AlignmentOptions, PositionedLayoutItem};
use parley::style::{FontFamily, FontFamilyName, FontWeight, StyleProperty};
use parley::{FontContext, LayoutContext, RangedBuilder};
use skrifa::MetadataProvider;
use skrifa::prelude::{FontRef, GlyphId, LocationRef, Size};
use swotvibe_core::{TextAlign, TextDirection};

use crate::contract::{
    FontKey, FontSet, FontSource, PositionedGlyph, RequestKey, ShapedLine, ShapedRun, ShapedText,
    TextError, TextLayoutEngine, TextRequest, TextSynthesis,
};
use crate::outline::{GlyphOutline, PathRecorder};

/// How many shaped paragraphs and glyph outlines are kept before the caches are
/// dropped.
///
/// Layout asks for the same measurement several times per pass, so the cache is
/// what keeps a layout pass from re-shaping the same string. The cap bounds the
/// memory a large document can hold through the engine; when it is reached the
/// cache is cleared rather than evicted one entry at a time, which keeps the
/// bookkeeping simple and costs at most one extra shaping pass.
const CACHE_LIMIT: usize = 4096;

/// A text engine built on Parley 0.11.
///
/// See the crate documentation for the limits of this adapter.
pub struct ParleyTextEngine {
    fonts: FontSet,
    inner: Mutex<Inner>,
}

struct Inner {
    font_context: FontContext,
    layout_context: LayoutContext<[u8; 4]>,
    shaped: HashMap<RequestKey, Arc<ShapedText>>,
    outlines: HashMap<OutlineKey, Arc<GlyphOutline>>,
}

/// The cache key of one glyph outline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct OutlineKey {
    font: FontKey,
    glyph: u16,
    size_bits: u64,
}

impl ParleyTextEngine {
    /// Builds an engine over `fonts`.
    ///
    /// No font is discovered from the host: only the faces in `fonts` are
    /// registered.
    #[must_use]
    pub fn new(fonts: FontSet) -> Self {
        let mut collection = Collection::new(CollectionOptions {
            shared: false,
            // Even with the `system` feature compiled out, the option is set
            // explicitly so the intent cannot be lost in a feature change.
            system_fonts: false,
        });
        for source in fonts.sources() {
            let blob = Blob::from(source.bytes.clone());
            let info = FontInfoOverride {
                family_name: Some(source.family.as_str()),
                ..FontInfoOverride::default()
            };
            collection.register_fonts(blob, Some(info));
        }

        let font_context = FontContext {
            collection,
            source_cache: Default::default(),
        };

        Self {
            fonts,
            inner: Mutex::new(Inner {
                font_context,
                layout_context: LayoutContext::new(),
                shaped: HashMap::new(),
                outlines: HashMap::new(),
            }),
        }
    }

    /// The engine over an empty font set, for tests of the error paths.
    #[must_use]
    pub fn without_fonts() -> Self {
        Self::new(FontSet::new())
    }

    /// Shapes one paragraph and returns its measurement.
    fn shape_uncached(
        inner: &mut Inner,
        fonts: &FontSet,
        request: &TextRequest<'_>,
    ) -> Result<ShapedText, TextError> {
        fonts.require_family(request.font_family)?;
        match request.direction {
            TextDirection::Auto => {}
            // Parley 0.11 resolves a paragraph's base direction from its own
            // content and exposes no way to force it. Reporting the gap is the
            // honest option: forcing it wrongly would reorder glyphs silently.
            TextDirection::Ltr | TextDirection::Rtl => {
                return Err(TextError::UnsupportedDirection {
                    requested: request.direction,
                });
            }
        }

        let Inner {
            font_context,
            layout_context,
            ..
        } = inner;

        let family = FontFamily::Single(FontFamilyName::Named(std::borrow::Cow::Owned(
            request.font_family.to_owned(),
        )));
        let mut builder: RangedBuilder<'_, [u8; 4]> =
            layout_context.ranged_builder(font_context, request.text, 1.0, false);
        builder.push_default(StyleProperty::FontFamily(family));
        builder.push_default(StyleProperty::FontSize(request.font_size as f32));
        builder.push_default(StyleProperty::FontWeight(FontWeight::new(f32::from(
            request.font_weight,
        ))));

        let mut layout = builder.build(request.text);
        layout.break_all_lines(request.max_width.map(|width| width as f32));
        layout.align(alignment_of(request.align), AlignmentOptions::default());

        let mut lines: Vec<ShapedLine> = Vec::with_capacity(layout.len());
        let mut width: f64 = 0.0;
        let mut height: f64 = 0.0;
        let mut ascent: f64 = 0.0;
        let mut descent: f64 = 0.0;

        for line in layout.lines() {
            let metrics = line.metrics();
            let baseline = f64::from(metrics.baseline);
            // `inline_min_coord` is the left edge of the line's ink and
            // `offset` is the alignment shift. Both are subtracted so a glyph's
            // reported `x` is measured from the line box's own left edge, which
            // is what a caller positions; the line's alignment is reported
            // separately as `ShapedLine::offset` and applied once, by the
            // caller, rather than being baked into every glyph.
            let origin = f64::from(metrics.inline_min_coord);
            let alignment = f64::from(metrics.offset);
            let mut runs: Vec<ShapedRun> = Vec::new();

            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                    // Inline boxes are not part of the M0 slice.
                    continue;
                };
                let run = glyph_run.run();
                let source = source_for_face(fonts, run.font().data.as_ref(), run.font().index)?;
                let mut glyphs: Vec<PositionedGlyph> = Vec::new();
                for glyph in glyph_run.positioned_glyphs() {
                    glyphs.push(PositionedGlyph {
                        id: u16::try_from(glyph.id).map_err(|_| TextError::Backend {
                            message: format!("glyph identity {} does not fit in 16 bits", glyph.id),
                        })?,
                        x: f64::from(glyph.x) - origin - alignment,
                        y: f64::from(glyph.y) - baseline,
                    });
                }
                runs.push(ShapedRun {
                    font: source,
                    font_size: f64::from(run.font_size()),
                    is_rtl: run.is_rtl(),
                    synthesis: TextSynthesis {
                        bold: run.synthesis().embolden(),
                        italic: run.synthesis().skew().is_some(),
                    },
                    glyphs,
                });
            }

            // The line's own metrics: `advance` is the true pen advance and
            // `trailing_whitespace` is the part of it that asks for no room.
            // Parley reports the *alignment* extent separately, which for a
            // wrapped line is the wrap width rather than what the text needs, so
            // the two are not interchangeable.
            let advance = f64::from(metrics.advance);
            let line_width = (advance - f64::from(metrics.trailing_whitespace)).max(0.0);
            width = width.max(line_width);
            height += f64::from(metrics.line_height);
            ascent = ascent.max(f64::from(metrics.ascent));
            descent = descent.max(f64::from(metrics.descent));

            lines.push(ShapedLine {
                start: line.text_range().start,
                end: line.text_range().end,
                baseline,
                height: f64::from(metrics.line_height),
                width: line_width,
                advance,
                offset: f64::from(metrics.offset),
                runs,
            });
        }

        Ok(ShapedText {
            lines,
            width,
            height,
            ascent,
            descent,
        })
    }

    /// Reads one glyph's outline from a registered face.
    fn outline_uncached(
        fonts: &FontSet,
        key: FontKey,
        glyph: u16,
        font_size: f64,
    ) -> Result<Option<GlyphOutline>, TextError> {
        if !font_size.is_finite() || font_size <= 0.0 {
            return Err(TextError::InvalidRequest {
                message: format!("the font size {font_size} is not greater than zero"),
            });
        }
        let source = fonts.source(key).ok_or_else(|| TextError::InvalidRequest {
            message: format!("font {key} is not registered"),
        })?;
        if glyph == 0 {
            // Glyph 0 is the font's `.notdef` box in a missing-glyph situation
            // and is never produced by a shaped run; drawing it would put a
            // stray rectangle on the canvas.
            return Ok(None);
        }

        let font = FontRef::from_index(&source.bytes, source.index).map_err(|error| {
            TextError::Backend {
                message: format!("font {key} could not be read: {error}"),
            }
        })?;
        let Some(outline) = font.outline_glyphs().get(GlyphId::new(u32::from(glyph))) else {
            return Ok(None);
        };

        let mut recorder = PathRecorder::new();
        outline
            .draw(
                skrifa::outline::DrawSettings::unhinted(
                    Size::new(font_size as f32),
                    LocationRef::new(&[]),
                ),
                &mut recorder,
            )
            .map_err(|error| TextError::Backend {
                message: format!("glyph {glyph} could not be drawn: {error}"),
            })?;
        Ok(Some(recorder.finish()))
    }
}

impl TextLayoutEngine for ParleyTextEngine {
    fn fonts(&self) -> &FontSet {
        &self.fonts
    }

    fn shape(&self, request: &TextRequest<'_>) -> Result<Arc<ShapedText>, TextError> {
        request.check()?;
        self.fonts.require_family(request.font_family)?;
        let key = request.cache_key();

        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(cached) = inner.shaped.get(&key) {
            return Ok(Arc::clone(cached));
        }
        let shaped = Arc::new(Self::shape_uncached(&mut inner, &self.fonts, request)?);
        if inner.shaped.len() >= CACHE_LIMIT {
            inner.shaped.clear();
        }
        inner.shaped.insert(key, Arc::clone(&shaped));
        Ok(shaped)
    }

    fn outline(
        &self,
        font: FontKey,
        glyph: u16,
        font_size: f64,
    ) -> Result<Option<Arc<GlyphOutline>>, TextError> {
        let key = OutlineKey {
            font,
            glyph,
            size_bits: font_size.to_bits(),
        };
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(cached) = inner.outlines.get(&key) {
            return Ok(Some(Arc::clone(cached)));
        }
        let Some(outline) = Self::outline_uncached(&self.fonts, font, glyph, font_size)? else {
            return Ok(None);
        };
        let outline = Arc::new(outline);
        if inner.outlines.len() >= CACHE_LIMIT {
            inner.outlines.clear();
        }
        inner.outlines.insert(key, Arc::clone(&outline));
        Ok(Some(outline))
    }
}

/// Finds the registered face whose file and index match what a run was shaped
/// with.
///
/// Parley reports the face it used as font data, not as a caller handle. Rather
/// than trust an index into a parallel list, the face is looked up by the bytes
/// it was registered from, which is what makes a run's [`FontKey`] correct even
/// when the font set grows between shaping and reading.
///
/// When one file is registered under two family names, the runs cannot be told
/// apart by their data — the same bytes produce the same outlines either way —
/// so the first registration of those bytes is reported. The glyphs and their
/// positions are identical, which is what a caller uses this key for.
fn source_for_face(fonts: &FontSet, data: &[u8], index: u32) -> Result<FontKey, TextError> {
    fonts
        .sources()
        .iter()
        .find(|source| source.index == index && source.bytes == data)
        .map(FontSource::key)
        .ok_or_else(|| TextError::Backend {
            // Only a registered face can produce a run, so reaching this means
            // the collection and the font set disagree. Reporting it is better
            // than drawing glyphs with a face that was never registered.
            message: "a shaped run used a font that is not in the set".to_owned(),
        })
}

fn alignment_of(align: TextAlign) -> Alignment {
    match align {
        TextAlign::Start => Alignment::Start,
        TextAlign::Center => Alignment::Center,
        TextAlign::End => Alignment::End,
    }
}
