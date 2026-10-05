# ADR-0002: Coordinate and transform contract

- **Status:** accepted
- **Date:** 2026-10-05
- **Owner:** project
- **Spec IDs:** CORE-COORD-01
- **Implementation:** `crates/core/src/geometry.rs`, `crates/core/src/props.rs`
- **Fixtures:** `tests/fixtures/coordinate-contract.v1.json` (reviewed by hand)

## Context

The runtime model does not yet store geometry. Coordinate meaning must be defined before geometry enters durable schema or renderer/export APIs.

## Decision

Accepted, with the rules below implemented and covered by committed fixtures.

- **Units.** Lengths are logical **design units**. They are resolution
  independent and are never device pixels. A renderer picks the pixels-per-unit
  factor at export time; viewport zoom is not persisted.
- **Direction.** The coordinate system is **y-down**: the origin is the
  top-left, `+x` grows right, `+y` grows down.
- **Per-node transform.** Each node stores a local 2D affine transform from its
  own space into its parent's space. A page root's parent space is page space.
- **Coefficient order.** `[a, b, c, d, e, f]` maps a point as
  `x' = a·x + c·y + e` and `y' = b·x + d·y + f`. This is the same order as the
  SVG `matrix(a b c d e f)` and the Kurbo/Vello `Affine`, so adapters pass the
  six coefficients through unchanged and no conversion step can silently
  transpose them.
- **Composition.** `parent.compose(&child)` maps a point through `child` first
  and `parent` second, so it yields the child's transform in the parent's
  parent space. A node's transform in page space is the fold of its ancestors'
  transforms from the root down. Composition is associative, and the fixtures
  check a regrouped composition against the stepwise one.
- **Angle sign.** A positive rotation angle turns `+x` toward `+y`. In the
  y-down space that is **clockwise on screen**. The fixture names this
  explicitly so the sign cannot be changed by accident.
- **Finite values.** Every stored number is finite and its magnitude is at most
  `Scalar::MAX_MAGNITUDE` (`1e9`), chosen so the product of two in-range
  scalars cannot overflow `f64`. `NaN` and infinities are rejected at
  construction, and negative zero is normalised to positive zero. A value that
  survives construction is reflexive under equality, which is why the geometry
  types implement `Eq`. Overflow during composition returns an error rather
  than producing an infinity.
- **Inversion.** `Transform::invert` refuses a determinant at or below
  `f64::EPSILON²`, so a degenerate transform is reported as
  `GeometryError::Singular` instead of producing a matrix of infinities.
- **Boundary.** Core stores geometry and local transforms. Renderer, layout,
  viewport zoom, and export adapters convert coordinates at their boundaries;
  screen pixels and viewport zoom are never persisted as node geometry.

## Options considered

Y-up is mathematically common, while SVG's initial user coordinate system is
y-down. The project prefers y-down as an interoperability choice for a design
canvas. No claim is made that SVG dictates the project's internal units; the
choice is confirmed by the fixtures above, which would fail visibly on a sign
error. A per-node matrix over a decomposed (translate/rotate/scale) record was
also considered: a matrix keeps the stored value small, makes composition a
single operation, and does not lose information when an edit mixes rotation and
skew. Nested transform stacks were rejected as duplication, since the parent
relation already supplies the hierarchy.

## Boundary

Core stores geometry and local transforms. Renderer, layout, viewport zoom, and
export adapters convert coordinates at their boundaries; screen pixels and
viewport zoom are not persisted node geometry.

## Evidence

- [SVG 2 coordinate systems](https://www.w3.org/TR/SVG/coords.html) defines abstract user units and the initial y-down direction.
- [`Kurbo::Affine`](https://docs.rs/kurbo/latest/kurbo/struct.Affine.html) documents a six-coefficient affine transform, multiplication convention, and y-down rotation direction.
- `tests/fixtures/coordinate-contract.v1.json` states each case as ops, mapped
  points, and the exported SVG matrix. It is reviewed by hand; the test that
  reads it (`crates/core/tests/coordinate_goldens.rs`) checks mapping,
  coefficients, SVG export, regrouping, and inversion.

## Acceptance criteria

Accepted. The fixtures verify translation, rotation, scale, nested transforms,
inversion, and export to SVG, and each makes composition and the angle sign
observable:

- `translate`, `scale`, `mirrored_scale_flips_orientation` — the linear part.
- `rotate_quarter_turn_is_clockwise_on_screen` and
  `rotate_negative_is_counter_clockwise_on_screen` — the angle sign.
- `nested_translate_rotate_scale`, `nested_four_levels`, `nested_exact_matrices`
  — composition order, including a four-level chain.
- `equivalentOps` on `nested_translate_rotate_scale` — a regrouped op list maps
  every probe point identically, so composition is associative in practice.
- `nested_exact_matrices` — the literal exported SVG text.
- `degenerate_scale_has_no_inverse` — a singular transform is refused, and every
  other case round-trips through its inverse.

## Risks and constraints

Import formats may use other coordinate conventions; a converter at the
boundary is required and is not part of this contract. Float precision is
bounded by the tolerance in the fixture (`1e-9` for mapping and coefficients,
`1e-6` for an inverse round-trip), which is the documented comparison rule for
transforms. An SVG `matrix` string is compared as parsed numbers, except where a
case sets `svgExact`, which pins the literal text of the export format.

## Change impact

Changing persisted transform semantics after schema stabilization requires
migration or an explicit format version change. Changing the coefficient order
or the angle sign would invalidate every committed fixture and any file written
under schema v2, so it needs both a migration and a new fixture version.

## Follow-up

No open item. Later work may add skew, and the boundary converters for SVG
import/export live outside this ADR.
