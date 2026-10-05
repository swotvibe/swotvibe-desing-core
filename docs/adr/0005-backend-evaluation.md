# ADR-0005: Bounded evaluation before backend selection

- **Status:** accepted
- **Date:** 2026-10-05
- **Owner:** project
- **Spec IDs:** DEC-RENDERER, DEC-LAYOUT, DEC-TEXT-AR, DEC-BOOLEAN

## Context

The adapter crates define contracts but no implementation has been selected. Official project documentation describes capabilities and status, but does not establish compatibility or performance on this product's scenes and platforms.

## Decision

Run bounded, reproducible evaluations before selecting dependencies:

- **Layout:** Taffy first, Yoga as comparison; measure product-specific sizing semantics, reference outputs, integration cost, and target builds.
- **Text:** Parley first; test Arabic, mixed-direction content, fallback fonts, caret/selection, line wrapping, and IME on target operating systems.
- **Renderer:** Vello CPU as a CPU/reference path; evaluate Vello GPU and Skia on the same scenes and target hardware. Keep the project renderer adapter independent of either library.
- **Boolean:** iCurve first as a candidate; evaluate conversion fidelity, precision boundaries, holes, self-intersections, and runtime on real paths.

No library is accepted by this record. A candidate that fails required features or platform builds is rejected regardless of benchmark speed.

## Options considered

Selecting by feature-list comparison alone was rejected because it does not verify semantic compatibility or end-user rendering quality. Building proprietary layout, text shaping, rasterization, or Boolean algorithms from scratch is not justified before testing available libraries.

## Boundary

The product owns adapters, persisted semantics, validation, cache invalidation, and reference fixtures. Dependencies own their algorithms behind those adapters; their types must not leak into the stable core or persisted schema.

## Evidence

- [Taffy official repository](https://github.com/DioxusLabs/taffy) documents Block/Flexbox/Grid and marks WASM bindings WIP.
- [Yoga official repository](https://github.com/react/yoga) documents its embeddable C++ Flexbox implementation; its [release notes](https://github.com/react/yoga/releases) document WASM and conformance details.
- [Parley official repository](https://github.com/linebender/parley) documents the HarfRust/Skrifa/ICU4X stack and selection/editing utilities.
- [Vello official repository](https://github.com/linebender/vello) distinguishes the maturity profiles of its CPU, GPU, and compute renderers; its [releases](https://github.com/linebender/vello/releases) discuss API stability.
- [iCurve official repository](https://github.com/iShape-Rust/iCurve) documents curved Boolean operations, closed-path requirements, and precision controls.

## Acceptance criteria

Before any backend ADR is accepted, record dependency version/commit, license, enabled features, target build results, fixture set, measured outcomes, known unsupported cases, and exit criteria. The same fixtures and hardware/settings must be used for competing candidates in a domain.

## Risks and constraints

Repositories and release status change. Recheck official docs and exact dependency versions at experiment time. A successful spike does not itself prove commercial fit or production readiness.

## Change impact

Adapter contracts should permit replacement. If a dependency-specific behavior has already entered the file schema, migration impact must be evaluated before adoption.

## Follow-up

Create separate decision records after each experiment with its observed results and selected dependency, if any.
