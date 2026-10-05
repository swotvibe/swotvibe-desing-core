# Golden files

Committed reference outputs for render and layout comparisons. A generated
reference becomes an approved expectation only after the human review below.

Rules:

- Every golden is reviewed by a human before it is committed.
- A comparison records the pinned fonts, backend, and color configuration; a
  backend/font/color change is part of the fingerprint.
- Pixel comparison uses a documented tolerance; different GPUs or operating
  systems are never compared for exact equality.

## Committed goldens

### M0 vertical slice (`m0-sample.*`)

| File | What it is |
| --- | --- |
| `m0-sample.png` | The rasterized sample document, 480x320 px at scale 1. |
| `m0-sample.layout.json` | Every node's offset, size, transform, world matrix, and page-space rectangle. |
| `m0-sample.fingerprint.json` | The backend, scale, colour, pixel tolerance, and font hashes the image was produced with. |

The input is `tests/fixtures/m0-sample-v2.json`, and the test that produces and
compares them is `crates/tools/tests/m0_vertical_slice.rs`.

**Review status: the image is generated and compared, but no human has approved
it yet.** `m0-sample.fingerprint.json` records this in its `reviewed` field
rather than leaving it implicit, and the M0 gate is not fully passed until that
field names a reviewer. What the review has to cover:

- The card, its rounded corners, and its 1-unit stroke are where the report says.
- The Latin line and the Arabic line are legible, correctly ordered, and not
  clipped by their boxes.
- The Arabic line reads right to left with its letters joined as the font
  intends, not as isolated forms in visual order.
- The red circle is round at this size, and no edge is visibly stepped or jagged
  beyond ordinary anti-aliasing.

### Arabic editor UI (`m0-editor-ui.*`)

| File | What it is |
| --- | --- |
| `m0-editor-ui.png` | Static 1440×900 Arabic editor screen, including decoded SVG/PNG assets. |
| `m0-editor-ui.layout.json` | Geometry for every document node at scale 1. |
| `m0-editor-ui.fingerprint.json` | Backend, font and asset hashes, viewport, background, tolerance and review state. |

Input: `tests/fixtures/m0-editor-ui-v2.json`; generator:
`python tests/fixtures/generate_m0_editor_ui.py`; integration test:
`crates/tools/tests/m0_editor_ui.rs`. Regenerate with
`SWOTVIBE_UPDATE_EDITOR_GOLDEN=1 cargo test -p swotvibe-tools --test m0_editor_ui`.
The fingerprint currently has `reviewed: null`: this generated fixture is not a
human-approved product design. Geometry is compared exactly as serialized and
pixels use the same per-channel tolerance of 2.

#### Regenerating

```text
SWOTVIBE_UPDATE_GOLDEN=1 cargo test -p swotvibe-tools --test m0_vertical_slice
```

Regeneration rewrites all three files. A regenerated image without a fresh
human review is not an approved reference, and a diff that only changes the
`reviewed` field is not a review.

#### Comparison rules

- The layout report is compared field by field: geometry to `1e-6` design units,
  transforms to `1e-9`. A difference names the node path and the value, so a
  regression does not have to be found by eye.
- The fingerprint is compared exactly, minus the `reviewed` field. A backend,
  font, scale, or colour change is a fingerprint change, not a tolerance
  question.
- Pixels are compared with a tolerance of **2** in every channel, with a count
  of pixels beyond it. The tolerance covers anti-aliasing arithmetic that
  differs between a fresh render and a stored one; it is deliberately small
  enough that a missing glyph or a moved box fails.
- The comparison never spans operating systems for exact equality. The test
  prefers a platform-specific reference (`m0-sample.windows.png`,
  `m0-sample.macos.png`, `m0-sample.linux.png`) when one is committed, and falls
  back to the shared `m0-sample.png`. When a platform diverges, commit that
  platform's own reviewed reference rather than widening the tolerance; the test
  failure message says so.
