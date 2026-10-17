# Swotvibe Design

A design editor built from the ground up. This repository does not build on or
inherit another editor's codebase.

## Current status

The Rust core kernel, versioned JSON schema (v2), restricted ZIP64 bundle codec,
and headless bundle CLI are implemented, and the **M0 vertical slice** now runs
end to end: a document is opened, validated, laid out by one layout adapter,
rasterized by one renderer, and compared against committed references.

| M0 gate item (§12.2) | State |
|---|---|
| 1. A document with a page, a frame, shapes, and text in known fonts | `tests/fixtures/m0-sample-v2.json` |
| 2. Validation, save, and reopen through a versioned DTO | Covered; v1 → v2 migration included |
| 3. One layout adapter and one renderer behind the documented contracts | Taffy and vello_cpu, both temporary ([ADR-0006](./docs/adr/0006-text-backend-parley.md)–[0008](./docs/adr/0008-render-backend-vello-cpu.md)) |
| 4. PNG plus layout report against pinned references | `tests/golden/m0-sample.*`; Arabic editor UI and asset sample: `tests/golden/m0-editor-ui.*` |
| 5. An edit with undo, redo, and a round trip | Covered |
| 6. A batch that fails at its last command leaves no trace | Covered |

The M0 reference images were regenerated on 2026-10-16 after correcting
variable-font outlines and image fitting. Their current review status is
**pending human review**; the owner's 2026-10-06 approval applies to the earlier
pixels. The accepted layered GTA VI composition is a separate authoring
acceptance case. Layout, text and render backend choices remain provisional
(ADR-0006 through ADR-0008).

Local Windows validation has passed for workspace tests, formatting, Clippy,
dependency license/advisory checks, Python `zipfile`, Info-ZIP
`unzip`/`zipinfo`, 7-Zip, and 500,000 parser fuzz mutations.

Cross-platform CI passed on commit `c224926` with both M0 slices, including the
Arabic editor sample, on the Windows, Linux, and macOS matrix
([workflow results](https://github.com/swotvibe/swotvibe-desing-core/actions/runs/37393874233)).
Linux external ZIP readers and the 500,000-mutation fuzz run passed on `caff203`
([workflow results](https://github.com/swotvibe/swotvibe-desing-core/actions/runs/37270260436)).
ZIP64 public-format readiness remains open until real product files establish
representative asset sizes and counts. No default bundle limits or stable file
extension have been published. See [ADR-0003](./docs/adr/0003-file-container.md)
and the [technical specification](./docs/architecture/core-kernel-technical-specification.md).

The M1 work keeps that matrix green and adds an interface job — Node, type-check,
component tests, and a production build
([workflow results](https://github.com/swotvibe/swotvibe-desing-core/actions/runs/37434712462)).
The UI branch also checks the desktop host, real IPC handler and standalone
build on Windows. Native Linux and macOS host builds are not claimed.

## Workspace

The [2026-10-16 core review](./docs/architecture/core-foundation-review-2026-10-16.md)
fixes compound-batch undo, preserves removed page/node/asset metadata, stages
application commands in order, and adds configurable history retention across
both undo and redo. The existing 256-step default remains; the estimated-byte
ceiling is unconfigured until representative workloads justify a host policy.

| Crate | Responsibility | State |
|---|---|---|
| `swotvibe-core` | Document model, properties, geometry, identity, commands, atomic transactions, history, validation, snapshots | Implemented for the current M0 model |
| `swotvibe-format` | Versioned DTOs (v2), migrations, JSON, restricted ZIP64 bundles, asset bytes | Implemented; release evidence remains open |
| `swotvibe-tools` | Headless bundle CLI and file replacement/recovery | Implemented; cross-platform CI passed; product corpus gate remains open |
| `swotvibe-layout` | Product layout adapter | Implemented on Taffy, temporary |
| `swotvibe-text` | Text measurement and shaping adapter | Implemented on Parley and Skrifa, temporary |
| `swotvibe-render` | Scene extraction and rendering adapter | Implemented on vello_cpu, temporary |
| `swotvibe-app` | Editor session: opens bytes, applies commands, undo/redo, views, layout and preview requests, typed errors | Implemented and tested; no window, no file system, no Tauri |

Every pinned dependency — version, license, MSRV, enabled features, and the
official source it was verified against — is recorded in the
[tooling inventory](./docs/architecture/tooling-inventory.md). The lockfiles are
the source of truth; that document explains what each tool owns and why it was
chosen.

The static Arabic editor sample exercises a 1440×900 screen, mixed Arabic/Latin
text, seven SVG icons, a generated PNG product image, and nested scene/layer
groups. Its fixture, generator, test, and review state are documented in
[`tests/fixtures/README.md`](./tests/fixtures/README.md) and
[`tests/golden/README.md`](./tests/golden/README.md). Its PNG is a technical
reference, not an approved product design.

## Desktop shell and editor interface

`apps/desktop` holds the M1 editing loop:

| Path | What it is | State |
|---|---|---|
| `src-tauri` | Tauri 2 host: window, native file dialogs, and thin IPC commands over `swotvibe-app` | Builds and runs on Windows; acceptance gate partial |
| `ui` | Vue 3 + Vite interface, TypeScript `6.0.3` pinned | Builds; 92 interface tests pass |

The host is its own Cargo workspace on purpose: its platform libraries are not
available on a headless CI runner, and including it in the repository workspace
would make `cargo test --workspace` machine-specific.

The interface never receives a file path, and no component talks to a service
directly — a shell injects one. In a plain browser the same interface runs against
a sample service so it can be reviewed without a host; the shell recognises the
host by the globals Tauri injects.

TypeScript is pinned at `6.0.3` because TypeScript 7 ships no programmatic API
and the published Vue tooling still needs the 6.x line for `.vue` type-checking.
A 7.x upgrade is a gated follow-up, not a promised version.

The plan, the acceptance gates, and what is still unproven are in
[`docs/architecture/ui-and-m1-plan.md`](./docs/architecture/ui-and-m1-plan.md) and
[ADR-0009](./docs/adr/0009-ui-and-bridge-boundary.md). This does not commit the
product to a desktop launch.

Adapters and tools depend on `core`; `core` does not depend on them. The text
and layout crates are the only ones that know their backend's types, and those
types never reach the document or the file schema.

## Native authoring foundation

`main` carries the headless kernel, application service, file format and render
adapters. `UI` adds the Vue/Tauri desktop editor and incorporates `main` updates.
Shared Rust code and acceptance fixtures are kept identical across the branches.

`EditorSession::apply` supports atomic creation, text styling and image binding
against a revision-tagged document. The native authoring acceptance test starts
with an empty page and seven prepared resources, executes 109 public commands,
and reproduces the 22-layer composition with seven editable text layers. It
checks undo/redo, asset preservation and identical rendering after save/reopen.
See the [implementation contract](./docs/architecture/agent-step-1-native-authoring.md).

```sh
cargo test -p swotvibe-app --test native_authoring_poster --locked
```

The test writes a native `.swv`, preview and report under
`tests/golden/native-authoring/out/`. `.swv` is the development host's bundle
extension, not a newly published stable format. Generated delivery files under
`output/` stay local. No model SDK or design-agent runtime is installed.

## Pinned fonts

`assets/fonts` holds the OFL fonts the reference tests measure and draw with
(Inter for Latin; Noto Sans Arabic for Arabic), each with its
provenance, version, and SHA-256 in
[`assets/fonts/README.md`](./assets/fonts/README.md). Reference tests register
those files explicitly and never read a system font, so a golden cannot depend
on the machine that produced it.

## Native bundle CLI

Every command requires finite caller-selected budgets. The numbers below are
illustrative command arguments, not recommended defaults:

```sh
cargo run -p swotvibe-tools -- bundle pack <source-dir> <bundle-path> \
  --max-archive-bytes 268435456 --max-manifest-bytes 67108864 \
  --max-assets 10000 --max-asset-bytes 67108864 \
  --max-total-asset-bytes 201326592

cargo run -p swotvibe-tools -- bundle verify <bundle-path> <same limits>
cargo run -p swotvibe-tools -- bundle unpack <bundle-path> <new-dir> <same limits>
cargo run -p swotvibe-tools -- bundle restore-backup <bundle-path> <same limits>
```

The source/unpacked layout is `document.json` plus optional
`assets/<lowercase-canonical-uuid>` files. Unpack requires a new directory.
Replacing a valid bundle keeps one validated `.bak`; Windows uses `ReplaceFileW`
to preserve the target ACL. This recovery path does not promise survival after
power loss. The profile stores entries without compression and rejects
unsupported ZIP features. CRC detects accidental corruption; it does not
authenticate a bundle or prevent tampering.

## Checks and documentation

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check licenses advisories bans
```

- [Architecture and implementation status](./docs/architecture/development-environment.md)
- [Core technical specification](./docs/architecture/core-kernel-technical-specification.md)
- [Technology adoption outline](./docs/architecture/technology-adoption-and-build-outline.md)
- [Technology research](./docs/architecture/technology-adoption-and-build-research.md)
- [Architecture decisions](./docs/adr/README.md)
- [Product requirements status](./docs/product/README.md)
- [Pinned fonts and their provenance](./assets/fonts/README.md)
- [Tooling inventory: pinned versions, licenses, MSRV](./docs/architecture/tooling-inventory.md)
- [Reference images and their comparison rules](./tests/golden/README.md)

GitHub Actions checks Windows, Linux, and macOS on pushes and pull requests,
with weekly and manual 500,000-mutation fuzz runs. See the linked run above for
the latest passing evidence.

## License

Source code: MIT. See [LICENSE](./LICENSE). Third-party image assets retain
their original attribution; see [native authoring fixture provenance](./tests/fixtures/native-authoring/README.md).
