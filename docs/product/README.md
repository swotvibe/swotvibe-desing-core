# Product documentation

**Product direction confirmed by the owner on 2026-10-16:** a professional,
intelligent design agent capable of competing with expert designers. The first
specialty is marketing designs and social media posts. See the
[product brief and evaluation plan](./design-agent-brief.md).

**Product validation is not documented yet.** The first specialty is known;
the specific customer segment, paid workflow and revenue hypothesis still need
validation. Expert-level quality is a target to measure, not an achieved claim.
Technical milestones alone do not establish it. Representative briefs, licensed
brand assets and independently reviewed outputs are still required.

Add these inputs before selecting product-level performance or schema goals:

- Product requirements and target customer
- Design goals and editor feature set for the first schema
- Reference design files used for evaluation

## Foundation prerequisite

The owner's 2026-10-16 direction is to finish the foundations, including core,
before expanding the design agent. Current work prioritizes document integrity,
ordered atomic commands, reversible compound edits, bounded history retention
and file round trips. See the [core review](../architecture/core-foundation-review-2026-10-16.md)
for verified fixes and remaining gates; this is not a claim that the full design
engine or expert-level agent is complete.

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
- `tests/fixtures/native-authoring/` — an empty-page seed, public authoring
  commands, prepared image resources and the accepted layered composition
  used to verify execution, not expert-level autonomous design.
- `tests/fixtures/document-v2-styled.json` — one node of every kind with
  explicit properties, including an image reference.
- `tests/fixtures/document-v1-minimal.json` — a v1 file, used as the migration
  input.
- `tests/golden/m0-sample.*` — the rendered reference and its layout report.
- `tests/golden/m0-editor-ui.*` — the editor screen render and geometry report;
  its adapter v2 pixels await human review; the earlier 2026-10-06 approval
  does not apply to regenerated pixels.

A licensed product corpus is still required before resource limits, bundle
defaults, or a stable file extension can be published. Tests generate small
synthetic documents, which is not a substitute for that corpus.

## Open questions, in the order they block work

1. **Which marketing customer and repeatable task does the first release serve?**
   Marketing designs and social media posts are the confirmed specialty. The
   initial customer segment, campaign types and delivery expectations still
   need representative briefs before the release feature set is fixed.
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
