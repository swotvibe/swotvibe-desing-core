# Product documentation

**Product validation is not documented yet.** The target customer, paid task,
and revenue hypothesis are still unknown. Technical milestones cannot by
themselves demonstrate commercial viability; performance and resource targets
remain undetermined until the intended workflow and reference files are known.

Add these inputs before selecting product-level performance or schema goals:

- Product requirements and target customer
- Design goals and editor feature set for the first schema
- Reference design files used for evaluation

## What the engineering side can state today

These are facts about the repository, not product claims. They are separated
from the open questions above so that a technical milestone is never mistaken for
evidence of demand.

| Statement | Evidence |
| --- | --- |
| A document can be created, saved, reopened, and edited with reversible transactions. | `crates/core`, `crates/format`, and their tests |
| The persisted schema rejects a malformed or newer file instead of guessing. | `crates/format/tests/round_trip.rs`, `schema_v2.rs` |
| One page can be laid out and rasterized to a PNG, with the geometry and the backend recorded. | `crates/tools/tests/m0_vertical_slice.rs`, `tests/golden` |
| Arabic text shapes and renders through the pinned font. | `crates/text/tests/shaping.rs` |
| The layout and render backends are temporary and unselected. | ADR-0006, ADR-0007, ADR-0008 |

Not established by any of the above: that anyone wants this editor, that the
chosen interaction model is workable, that the file format suits a real customer
workflow, or that performance is acceptable on a real document.

## Evaluation inputs available now

Committed reference inputs exist for technical evaluation, and none of them is a
customer document:

- `tests/fixtures/m0-sample-v2.json` — the M0 sample: a frame with a shape, two
  text nodes (Latin and Arabic), and a second shape.
- `tests/fixtures/m0-editor-ui-v2.json` — a generated Arabic-first editor
  screen with nested layer content, SVG icons, and a PNG product illustration.
- `tests/fixtures/document-v2-styled.json` — one node of every kind with
  explicit properties, including an image reference.
- `tests/fixtures/document-v1-minimal.json` — a v1 file, used as the migration
  input.
- `tests/golden/m0-sample.*` — the rendered reference and its layout report.
- `tests/golden/m0-editor-ui.*` — the editor screen render and geometry report;
  its `reviewed` field records a visual approval dated 2026-10-06, which makes
  the render a comparison reference rather than an accepted product design.

A licensed product corpus is still required before resource limits, bundle
defaults, or a stable file extension can be published. Tests generate small
synthetic documents, which is not a substitute for that corpus.

## Open questions, in the order they block work

1. **Who is the customer, and which paid task does the first release serve?**
   Nothing about the feature set, the schema's stable fields, or the reference
   corpus can be settled first.
2. **Which design files represent that task?** Their size, nesting, and asset
   mix decide the resource limits and the bundle profile's defaults.
3. **Which editing operations are in scope for the first release?** The M0
   property set (geometry, transform, fill, stroke, shape geometry, text, flex
   frames) is a starting point, not a decided feature list.
4. **What performance is required?** Target timings for opening, laying out, and
   rendering a representative file are unknown, so no performance work is
   justified yet.
5. **Which platforms ship first?** The CI matrix covers three, but the supported
   set, the minimum OS versions, and the MSRV policy are undecided.
