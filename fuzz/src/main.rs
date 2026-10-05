//! Deterministic bounded mutation harness for the strict bundle reader.
//!
//! Seeds are one valid bundle per schema version we still read, so a mutation
//! reaches the property record parser (v2) as well as the bare structure
//! parser (v1).
//!
//! Usage: `cargo run --manifest-path fuzz/Cargo.toml --release -- 10000`.

use swotvibe_format::{BundleLimits, unpack_bundle};

const SEEDS: &[&[u8]] = &[
    include_bytes!("../corpus/valid-empty-v1.swv"),
    include_bytes!("../corpus/valid-styled-v2.swv"),
    include_bytes!("../../crates/format/tests/fixtures/upstream_zip_8_6_0/id-000000,time-0,execs-0,orig-0010aea18cf7489e5fdb14d0073a54df79149347.min"),
    include_bytes!("../../crates/format/tests/fixtures/upstream_zip_8_6_0/id-000001,time-0,execs-0,orig-00723069d9adfb794ee55f7de3dd32cc72fe9256.min"),
    include_bytes!("../../crates/format/tests/fixtures/upstream_zip_8_6_0/id-000002,time-0,execs-0,orig-0102a133e458503e63ead5bf560953b3961f5177.min"),
    include_bytes!("../../crates/format/tests/fixtures/upstream_zip_8_6_0/id-000003,time-0,execs-0,orig-027e778bfd8e498157369ed24006081baddb0e40.min"),
];

fn main() {
    let iterations = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(2_000usize);
    let limits = BundleLimits { max_archive_bytes: 1 << 20, max_manifest_bytes: 1 << 18, max_assets: 64, max_asset_bytes: 1 << 18, max_total_asset_bytes: 1 << 19 };
    let mut state = 0x9e37_79b9_7f4a_7c15u64;
    for i in 0..iterations {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let seed = SEEDS[i % SEEDS.len()];
        let mut input = seed.to_vec();
        if input.is_empty() { input.push(0); }
        let edits = (state as usize % 12) + 1;
        for edit in 0..edits {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            match (state as usize).wrapping_add(edit) % 3 {
                0 if input.len() < (1 << 20) => input.insert(state as usize % (input.len() + 1), state as u8),
                1 if !input.is_empty() => { let at = state as usize % input.len(); input[at] ^= (state >> 32) as u8; }
                _ if !input.is_empty() => { input.remove(state as usize % input.len()); }
                _ => {}
            }
        }
        let _ = unpack_bundle(&input, limits);
    }
    println!("completed {iterations} bounded parser mutations");
}
