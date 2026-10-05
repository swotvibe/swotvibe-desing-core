# Pinned upstream ZIP fuzz seeds

These four negative inputs are copied unchanged from `zip-rs/zip2` tag
`v8.6.0`, directory `fuzz/read/in`. They are parser fuzz seeds, not native
SWOTVIBE bundles. The source repository's MIT license is included as
`LICENSE-upstream.txt`; the upstream project notes that separately licensed
files are identified by their own `LICENSE.*.txt` files.

Source: https://github.com/zip-rs/zip2/tree/v8.6.0/fuzz/read/in

SHA-256 is pinned in `crates/format/tests/upstream_zip_corpus.rs`. Do not edit
these binary files in place. Add a new fixture with its source, applicable
license, and digest instead.
