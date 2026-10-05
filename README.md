# Swotvibe Design

A design editor built from the ground up. This repository does not build on or
inherit another editor's codebase.

## Current status

The Rust core kernel, versioned JSON schema, restricted ZIP64 bundle codec, and
headless bundle CLI are implemented. Local Windows validation has passed for
workspace tests, formatting, Clippy, dependency license/advisory checks, Python
`zipfile`, Info-ZIP `unzip`/`zipinfo`, 7-Zip, and 500,000 parser fuzz mutations.

The full M0 vertical slice is **not complete**: layout, text, and render crates
are contracts only, no backend is selected, and product reference files are not
available. ZIP64 public-format readiness also remains open until cross-platform
CI, Linux external-reader/fuzz results, and real product files establish the
remaining evidence. The project has not published default bundle limits or a
stable file extension. See [ADR-0003](./docs/adr/0003-file-container.md) and the
[technical specification](./docs/architecture/core-kernel-technical-specification.md).

## Workspace

| Crate | Responsibility | State |
|---|---|---|
| `swotvibe-core` | Document model, identity, commands, atomic transactions, history, validation, snapshots | Implemented for the current M0 model |
| `swotvibe-format` | Versioned DTOs, migrations, JSON, restricted ZIP64 bundles, asset bytes | Implemented; release evidence remains open |
| `swotvibe-tools` | Headless bundle CLI and file replacement/recovery | Implemented; cross-platform CI remains open |
| `swotvibe-layout` | Product layout adapter | Contract only |
| `swotvibe-text` | Text measurement and shaping adapter | Contract only |
| `swotvibe-render` | Scene extraction and rendering adapter | Contract only |

Adapters and tools depend on `core`; `core` does not depend on them.

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

The GitHub Actions workflow adds Linux external ZIP readers and scheduled fuzz
coverage. Its configured jobs are not evidence of passing remote CI runs.

## License

MIT. See [LICENSE](./LICENSE).
