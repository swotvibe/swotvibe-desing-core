# ADR-0003: ZIP64 document bundle profile

- **Status:** accepted architecture; ZIP64 codec and CLI implemented; release gates remain open
- **Date:** 2026-10-05
- **Owner:** project
- **Spec IDs:** CORE-FORMAT-01

## Context

At the time this decision was made, `swotvibe-format` owned versioned JSON
DTOs and migrations but had no file container or binary asset payloads. A
portable document needs one logical manifest and binary objects referenced by
asset IDs, while keeping the schema independent from the container.

## Decision

The first native document bundle will be a single ZIP64 archive with this
project-owned entry profile:

| Entry | Meaning |
|---|---|
| `document.json` | UTF-8 JSON for the versioned document DTO and its asset metadata |
| `assets/<canonical-lowercase-uuid>` | Optional binary payload for the matching `AssetId` |

The profile is deliberately narrower than general ZIP:

- Write entries using ZIP `Stored` (no compression). ZIP64 records are used as
  required by entry or archive size. This keeps pre-compressed image data from
  being recompressed and avoids decompression-ratio expansion in our own
  profile.
- Do not write directory entries, symbolic links, encrypted entries, archive
  comments, or filesystem metadata with product meaning.
- Reject duplicate names, duplicate asset IDs, non-canonical asset names,
  unknown top-level entries, and entries that are not regular files. Never use
  archive-provided names as filesystem paths and never call extraction APIs.
- Require one `document.json`. Asset payloads are optional at the container
  layer because the current runtime asset record does not promise embedded
  bytes; when present, each payload must be referenced exactly once by the
  manifest. A later schema decision may make payload presence mandatory for a
  particular asset kind.
- Keep the schema version in the JSON DTO. The container does not replace the
  migration chain and does not promise compatibility with arbitrary ZIP tools.
- Keep filesystem access and replacement outside `swotvibe-format`. The host
  writes a complete temporary file, flushes/syncs it as required by its
  durability contract, then replaces the target using a platform-specific
  implementation. ZIP `finish()` success alone is not a crash-durability
  guarantee.

The initial implementation should use the maintained Rust `zip` crate behind
the format boundary, with default features disabled and only the features
required for ordinary ZIP/ZIP64 read and write. Pin the resolved dependency in
`Cargo.lock`, record its exact version and license in the dependency inventory,
and review features on upgrades. Do not expose its types through public APIs.

## Options considered

- **JSON plus sidecar directory:** permits independent payload updates but
  complicates moving, sharing, and replacing a document as one unit.
- **Custom container:** offers control but creates a proprietary parser and
  recovery burden without a demonstrated product need.
- **General compressed ZIP:** uses widely available compression options, but
  expands the accepted parser surface and offers little benefit for the
  expected binary assets. The first profile chooses the smaller `Stored` set.

## Security and resource boundary

The parser must validate entry count and names before consuming payloads and
must enforce caller-configurable limits for archive bytes, manifest bytes,
asset count, each asset's bytes, and total asset bytes. ZIP central-directory
size metadata is an early rejection hint, not the only limit: reads must also
be capped and must fail if actual bytes exceed the declared/configured limit.
CRC checks are integrity checks, not authenticity checks. The native format
does not provide signatures or confidentiality.

The `zip` crate documents ZIP64 and supports stored entries. Its API warns
against using entry names directly as extraction paths. A 2025 high-severity
advisory describes a path traversal issue in extraction routines in affected
versions; this design avoids extraction entirely and still requires a patched,
audited dependency. [zip crate](https://docs.rs/zip/latest/zip/) · [ZipFile
name and metadata API](https://docs.rs/zip/latest/zip/read/struct.ZipFile.html) ·
[GHSA-94vh-gphv-8pm8](https://github.com/advisories/GHSA-94vh-gphv-8pm8)

## Acceptance gates (implementation, not design choice)

Implementation status, 2026-10-05:

- **Implemented:** bounded in-memory read/write, strict project-profile layout
  validation, deterministic output, round-trip and malformed-input tests,
  unknown JSON extension preservation, CLI pack/verify/unpack/restore, and
  temporary-file replacement with a validated backup. Windows replacement uses
  `ReplaceFileW`; this is not a power-loss durability guarantee.
- **Run locally:** `cargo test --workspace`, `cargo clippy --workspace
  --all-targets -- -D warnings`, formatting checks, dependency license/advisory
  checks, Python `zipfile`, Info-ZIP `unzip`/`zipinfo`, 7-Zip interoperability,
  and 500,000 bounded parser fuzz mutations passed on Windows on 2026-10-05.
- **Configured but not yet evidenced remotely:** CI defines Windows, Linux, and
  macOS jobs, Python ZIP validation, Linux `unzip`/`zipinfo`/`7z` checks, and a
  bounded fuzz smoke run plus a scheduled 500,000-mutation session. A workflow
  definition is not a passing CI result; the scheduled fuzz run has not been
  observed on its Linux runner.
- **Still open:** a completed cross-platform CI run, the scheduled longer fuzz
  result and external-reader run from Linux CI, and a corpus based on real
  intended-user files and asset-count/size distributions. No such product
  corpus is currently documented, so synthetic profiles must not be described
  as representative customer workloads.

Keep `CORE-FORMAT-01` open for public-format release readiness until the open
evidence is collected. Do not publish default resource limits, claim general
format stability, or promise crash safety without the corresponding evidence.

## Change impact

Logical DTOs and migrations remain independent of ZIP. Container parsing yields
the DTO and asset payload map; importing the DTO into the runtime model remains
an explicit conversion step. This leaves room for a future container change
without changing runtime model types.

## Follow-up

The codec is in `crates/format/src/bundle.rs`; file operations and CLI commands
are in `crates/tools`. The parser rejects non-contiguous layouts and ZIP
metadata outside the profile before passing bytes to the pinned `zip = 8.6.0`
reader. Deterministic archive output, Python interoperability, pinned upstream
negative seeds, and a bounded mutation harness are covered in the repository.

The repository includes synthetic profiles for zero assets, 64 assets of 64
bytes each, and four assets of 192 KiB each, plus a CLI recovery test with a
256 KiB asset. These exercise finite configured limits and recovery logic, but
they do not establish intended user workloads or validate multi-gigabyte
payloads. The small synthetic ZIP64 fixtures validate ZIP64 record parsing
without such allocations. Keep
`CORE-FORMAT-01` open for public-format release readiness until the remaining
cross-platform, Linux fuzz/external-reader, and product-corpus evidence is
recorded.
