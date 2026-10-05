use sha2::{Digest, Sha256};
use swotvibe_format::{BundleLimits, unpack_bundle};

fn limits() -> BundleLimits {
    BundleLimits {
        max_archive_bytes: 1_000_000,
        max_manifest_bytes: 100_000,
        max_assets: 10,
        max_asset_bytes: 500_000,
        max_total_asset_bytes: 900_000,
    }
}

#[test]
fn pinned_upstream_zip_fuzz_seeds_keep_their_hashes_and_are_rejected_by_our_profile() {
    let fixtures: &[(&str, &[u8], &str)] = &[
        (
            "id-000000",
            include_bytes!(
                "fixtures/upstream_zip_8_6_0/id-000000,time-0,execs-0,orig-0010aea18cf7489e5fdb14d0073a54df79149347.min"
            ),
            "d9ee062ca14cb8094a214cd6a9d69496bfe11b10204f4b4c90c8a06a1aee965b",
        ),
        (
            "id-000001",
            include_bytes!(
                "fixtures/upstream_zip_8_6_0/id-000001,time-0,execs-0,orig-00723069d9adfb794ee55f7de3dd32cc72fe9256.min"
            ),
            "40a90e1a6d970e7a959bc83d7e957565f4efdc3899a55c0be34c76e7e18364f9",
        ),
        (
            "id-000002",
            include_bytes!(
                "fixtures/upstream_zip_8_6_0/id-000002,time-0,execs-0,orig-0102a133e458503e63ead5bf560953b3961f5177.min"
            ),
            "454fcead4125c1ed6f3455cf2a3f87b34b81deb65f611786b54f1045506d1159",
        ),
        (
            "id-000003",
            include_bytes!(
                "fixtures/upstream_zip_8_6_0/id-000003,time-0,execs-0,orig-027e778bfd8e498157369ed24006081baddb0e40.min"
            ),
            "63f0fee30fe687d9cbbb59038a1cc01b84831aeff6c527e08c85ed0ad3c81e8b",
        ),
    ];
    for (name, bytes, expected) in fixtures {
        assert_eq!(
            format!("{:x}", Sha256::digest(bytes)),
            *expected,
            "upstream fixture {name} changed; review the source and update the manifest deliberately"
        );
        assert!(
            unpack_bundle(bytes, limits()).is_err(),
            "upstream ZIP seed {name} unexpectedly matches the native profile"
        );
    }
}
