# ADR-0006: Text backend for the M0 slice (Parley)

- **Status:** accepted, **temporary**
- **Date:** 2026-10-05
- **Owner:** project
- **Spec IDs:** DEC-TEXT-AR
- **Supersedes:** nothing. Recorded under the process ADR-0005 defines.

## Context

The text adapter contract existed as documentation only: no shaping, no
measurement, and no glyph outlines, so nothing downstream could lay out or draw
text. The M0 gate (§12.2) needs one working text path with known fonts, and
ADR-0005 requires each backend recorded with its version, license, build result,
fixtures, outcomes, unsupported cases, and exit criteria.

## Decision

Adopt **Parley 0.11.1** behind `swotvibe_text::TextLayoutEngine`, and read glyph
outlines with **Skrifa 0.44.0** — both as a *temporary* M0 backend. The decision
is explicitly not final: `DEC-TEXT-AR` stays open until the Arabic, IME, and
fallback requirements below are tested on all target platforms.

## Record

| Item | Value |
| --- | --- |
| Crates | `parley =0.11.1`, `skrifa =0.44.0` |
| Licenses | `Apache-2.0 OR MIT` (both) |
| Enabled features | `default-features = false`, `features = ["std"]` |
| Pinning | Exact (`=`), because this stack's MSRV moves in patch releases |
| MSRV of the dependency | `parley` 1.88, `skrifa` 1.85; the workspace builds with 1.99.0, so the dependency floor is below the build toolchain but is the highest of the three M0 backends |
| Builds | Windows x86_64, Linux x86_64, macOS aarch64 (CI matrix) |
| Fixtures | `assets/fonts/inter`, `assets/fonts/noto-sans-arabic`, `tests/fixtures/m0-sample-v2.json` |
| Tests | `crates/text/src/contract.rs`, `crates/text/tests/shaping.rs` |

### Configuration choices, and why

- **No system fonts.** The collection is built with `system_fonts: false`, and
  only the faces of an explicit `FontSet` are registered. A reference test that
  could pick up the host's fonts is not reproducible, so the engine cannot see
  them at all.
- **No quantization.** Parley is asked for unrounded advances, so a measurement
  is arithmetic rather than a pixel-snapped approximation.
- **Unhinted outlines.** `DrawSettings::unhinted` keeps glyph outlines free of
  any hinting implementation, which removes a per-machine variable from a
  reference image.
- **Quadratic curves preserved.** A `QuadTo` from the font stays quadratic
  across the adapter boundary, so no tolerance is introduced by elevating it.
- **Ink extents for boxes.** A line's width is its pen advance minus trailing
  whitespace, so a hug-sized node does not grow for a trailing space.

### Observed outcomes

- Inter Latin and Noto Sans Arabic shape, including contextual Arabic forms: four repetitions of
  one letter shape into more than one distinct glyph.
- An Arabic paragraph resolves right-to-left from its own content.
- Wrapping, alignment, and line ranges behave as documented, and a measurement
  is identical across engines and repeated calls.
- Outlines scale linearly with the requested size, and a space has an advance
  with no ink.
- A repeated request is served from the engine's cache.

### Known unsupported cases, reported rather than approximated

| Case | Behaviour | Why |
| --- | --- | --- |
| `TextDirection::Ltr` or `Rtl` (a forced paragraph base direction) | `TextError::UnsupportedDirection` | Parley 0.11.1 resolves a paragraph's base direction from its own content and exposes no way to force it. Treating a forced direction as `Auto` would silently reorder glyphs. |
| Font fallback beyond the registered set | `TextError::UnknownFamily` | There are no system fonts by construction, so an unregistered family is a typed error rather than a surprise substitute. |
| Inline boxes | Not modelled | Out of the M0 slice. |
| IME, caret, and selection | Not tested | Required before `DEC-TEXT-AR` can be closed. |
| Word-level wrapping metrics beyond Parley's line breaking | Not verified | Acceptable for M0; a product-level review of orphan/widow behaviour is outstanding. |

### Exit criteria

This record stops being sufficient when either happens:

1. `DEC-TEXT-AR` is decided by a comparison against at least one alternative on
   the same fixtures, including Arabic shaping, mixed direction, fallback, IME,
   caret/selection, and wrapping; or
2. Parley fails a required platform build, or its MSRV or license changes in a
   way the project cannot accept.

## Options considered

- **Parley (chosen for the slice).** Provides shaping (HarfRust), font matching
  (Fontique), and line breaking behind one API, plus outline access through
  Skrifa, which is what the renderer needs without a second font stack.
- **Swashes/custom shaping.** Rejected: it would mean owning script
  itemization, bidi, and shaping, which is not justified before testing an
  existing library.
- **No text at all in M0.** Rejected: text is the part of the slice most likely
  to expose a wrong contract, and Arabic is a project requirement.

## Boundary

The engine speaks design units and [`PathCommand`] values only. No Parley type
appears in the crate's public API, and no Parley type is persisted: the file
schema carries the resolved style values (family, size, weight, direction,
alignment) and nothing else.

## Evidence

- [Parley](https://github.com/linebender/parley) — API and feature surface for
  0.11.
- [Skrifa](https://github.com/googlefonts/fontations) — outline access and
  `DrawSettings`.
- `crates/text/tests/shaping.rs` — the behaviours listed above, run against the
  pinned fonts.

## Risks and constraints

- Pinning is exact and the crates are upgraded deliberately, not automatically.
- The engine holds caches bounded by a fixed entry count; a very large document
  may clear them, which costs a re-shape but never changes a result.
- The adapter is not proven thread-safe for a shared worker pool. The host owns
  that decision; the trait makes no `Send` or `Sync` promise.

## Change impact

Replacing this backend does not touch the persisted schema, because the schema
stores resolved style values rather than backend state. A replacement that
shapes differently changes measurements and therefore reference images, which is
a fingerprint change, not a migration.

## Follow-up

- Decide `DEC-TEXT-AR` with a comparison record.
- Measure shaping and layout cost on a representative document.
- Test IME, caret, and selection before any editing UI depends on this adapter.
