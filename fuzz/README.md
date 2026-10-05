# Bundle parser fuzz smoke

This separate harness builds mutated inputs from one valid project bundle per
supported schema version and the pinned `zip-rs/zip2` 8.6.0 reader corpus, then
calls the bounded public `unpack_bundle` API. It uses caller limits of 1 MiB
archive, 256 KiB manifest, 64 assets, 256 KiB per asset, and 512 KiB total
payload.

The seeds are `fuzz/corpus/valid-empty-v1.swv` (schema v1, structure only) and
`fuzz/corpus/valid-styled-v2.swv` (schema v2, structure plus node properties,
derived from `tests/fixtures/document-v2-styled.json` by
`swotvibe-tools bundle pack`).

Run locally with:

```text
cargo run --manifest-path fuzz/Cargo.toml --release -- 10000
```

Pull requests run the smoke session on the CI OS matrix. The weekly scheduled
workflow runs 500,000 deterministic mutations. Seeds are unchanged inputs from
`zip-rs/zip2` tag `v8.6.0` and the project's own deterministic empty-bundle
fixture; upstream seed digests and license attribution are checked under
`crates/format/tests`.
