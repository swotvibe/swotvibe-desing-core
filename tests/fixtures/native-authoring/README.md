# Native authoring acceptance case

This fixture freezes the layered GTA VI composition accepted by the project owner on 2026-10-16. It tests execution through the public editor commands, not autonomous design quality.

- `seed.json` contains document metadata, one empty page and the seven asset records. It has no scene nodes.
- `commands.json` contains 109 public `EditCommand` operations, including all 22 creations, text styles and image bindings. This is test input, not a new document format or runtime scripting language.
- `reference-scene.json` is a structural expectation, read only after authoring.
- `reference.png` is the existing editor-rendered visual expectation. The per-channel tolerance is 2; tests cannot regenerate this reference.
- `assets/` contains the exact seven resources embedded in the accepted native document, keyed by asset identity. Five are prepared Rockstar web images and two are original SVG lighting layers. `source-manifest.json` records original source URLs and hashes. The original images remain Rockstar's artwork; no claim of original authorship is made for those pixels.

Run `cargo test -p swotvibe-app --test native_authoring_poster --locked`. Successful execution writes an inspectable native file, preview and verification report to `tests/golden/native-authoring/out/` (ignored build artifacts).

The fixture was derived once from the accepted design. Runtime authoring uses only the commands and empty-page seed. It never imports the reference scene as the output. A separate small Arabic/Latin and non-square image case in `authoring.rs` checks that the API does not depend on GTA identifiers, names or geometry.

The repository MIT license covers its source code, not ownership of the included Rockstar images and logo. Original source attribution is retained in the manifest; the fixture is an unofficial composition used for editor acceptance testing.
