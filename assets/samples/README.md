# Sample assets

Sample assets used by deterministic fixtures. `m0-editor-ui/` contains
repository-generated, path-only SVG toolbar icons and a product PNG; their
source generator is `tests/fixtures/generate_m0_editor_ui.py`. The generator
and generated art are authored for this repository and covered by its MIT
license; they contain no third-party artwork.

The earlier technical reference sample (`tests/fixtures/m0-sample-v2.json`)
deliberately has no image payload. Asset decode and paint paths are exercised
separately by the editor UI fixture.

Before adding an asset, record its source and license. Large or local-only
samples are ignored by `.gitignore` (`*.local.*`).
