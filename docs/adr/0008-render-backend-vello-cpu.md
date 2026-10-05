# ADR-0008: Render backend for the M0 slice (vello_cpu)

- **Status:** accepted, **temporary**
- **Date:** 2026-10-05
- **Owner:** project
- **Spec IDs:** DEC-RENDERER
- **Recorded under:** the process ADR-0005 defines.

## Context

The render adapter contract existed as documentation only: no backend, no pixels,
and therefore no reference image. The M0 gate (§12.2) needs one renderer that
produces a PNG from a laid out page, and ADR-0005 requires each backend recorded
with its version, license, build result, fixtures, outcomes, unsupported cases,
and exit criteria.

## Decision

Adopt **vello_cpu 0.3.0** behind `swotvibe_render::Renderer` as a *temporary* M0
backend, with **png 0.18.1** for the image codec, **resvg 0.48.1** for bounded
SVG rasterization, and **kurbo**/*peniko* (through vello_cpu's re-exports) for
geometry and colour. `DEC-RENDERER` stays open: Vello GPU, Skia, and
platform-native paths have not been run on the same scenes.

## Record

| Item | Value |
| --- | --- |
| Crates | `vello_cpu =0.3.0`, `png =0.18.1`, `resvg =0.48.1`, `kurbo`/`peniko` via vello_cpu |
| Licenses | `Apache-2.0 OR MIT` (vello_cpu, kurbo, peniko, resvg), `MIT OR Apache-2.0` (png) |
| Enabled features | `vello_cpu`: `default-features = false`, `features = ["std", "f32_pipeline"]` — the `text` (glifo) feature is **not** used |
| Pinning | Exact (`=`) for vello_cpu, so the pipeline cannot change under a reference image |
| Build toolchain | Rust 1.99.0 pinned in `rust-toolchain.toml`; no public MSRV promise is made by this ADR |
| Builds | Windows x86_64, Linux x86_64, macOS aarch64 (CI matrix) |
| Fixtures | `m0-sample.*` technical flow; `m0-editor-ui.*` Arabic editor, SVG icons, PNG product asset |

### Configuration choices, and why

- **Quality pipeline.** `RenderMode::OptimizeQuality` selects the f32 pipeline,
  which is the accurate path; the M0 slice is a reference image, not a preview
  loop.
- **Baseline SIMD.** The rasterizer level is pinned to `Level::baseline()` so an
  edge cannot depend on the host CPU's vector width. This is slower and
  deliberate: a golden must not change between two machines.
- **Single-threaded.** The multithreading feature is off, so tile scheduling
  cannot influence the result.
- **Background through the backend's own clear.** The backdrop is a
  `TargetInit::Clear` colour rather than a first drawn rectangle, so the colour
  is composited by the same path for every pixel.
- **Straight-alpha boundary.** The backend rasterizes premultiplied; the adapter
  converts to straight-alpha RGBA8 once, because that is the form a file, a
  comparison, and a human share.
- **Text outside the backend's text feature.** Glyphs are drawn by filling
  outlines obtained through the text contract, not through vello_cpu's
  experimental `glifo` integration. That keeps one shaping path for measurement
  and drawing, and one less experimental dependency.
- **Geometry tolerance is fixed.** Shape paths are converted with a constant
  tolerance in design units, so the same document always produces the same path.

### Observed outcomes

- Shapes, strokes, rounded corners, ellipses, and text all rasterize, at scale 1
  and at scale 2, with the expected pixel extents.
- Two renders of one scene are byte-identical, including the encoded PNG.
- The sample document renders with no diagnostic and no missing content.
- The static editor UI fixture renders at 1440×900 with no layout or render
  diagnostics. It uses checked-in Inter and Noto Sans Arabic font files, seven
  generated SVG icons, and one generated product PNG. Decode budgets are
  explicit test limits, not product defaults.
- The SVG profile is path-focused, uses resvg with default features disabled,
  and does not enable scripts, external resources, or embedded raster images.
- The comparison used by the reference test (`RenderedImage::compare`) reports
  the number of pixels beyond the tolerance and the worst channel difference.

### Known unsupported cases, reported rather than approximated

| Case | Behaviour | Why |
| --- | --- | --- |
| An image node without supplied bytes | `UnresolvedImage` diagnostic, nothing drawn | An asset reference alone has no image payload. |
| Unsupported or oversized PNG/SVG | Typed decode error within explicit caller limits | The M0 reader accepts bounded PNG and a restricted SVG subset; it is not a general-purpose asset importer. |
| A text node whose style needs synthesis (for example weight 700 with a regular-only face) | `SynthesizedStyle` diagnostic; the shipped outlines are drawn | The adapter does not fake a bold or an oblique. Drawing the shipped face and saying so is better than an unrecorded approximation. |
| Text overflow beyond its box | Drawn, not clipped | Clipping and scroll regions are not part of M0. |
| A node with neither fill nor stroke | `NoPaint` diagnostic, nothing drawn | Distinguishes "nothing to draw" from "failed to draw". |
| Gradients, filters, blend modes, clipping, and layer effects | Not modelled | Outside the M0 property set. |
| GPU and platform-native rendering | Not implemented | The CPU path is the reference; a GPU comparison is outstanding. |
| Colour management beyond sRGB | Not implemented | sRGB is the document's colour space and the fingerprint records it. |

### Exit criteria

This record stops being sufficient when either happens:

1. `DEC-RENDERER` is decided by running the same scenes through Vello GPU, Skia,
   and this CPU path, comparing fidelity, cost, and deployment constraints; or
2. vello_cpu fails a required platform build, or its API or license changes in a
   way the project cannot accept.

## Options considered

- **vello_cpu (chosen for the slice).** A CPU rasterizer with an f32 pipeline
  gives a reference image with no GPU or driver in the loop, which is what a
  reproducibility claim needs.
- **Vello GPU / wgpu.** Faster on real scenes, but introduces a driver,
  adapter selection, and a GPU vendor into a reference run. Kept as a candidate
  for the product path, not for the golden.
- **Skia.** Mature and complete, but a heavier dependency and a different
  licensing and build story; still a candidate for the comparison record.
- **A hand-written rasterizer.** Rejected: the project needs a reference of its
  contracts, not a new graphics library.

## Boundary

The renderer reads a snapshot, a layout result, and a configuration. It never
mutates the document, never owns scene semantics, and never sees Parley or Taffy
types: glyphs and boxes cross into it as plain numbers and path commands.

## Evidence

- [vello_cpu](https://github.com/linebender/vello) — render context, pipeline
  modes, and pixmap formats for 0.3.
- [png](https://github.com/image-rs/image-png) — codec API and determinism.
- `crates/render/tests/render.rs`, `crates/tools/tests/m0_vertical_slice.rs`,
  and `crates/tools/tests/m0_editor_ui.rs` — the behaviours listed above.

## Risks and constraints

- The `f32_pipeline` path is documented as the accurate one, and is slower. It is
  for reference, not for interactive use.
- Pinning to the baseline SIMD level costs throughput that an interactive
  preview would want; a preview path would need its own decision and its own
  tolerance.
- Anti-aliasing arithmetic may still differ across operating systems and CPU
  architectures. The reference comparison is therefore per-OS, with a documented
  tolerance, and never a cross-OS byte comparison.
- vello_cpu is pre-1.0. The adapter's public API is plain data, so a replacement
  is contained.

## Change impact

Replacing the renderer does not touch the persisted schema. A replacement that
rasterizes differently changes the reference image, which is a fingerprint
change: `tests/golden/m0-sample.fingerprint.json` names the backend and the
pipeline, so the change cannot pass unnoticed.

## Follow-up

- Decide `DEC-RENDERER` with a comparison against Vello GPU and Skia.
- Run the editor UI golden on the full CI platform matrix and review its image
  against the product brief before marking the fingerprint approved.
- Measure a full-page reference render's cost, and decide whether a separate
  interactive path is required.
