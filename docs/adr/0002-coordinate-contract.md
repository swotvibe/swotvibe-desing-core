# ADR-0002: Coordinate and transform contract

- **Status:** proposed
- **Date:** 2026-10-05
- **Owner:** project
- **Spec IDs:** CORE-COORD-01

## Context

The runtime model does not yet store geometry. Coordinate meaning must be defined before geometry enters durable schema or renderer/export APIs.

## Decision

Propose logical design units independent of device pixels, a y-down coordinate system (positive x right, positive y down), and a local two-dimensional affine transform per node. The contract must define parent/child matrix composition, angle direction, finite-value validation, and conversions at rendering/export boundaries before acceptance.

## Options considered

Y-up is mathematically common, while SVG's initial user coordinate system is y-down. The project prefers y-down as an interoperability choice for a design canvas, subject to fixture validation. No claim is made that SVG dictates the project's internal units.

## Boundary

Core stores geometry and local transforms. Renderer, layout, viewport zoom, and export adapters convert coordinates at their boundaries; screen pixels and viewport zoom are not persisted node geometry.

## Evidence

- [SVG 2 coordinate systems](https://www.w3.org/TR/SVG/coords.html) defines abstract user units and the initial y-down direction.
- [`Kurbo::Affine`](https://docs.rs/kurbo/latest/kurbo/struct.Affine.html) documents a six-coefficient affine transform, multiplication convention, and y-down rotation direction.

## Acceptance criteria

Accept only after golden fixtures verify translation, rotation, scale, nested transforms, inversion, and export to SVG. Each fixture must make matrix composition and angle sign observable.

## Risks and constraints

Import formats may use other coordinate conventions. Float precision and normalization rules must be specified. The recommendation is not accepted merely because the library's convention matches it.

## Change impact

Changing persisted transform semantics after schema stabilization requires migration or an explicit format version change.

## Follow-up

Implement and review coordinate fixtures before accepting this ADR or persisting geometry.
