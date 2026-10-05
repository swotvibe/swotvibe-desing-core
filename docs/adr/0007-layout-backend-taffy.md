# ADR-0007: Layout backend for the M0 slice (Taffy)

- **Status:** accepted, **temporary**
- **Date:** 2026-10-05
- **Owner:** project
- **Spec IDs:** DEC-LAYOUT
- **Recorded under:** the process ADR-0005 defines.

## Context

The layout adapter contract existed as documentation only, so the document's
sizing rules (`fixed`/`fill`/`hug`) had no implementation and no text leaf could
be measured. The M0 gate (§12.2) needs one working layout path, and ADR-0005
requires each backend recorded with its version, license, build result,
fixtures, outcomes, unsupported cases, and exit criteria.

## Decision

Adopt **Taffy 0.14.0** behind `swotvibe_layout::LayoutEngine` as a *temporary*
M0 backend. `DEC-LAYOUT` stays open: Yoga has not been run on the same fixtures,
and the product-specific sizing semantics below are not yet verified against an
alternative.

## Record

| Item | Value |
| --- | --- |
| Crate | `taffy =0.14.0` |
| License | MIT |
| Enabled features | `default-features = false`, `features = ["std", "taffy_tree", "flexbox", "block_layout", "content_size"]` |
| Pinning | Exact (`=`), so a minor release cannot move a reference image silently |
| MSRV of the dependency | 1.71, the lowest of the three M0 backends |
| Builds | Windows x86_64, Linux x86_64, macOS aarch64 (CI matrix) |
| Fixtures | `tests/fixtures/m0-sample-v2.json`, plus the focused cases in `crates/layout/tests/layout.rs` |
| Text measurement | Through `swotvibe_text::TextLayoutEngine` only |

### Configuration choices, and why

- **Rounding off.** `TaffyTree::disable_rounding` keeps every value a design
  unit. Pixel rounding is a renderer's decision at its own boundary, and letting
  layout round would make geometry depend on a scale.
- **Layout owns the box, the transform owns the shape.** The linear part of a
  node's stored transform (rotation, scale, skew) maps the box that layout
  produced; it does not change the box, so a scaled node does not move its
  siblings. This matches CSS transform semantics and is asserted by a test.
- **A transform translation is a position only where layout has no rule.** In a
  flex container, the child's position comes from flexbox and its stored
  translation is ignored, exactly as `NodeProps` documents.
- **Hug is resolved where content exists.** A text leaf is measured; a flex
  container is sized from its children. A `hug` anywhere else (a shape, an
  image, a container whose children are positioned by their own transforms) has
  no content flow to fit, so the stored size is used and a
  `LayoutDiagnostic::SizingFallback` is recorded instead of a silent substitution.

### Observed outcomes

- Fixed, fill, hug, gaps, padding, main-axis and cross-axis alignment, and
  nested flex containers all lay out as the fixtures state.
- A `fill` child takes exactly the free space a fixed sibling left.
- Text leaves measure to the intrinsic text size, and the measured height equals
  the height the renderer draws.
- Two passes over one document produce equal results, including the
  page-space transforms.

### Known unsupported cases, reported rather than approximated

| Case | Behaviour | Why |
| --- | --- | --- |
| `hug` on a shape, image, or transform-positioned container | Stored size used, `SizingFallback` diagnostic | There is no content flow to fit. |
| Percentage sizes and `aspect-ratio` | Not exposed by the model | The M0 property set has no percentages. |
| Grid, block flow, and float | Unused (features left off) | The product's frame layout is flexbox; enabling them would widen the API for nothing. |
| Baseline alignment across mixed fonts | Container-level only | The model has no per-node alignment override yet. |
| Scrollable overflow and clipping | Computed but not used | M0 draws overflow rather than clipping it. |
| Layout cost on large documents | Not measured | An explicit follow-up. |

### Exit criteria

This record stops being sufficient when either happens:

1. `DEC-LAYOUT` is decided by running Yoga (or another engine) over the same
   fixtures and comparing semantics, outputs, build results on the target
   platforms, and cost; or
2. Taffy fails a required platform build, or its license or maintenance status
   changes in a way the project cannot accept.

## Options considered

- **Taffy (chosen for the slice).** Implements flexbox with a style model that
  maps closely onto the product's rules, and exposes a measure function, which
  is what a text leaf needs.
- **Yoga.** Still a candidate for the comparison record; not run yet.
- **A hand-written flexbox.** Rejected: it would own a large, well-specified
  algorithm with no evidence that libraries are insufficient.

## Boundary

Layout reads a `Snapshot` and returns geometry. It never mutates the document
and never returns a reference to the engine that applied a command. Only the
adapter knows Taffy exists: the crate's public API is `NodeId`, design units, and
plain data.

## Evidence

- [Taffy](https://github.com/DioxusLabs/taffy) — API, layout modes, and the
  measure-function contract for 0.14.
- `crates/layout/tests/layout.rs` — the behaviours listed above, run against the
  pinned fonts.

## Risks and constraints

- The exact pin means a security fix requires a deliberate upgrade and a golden
  regeneration.
- Taffy's flexbox is verified against the CSS specification, not against this
  product's documents. Real-scene verification is outstanding.
- A `Hug` container inside a flex parent is measured by Taffy from its in-flow
  children only; a container whose children are absolutely positioned reports
  the stored size under the diagnostic above.

## Change impact

Replacing the engine does not touch the persisted schema: the document stores
sizing rules and transforms, not layout results. A replacement that computes
different boxes changes reference images and layout reports, which is a
fingerprint change.

## Follow-up

- Run the Yoga comparison and close `DEC-LAYOUT`.
- Measure a representative document's layout cost.
- Decide whether `Hug` should be rejected at the model level for kinds where it
  has no meaning, which would remove the diagnostic path entirely.
