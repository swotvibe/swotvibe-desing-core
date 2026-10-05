//! Shaping, measurement, and outlines with the pinned fonts.
//!
//! These tests are the acceptance evidence for the text adapter contract: they
//! show that Latin and Arabic shape, that an RTL paragraph is laid out in
//! visual order, that measurement is stable across runs, and that the error
//! paths stay typed.

#[path = "support/mod.rs"]
mod support;

use std::sync::Arc;

use support::{ARABIC_FAMILY, LATIN_FAMILY, pinned_fonts};
use swotvibe_core::{TextAlign, TextDirection};
use swotvibe_text::{
    ParleyTextEngine, PathCommand, ShapedText, TextError, TextLayoutEngine, TextRequest,
};

fn request<'a>(text: &'a str, family: &'a str, size: f64) -> TextRequest<'a> {
    TextRequest {
        text,
        font_family: family,
        font_size: size,
        font_weight: 400,
        direction: TextDirection::Auto,
        align: TextAlign::Start,
        max_width: None,
    }
}

fn engine() -> ParleyTextEngine {
    ParleyTextEngine::new(pinned_fonts())
}

#[test]
fn latin_text_shapes_into_positioned_glyphs() {
    let engine = engine();
    let shaped = engine
        .shape(&request("Hello", LATIN_FAMILY, 16.0))
        .expect("Latin text should shape");

    assert_eq!(shaped.lines.len(), 1);
    let line = &shaped.lines[0];
    assert_eq!(line.runs.len(), 1);
    let run = &line.runs[0];
    assert_eq!(run.glyphs.len(), 5);
    assert!(!run.is_rtl);
    assert!(
        shaped.width > 0.0 && shaped.height > 0.0,
        "a measurement should have a positive extent, found {shaped:?}"
    );
    // Glyph x positions advance left to right.
    for pair in run.glyphs.windows(2) {
        assert!(
            pair[1].x >= pair[0].x,
            "positions should advance in LTR order: {:?}",
            run.glyphs
        );
    }
}

#[test]
fn arabic_text_shapes_into_a_right_to_left_run() {
    let engine = engine();
    let shaped = engine
        .shape(&request("مرحبا", ARABIC_FAMILY, 20.0))
        .expect("Arabic text should shape");

    let line = &shaped.lines[0];
    assert_eq!(line.runs.len(), 1, "one family means one run");
    let run = &line.runs[0];
    assert!(run.is_rtl, "an Arabic paragraph should resolve as RTL");
    assert!(
        run.glyphs.len() >= 5,
        "five letters should produce at least five glyphs, found {:?}",
        run.glyphs
    );
    assert!(
        shaped.width > 0.0,
        "Arabic text should have a positive width, found {}",
        shaped.width
    );
}

#[test]
fn a_mixed_arabic_and_latin_paragraph_keeps_both_bidi_runs() {
    let engine = engine();
    let shaped = engine
        .shape(&request(
            "استخدم Auto Layout داخل Frame ٣",
            ARABIC_FAMILY,
            18.0,
        ))
        .expect("mixed-direction Arabic and Latin should shape");

    let runs: Vec<_> = shaped.lines.iter().flat_map(|line| &line.runs).collect();
    assert!(
        runs.iter().any(|run| run.is_rtl),
        "Arabic runs should be RTL"
    );
    assert!(
        runs.iter().any(|run| !run.is_rtl),
        "Latin terms and digits should retain LTR runs"
    );
    assert!(
        shaped.width > 0.0 && shaped.height > 0.0,
        "mixed text should have a measurable extent, found {shaped:?}"
    );
}

#[test]
fn arabic_shaping_uses_contextual_forms() {
    // The point of a real shaper is that a letter is not the same glyph in every
    // position. Asserting exact glyph identities would over-specify the font, so
    // the check is that a run of one repeated letter is not mapped to one
    // repeated glyph, which is what a shaper that ignored joining would do.
    let engine = engine();
    let shaped = engine
        .shape(&request("بببب", ARABIC_FAMILY, 20.0))
        .expect("Arabic text should shape");
    let ids: Vec<u16> = shaped.lines[0].runs[0]
        .glyphs
        .iter()
        .map(|glyph| glyph.id)
        .collect();
    let distinct: std::collections::BTreeSet<u16> = ids.iter().copied().collect();
    assert!(
        distinct.len() > 1,
        "contextual shaping should distinguish positions, found {ids:?}"
    );
}

#[test]
fn measurement_is_stable_across_engines_and_repeated_calls() {
    let first = engine()
        .shape(&request("Measurement", LATIN_FAMILY, 18.0))
        .expect("shape");
    let second = engine()
        .shape(&request("Measurement", LATIN_FAMILY, 18.0))
        .expect("shape");
    assert_eq!(*first, *second, "the same request must measure identically");

    let engine = engine();
    let a = engine
        .shape(&request("Measurement", LATIN_FAMILY, 18.0))
        .expect("shape");
    let b = engine
        .shape(&request("Measurement", LATIN_FAMILY, 18.0))
        .expect("shape");
    assert!(
        Arc::ptr_eq(&a, &b),
        "a repeated request should be served from the cache"
    );
}

#[test]
fn wrapping_breaks_a_long_line_and_reports_each_line_range() {
    let engine = engine();
    let text = "The quick brown fox jumps over the lazy dog";
    let mut request = request(text, LATIN_FAMILY, 16.0);
    request.max_width = Some(80.0);
    let shaped: Arc<ShapedText> = engine.shape(&request).expect("shape");

    assert!(
        shaped.lines.len() > 1,
        "an 80-unit wrap should produce several lines, found {}",
        shaped.lines.len()
    );
    assert!(
        shaped.width <= 80.0 + 1.0,
        "no line should be wider than the wrap, found {}",
        shaped.width
    );
    let mut covered = 0usize;
    for line in &shaped.lines {
        assert!(line.start < line.end, "a line should cover some text");
        assert_eq!(line.start, covered, "line ranges should be contiguous");
        covered = line.end;
    }
    assert_eq!(covered, text.len(), "every byte should be on some line");
}

#[test]
fn alignment_offsets_the_lines_without_changing_their_width() {
    let engine = engine();
    let mut start = request("Centered", LATIN_FAMILY, 16.0);
    start.max_width = Some(200.0);
    let mut center = start.clone();
    center.align = TextAlign::Center;
    let mut end = start.clone();
    end.align = TextAlign::End;

    let start = engine.shape(&start).expect("shape");
    let center = engine.shape(&center).expect("shape");
    let end = engine.shape(&end).expect("shape");

    assert!(center.lines[0].offset > start.lines[0].offset);
    assert!(end.lines[0].offset > center.lines[0].offset);
    assert!((center.lines[0].width - start.lines[0].width).abs() < 1e-9);
    assert!((end.lines[0].width - start.lines[0].width).abs() < 1e-9);

    // The alignment shift is reported on the line and is *not* baked into every
    // glyph, so a caller applies it exactly once.
    for shaped in [&start, &center, &end] {
        let first = shaped.lines[0].runs[0].glyphs[0];
        assert!(
            first.x.abs() < 1.0,
            "the first glyph should sit at the line's own origin, found {}",
            first.x
        );
    }
}

#[test]
fn an_unregistered_family_is_a_typed_error() {
    let engine = engine();
    let error = engine
        .shape(&request("Hello", "Comic Sans", 16.0))
        .expect_err("an unknown family must fail");
    match error {
        TextError::UnknownFamily { family, registered } => {
            assert_eq!(family, "Comic Sans");
            assert!(registered.contains(&LATIN_FAMILY.to_owned()));
        }
        other => panic!("expected UnknownFamily, found {other:?}"),
    }
}

#[test]
fn an_empty_font_set_reports_every_family_as_unregistered() {
    let engine = ParleyTextEngine::without_fonts();
    assert!(engine.fonts().is_empty());
    assert!(engine.shape(&request("Hello", LATIN_FAMILY, 16.0)).is_err());
}

#[test]
fn a_forced_paragraph_direction_is_refused_rather_than_guessed() {
    let engine = engine();
    for direction in [TextDirection::Ltr, TextDirection::Rtl] {
        let mut request = request("Hello", LATIN_FAMILY, 16.0);
        request.direction = direction;
        assert_eq!(
            engine.shape(&request).err(),
            Some(TextError::UnsupportedDirection {
                requested: direction
            })
        );
    }
}

#[test]
fn an_invalid_request_is_refused_before_shaping() {
    let engine = engine();
    let mut request = request("Hello", LATIN_FAMILY, 0.0);
    assert!(matches!(
        engine.shape(&request).err(),
        Some(TextError::InvalidRequest { .. })
    ));
    request.font_size = 16.0;
    request.font_weight = 10_000;
    assert!(matches!(
        engine.shape(&request).err(),
        Some(TextError::InvalidRequest { .. })
    ));
}

#[test]
fn an_empty_string_measures_as_a_single_empty_line() {
    let engine = engine();
    let shaped = engine
        .shape(&request("", LATIN_FAMILY, 16.0))
        .expect("shape");
    assert_eq!(shaped.width, 0.0);
    assert_eq!(shaped.lines.len(), 1);
    assert!(shaped.lines[0].runs.is_empty());
}

#[test]
fn a_latin_glyph_has_a_quadratic_or_cubic_outline_and_closes() {
    let engine = engine();
    let shaped = engine
        .shape(&request("A", LATIN_FAMILY, 100.0))
        .expect("shape");
    let glyph = shaped.lines[0].runs[0].glyphs[0];

    let outline = engine
        .outline(shaped.lines[0].runs[0].font, glyph.id, 100.0)
        .expect("the outline should be readable")
        .expect("`A` has an outline");
    assert!(!outline.is_empty(), "`A` should have ink: {outline:?}");
    assert!(outline.contours >= 1);
    assert!(
        outline
            .commands
            .iter()
            .any(|command| matches!(command, PathCommand::QuadTo { .. })),
        "a TrueType glyph should keep its quadratic segments"
    );
    assert_eq!(
        outline.commands.last(),
        Some(&PathCommand::Close),
        "a contour should be closed"
    );
}

#[test]
fn an_outline_scales_with_the_requested_size() {
    let engine = engine();
    let font = engine
        .fonts()
        .faces_of(LATIN_FAMILY)
        .first()
        .expect("the Latin face is registered")
        .key();
    let shaped = engine
        .shape(&request("A", LATIN_FAMILY, 100.0))
        .expect("shape");
    let glyph = shaped.lines[0].runs[0].glyphs[0].id;

    let small = engine
        .outline(font, glyph, 10.0)
        .expect("outline")
        .expect("`A` has an outline");
    let large = engine
        .outline(font, glyph, 100.0)
        .expect("outline")
        .expect("`A` has an outline");

    let extent = |outline: &swotvibe_text::GlyphOutline| {
        outline
            .commands
            .iter()
            .map(|command| match *command {
                PathCommand::MoveTo { y, .. }
                | PathCommand::LineTo { y, .. }
                | PathCommand::QuadTo { y, .. } => y,
                PathCommand::CubicTo { y, .. } => y,
                // `PathCommand` is non-exhaustive on purpose: a later variant
                // must not break this test, and a command with no point
                // contributes nothing to the extent.
                _ => 0.0,
            })
            .fold(0.0f64, f64::max)
    };
    let ratio = extent(&large) / extent(&small);
    assert!(
        (ratio - 10.0).abs() < 0.1,
        "the outline should scale with the size, found a ratio of {ratio}"
    );
}

#[test]
fn glyph_zero_and_a_missing_glyph_have_no_outline() {
    let engine = engine();
    let font = engine
        .fonts()
        .faces_of(LATIN_FAMILY)
        .first()
        .expect("the Latin face is registered")
        .key();
    assert_eq!(engine.outline(font, 0, 16.0).expect("lookup"), None);
    // Glyph 65535 is beyond the font's glyph count.
    assert_eq!(engine.outline(font, u16::MAX, 16.0).expect("lookup"), None);
}

#[test]
fn a_space_has_no_ink_and_does_not_widen_a_line() {
    let engine = engine();
    let shaped = engine
        .shape(&request(" ", LATIN_FAMILY, 16.0))
        .expect("shape");

    // The measured width is the ink extent, so a paragraph of one space is
    // zero wide. That is what keeps a hug-sized node from growing for trailing
    // whitespace, and it is why the width cannot be used as a pen advance.
    assert_eq!(shaped.width, 0.0);
    let run = &shaped.lines[0].runs[0];
    assert_eq!(run.glyphs.len(), 1, "the space should still be one glyph");
    let outline = engine
        .outline(run.font, run.glyphs[0].id, 16.0)
        .expect("lookup")
        .expect("the space glyph exists");
    assert!(
        outline.is_empty(),
        "a space should have no ink, found {:?}",
        outline.commands
    );
}

#[test]
fn a_trailing_space_does_not_widen_a_measured_line() {
    let engine = engine();
    let bare = engine
        .shape(&request("Hello", LATIN_FAMILY, 16.0))
        .expect("shape");
    let padded = engine
        .shape(&request("Hello ", LATIN_FAMILY, 16.0))
        .expect("shape");
    // The two are computed by different subtractions inside the backend, so the
    // comparison is to a fraction of a design unit rather than to the bit.
    assert!(
        (bare.width - padded.width).abs() < 0.01,
        "a trailing space should not widen the line: {} against {}",
        bare.width,
        padded.width
    );
    assert!(
        padded.lines[0].advance > bare.lines[0].advance,
        "a trailing space should still advance the pen"
    );
}
