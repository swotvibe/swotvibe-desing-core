# ADR-0001: M0 model invariants and identity

- **Status:** accepted
- **Date:** 2026-10-05
- **Owner:** project
- **Spec IDs:** CORE-MODEL-01, CORE-COMP-01, CORE-TREE-01, CORE-STORE-01, CORE-ID-01

## Context

The M0 kernel has a working document tree, stable identity, and versioned schema DTOs. The specification previously left several of these implementation choices unresolved. This record states the M0 baseline; it does not freeze the full product schema or claim performance superiority.

## Decision

- M0 structural node kinds are `Frame`, `Group`, `Shape`, `Text`, and `Image`. Their complete editable property sets remain product work and must be specified before schema v1 is declared stable.
- Ordered child lists owned by their parent are the canonical representation of sibling order and parentage. The parent's lookup index is derived and rebuildable. Fractional order keys are out of scope unless concurrent collaboration becomes a confirmed product requirement.
- Project-specific ID newtypes wrap UUID v4. IDs never encode layer order. Imports validate and reject collisions rather than silently remapping identities.
- The M0 node store remains `HashMap<NodeId, Node>` with ordered `Vec<NodeId>` child lists. An arena or slot map requires benchmark evidence on representative documents and may not leak temporary handles into persisted files or the public API.
- Components, instances, overrides, and variables are excluded from the first durable schema until a launch requirement defines their identity, override key, nesting, and migration behavior.

## Options considered

The current parent-owned order list was selected over storing parent and fractional order key on each node because it is implemented and sufficient for single-user M0. UUID v4 was selected over v7 because temporal ordering is not document semantics. The existing map representation is retained pending measurements rather than being declared faster. Components and variables are deferred rather than represented by incomplete fields.

## Boundary

The core owns durable document identities, tree invariants, and command validation. Runtime indexes and storage handles are derived implementation details. Format DTOs remain separate from runtime types.

## Evidence

- Current implementation: [`model.rs`](../../crates/core/src/model.rs), [`ids.rs`](../../crates/core/src/ids.rs), [`dto.rs`](../../crates/format/src/dto.rs).
- [UUID RFC 9562](https://www.rfc-editor.org/rfc/rfc9562.html) and [`uuid` crate documentation](https://docs.rs/uuid/latest/uuid/).
- Figma's [ordered sequence engineering account](https://www.figma.com/blog/realtime-editing-of-ordered-sequences/) documents fractional indexing in its concurrent editing context; this is a first-party case study, not a general requirement.

## Acceptance criteria

These choices match the current M0 code and must remain covered by model, import, collision, move, and ordering checks. Schema v1 is not called stable until node property contracts and coordinate conventions are specified.

## Risks and constraints

UUID collisions are unlikely but not logically impossible; duplicate detection remains required. The map choice may need revision if profiling shows poor memory or traversal behavior. Deferring components can require schema expansion when product requirements become clear.

## Change impact

Changing persisted identity or canonical tree representation after schema stabilization requires explicit migrations. Replacing the internal map does not require a file migration if identity and serialization remain unchanged.

## Follow-up

Resolve `CORE-MODEL-01` property contracts from product tasks. Reopen `CORE-COMP-01` only with a documented launch need. Benchmark the store before considering another structure.
