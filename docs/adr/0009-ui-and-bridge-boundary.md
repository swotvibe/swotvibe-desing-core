# ADR-0009: Application service, desktop host, and interface foundation

- **Status:** accepted for the implemented parts; the desktop acceptance gate is partial
- **Date:** 2026-10-06
- **Owner:** project
- **Spec IDs:** CORE-UI-01, CORE-APP-01, CORE-TS-01

## Context

M0 proved the Rust core, format, layout and render path headlessly, and the
repository had no interactive editor and no application layer. Section 6.10 of
the technology outline required a decision about the interface host, the message
contract, data ownership, error transport and resource handling.

Two things were unknown when this record started and are still unknown: which
platform the first release targets, and which customers it serves. The decision
therefore covers an implementation path and a boundary, not a commercial
platform commitment or a final renderer.

## Decision

1. **An application-service crate, not a bridge.** `crates/app`
   (`swotvibe-app`) owns the editor session: it composes `format`, `core`,
   `layout` and `render`, applies commands, runs undo and redo, produces read
   views, and maps every failure to a stable code. It does not depend on Tauri,
   on a windowing library, or on a file system. The actual bridge — the IPC
   layer — is a thin adapter in the host.
2. **Tauri 2 as the desktop host, behind its own workspace.**
   `apps/desktop/src-tauri` owns the window, the native file dialogs, and the
   file system. Its commands validate, call the service, and translate errors;
   they contain no document rules. It declares `[workspace]` so a host whose
   platform libraries a headless CI runner lacks cannot break the repository's
   own test, lint and format jobs.
3. **Vue 3 + Vite as the interface, with TypeScript `6.0.3` pinned exactly.**
   TypeScript 7 ships no programmatic API, and Vue's language tooling still needs
   the 6.x line for `.vue` type-checking. A 7.x upgrade is a gated follow-up, not
   a version number promised in advance.
4. **One injected session, two keys.** `editorKey` carries the service and
   `editorStateKey` carries the session. The shell creates the session once and
   every panel injects it. Two panels each creating their own state is how a
   layer list and a canvas come to disagree about what is selected.
5. **Least privilege in the host.** One capability, no plugin permissions beyond
   `core:default`, an explicit CSP, and no path ever crossing to the WebView. The
   interface asks to open or save; Rust shows the dialog and keeps the path.
6. **Tailwind CSS 4 and `@lucide/vue` for presentation.** Reka UI, Pinia, and a
   Rust-to-TypeScript type generator are deferred until a need is demonstrated
   rather than predicted.
7. **A browser stand-in, clearly labelled.** `SampleEditorBridge` lets the shell
   run and be reviewed without a host. It stores properties, refuses a stale
   revision, and answers a hit test — and nothing else. It is not a second
   implementation of the kernel, and no component imports a service directly.

## Options considered

- **An application service plus a thin host (chosen).** The session is testable
  in Rust without a window, which is what made the M1 acceptance tests possible
  at all, and a second host — CLI, web, or a test harness — reuses the behaviour
  instead of restating it.
- **Tauri commands calling the kernel directly.** Rejected: it mixes transport
  with product and session policy, duplicates command translation, and leaves the
  editing loop testable only through a window.
- **Naming the service crate `bridge`.** Rejected after review: the crate owns a
  session and document lifecycle, so calling it a bridge would describe the wrong
  layer and invite a future host to duplicate the session.
- **Exposing the kernel's types as the wire contract.** Rejected: runtime types
  are neither the file schema nor the interface contract, and their shape must
  not become a public API by accident.
- **TypeScript 7.0 as the current compiler.** Rejected: no programmatic API, and
  the published Vue tooling does not support it. The evidence is in the plan's
  section 2.
- **Reka UI, Pinia, and a generator from day one.** Deferred. Each adds learning
  and upgrade surface, and the current shell needs none of them: the only
  accessibility primitive it requires is a native dialog, the session state is
  owned in one place, and the DTO count is still small enough to write once.
- **Sending rendered pixels on every pointer move.** Rejected as a rule before
  the feature exists: a full-page rasterize per mouse move puts IPC in the
  critical path. Pointer feedback belongs to the interface; a commit produces the
  new preview.
- **A single root workspace including the desktop host.** Rejected: it would make
  `cargo test --workspace` depend on WebKitGTK and similar platform libraries,
  turning a green build into a machine-specific one.

## Boundary

`swotvibe-app` composes `format`, `core`, `layout` and `render`; the kernel
remains unaware of it. The service owns the open document, its undo timeline, and
the typed failures. The Tauri host owns process setup, capabilities, dialogs, and
IPC serialization. Vue owns ephemeral interaction state only: selection, active
tool, zoom, and panel layout.

Messages use explicit `camelCase` DTOs, opaque identity strings, finite
design-unit numbers, stable error codes, and an `expectedRevision` on every
mutation. Document schema versioning is separate from interface DTO versioning.
Large image payloads are returned as bytes, and the host is expected to move to a
binary response rather than a JSON array.

The persisted schema keeps its own `snake_case` names. A file is not a message.

## Evidence

- [Tauri IPC](https://v2.tauri.app/concept/inter-process-communication/) and
  [Calling Rust](https://v2.tauri.app/develop/calling-rust/) document async
  request/response commands, structured errors, and binary `Response` for large
  data.
- [Tauri capabilities](https://v2.tauri.app/security/capabilities/) and
  [CSP](https://v2.tauri.app/security/csp/) document per-window permissions and
  content policies; [the dialog plugin](https://v2.tauri.app/plugin/dialog/)
  documents the native open and save dialogs.
- [TypeScript 7.0 announcement](https://devblogs.microsoft.com/typescript/announcing-typescript-7-0/)
  and [npm release metadata](https://registry.npmjs.org/typescript) establish the
  current stable lines and the missing embedded-language API.
- [Vue Language Tools issue 6124](https://github.com/vuejs/language-tools/issues/6124)
  records published `vue-tsc` failing with `typescript@7.0.2`. It is
  supplementary, not an acceptance test.
- [TypeScript 6.0 release notes](https://www.typescriptlang.org/docs/handbook/release-notes/typescript-6-0.html)
  record `baseUrl` becoming deprecated, which the interface configuration avoids.
- Implemented and tested in this repository: `crates/app` (27 tests across unit,
  integration and documentation), `apps/desktop/src-tauri` (4 tests), and
  `apps/desktop/ui` (17 component tests), with `cargo fmt --all --check`,
  `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace`, `vue-tsc --noEmit`, and `vite build` all passing.

## Acceptance criteria

The parts that are implemented are accepted. The record stays partial until the
following are observed on a real desktop session:

1. In the packaged desktop window: open a file through the native dialog, select
   a node by clicking the canvas, change its fill, undo, redo, save to a file, and
   reopen it — with the reopened document matching the intended state.
2. Undo and redo are exercised against the host's real timeline, not only the
   refusal path the browser stand-in can produce.
3. One native WebDriver end-to-end scenario passes, and the host's own CI entry
   exists on a runner with its platform libraries.
4. A platform is chosen, so "supported" is a statement about a tested platform
   rather than about the machine that happened to build it.

An upgrade to TypeScript 7.x is accepted only when it is published and the whole
of Vue SFC and template type-checking, editor tooling, the Vite build, lint and
the component tests pass without an unofficial shim.

## Risks and constraints

- Tauri depends on the system WebView, so a Windows result is not evidence for
  Linux or macOS.
- The host's dependency tree is large, and its debug binary is roughly 33 MB
  before any optimization. Bundle size needs its own decision before packaging
  claims are made.
- The interface currently draws nodes as DOM elements, so it does not yet
  demonstrate that a rasterized preview of a real document is fast or faithful.
- TypeScript's embedded-language tooling can break on a compiler major even when
  plain `.ts` files still compile.
- The product's launch platform, file extension, reference corpus, and final
  renderer all remain undecided.

## Change impact

The service DTOs isolate both the kernel and the IPC transport. Replacing Tauri,
Vue, or the TypeScript compiler should not change the persisted schema or the
kernel API. Changing the interface DTOs requires explicit compatibility
versioning, updated bindings, and updated contract tests. Moving the host into
the repository workspace would make every Rust job depend on that host's platform
libraries, so it stays a separate workspace until a platform matrix justifies
otherwise.

## Follow-up

Continue with the ordered steps in
[`ui-and-m1-plan.md`](../architecture/ui-and-m1-plan.md#9-الخطوات-التالية-بالترتيب):
the manual desktop run, real undo and redo in the interface, one WebDriver
scenario, and a platform decision. Close this record when its acceptance criteria
are met.
