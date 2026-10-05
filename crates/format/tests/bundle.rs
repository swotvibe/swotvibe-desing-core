use std::collections::BTreeMap;
use std::io::{Cursor, Write};

use swotvibe_core::AssetId;
use swotvibe_format::{Bundle, BundleLimits, DtoDocument, pack_bundle, unpack_bundle};

fn limits() -> BundleLimits {
    BundleLimits {
        max_archive_bytes: 1_000_000,
        max_manifest_bytes: 100_000,
        max_assets: 10,
        max_asset_bytes: 500_000,
        max_total_asset_bytes: 900_000,
    }
}
fn sample() -> Bundle {
    let id = AssetId::deterministic("bundle-test", "asset-1");
    Bundle {
        document: DtoDocument {
            schema_version: 1,
            id: swotvibe_core::DocumentId::deterministic("bundle-test", "doc").to_string(),
            pages: vec![],
            assets: vec![swotvibe_format::DtoAsset {
                id: id.to_string(),
                name: "fixture".into(),
                extensions: BTreeMap::new(),
            }],
            nodes: vec![],
            extensions: BTreeMap::new(),
        },
        assets: BTreeMap::from([(id, b"small binary payload\0\xff".to_vec())]),
    }
}

#[test]
fn pack_is_byte_deterministic_and_round_trips() {
    let bundle = sample();
    let first = pack_bundle(&bundle, limits()).unwrap();
    let second = pack_bundle(&bundle, limits()).unwrap();
    assert_eq!(first, second);
    let mut exact_budget = limits();
    exact_budget.max_archive_bytes = first.len();
    assert_eq!(pack_bundle(&bundle, exact_budget).unwrap(), first);
    exact_budget.max_archive_bytes -= 1;
    assert!(pack_bundle(&bundle, exact_budget).is_err());
    assert_eq!(unpack_bundle(&first, limits()).unwrap(), bundle);
}

#[test]
fn rejects_prefix_suffix_and_local_header_mismatch() {
    let packed = pack_bundle(&sample(), limits()).unwrap();
    let mut prefixed = vec![0];
    prefixed.extend_from_slice(&packed);
    assert!(unpack_bundle(&prefixed, limits()).is_err());
    let mut suffixed = packed.clone();
    suffixed.push(0);
    assert!(unpack_bundle(&suffixed, limits()).is_err());
    let mut mismatch = packed;
    mismatch[30] ^= 1; // First local filename byte.
    assert!(unpack_bundle(&mismatch, limits()).is_err());
}

#[test]
fn rejects_ambiguous_names_gaps_compression_encryption_and_bad_crc() {
    let packed = pack_bundle(&sample(), limits()).unwrap();
    let eocd = packed.len() - 22;
    let central = u32::from_le_bytes(packed[eocd + 16..eocd + 20].try_into().unwrap()) as usize;
    let first_name_len =
        u16::from_le_bytes(packed[central + 28..central + 30].try_into().unwrap()) as usize;
    let first_extra_len =
        u16::from_le_bytes(packed[central + 30..central + 32].try_into().unwrap()) as usize;
    let second_central = central + 46 + first_name_len + first_extra_len;
    let second_local = u32::from_le_bytes(
        packed[second_central + 42..second_central + 46]
            .try_into()
            .unwrap(),
    ) as usize;

    let mut wrong_case = packed.clone();
    wrong_case[30] = b'D';
    wrong_case[central + 46] = b'D';
    assert!(unpack_bundle(&wrong_case, limits()).is_err());

    let mut unknown = packed.clone();
    unknown[30] = b'x';
    unknown[central + 46] = b'x';
    assert!(unpack_bundle(&unknown, limits()).is_err());

    let mut gap = packed.clone();
    gap[second_central + 42..second_central + 46]
        .copy_from_slice(&((second_local + 1) as u32).to_le_bytes());
    assert!(unpack_bundle(&gap, limits()).is_err());

    let mut compressed = packed.clone();
    compressed[8..10].copy_from_slice(&8u16.to_le_bytes());
    compressed[central + 10..central + 12].copy_from_slice(&8u16.to_le_bytes());
    assert!(unpack_bundle(&compressed, limits()).is_err());

    let mut encrypted = packed.clone();
    encrypted[6..8].copy_from_slice(&1u16.to_le_bytes());
    encrypted[central + 8..central + 10].copy_from_slice(&1u16.to_le_bytes());
    assert!(unpack_bundle(&encrypted, limits()).is_err());

    let mut bad_crc = packed;
    let manifest_name_len = u16::from_le_bytes(bad_crc[26..28].try_into().unwrap()) as usize;
    let manifest_extra_len = u16::from_le_bytes(bad_crc[28..30].try_into().unwrap()) as usize;
    bad_crc[30 + manifest_name_len + manifest_extra_len + 5] ^= 1;
    assert!(unpack_bundle(&bad_crc, limits()).is_err());
}

#[test]
fn rejects_duplicate_entry_names() {
    let document = swotvibe_format::to_json(&sample().document).unwrap();
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    writer.start_file("document.json", options).unwrap();
    writer.write_all(&document).unwrap();
    let single = writer.finish().unwrap().into_inner();
    let end_at = single.len() - 22;
    let central_at =
        u32::from_le_bytes(single[end_at + 16..end_at + 20].try_into().unwrap()) as usize;
    let central_len =
        u32::from_le_bytes(single[end_at + 12..end_at + 16].try_into().unwrap()) as usize;
    let local_copy = single[..central_at].to_vec();
    let mut central_copy = single[central_at..central_at + central_len].to_vec();
    let cloned_local_at = central_at as u32;
    central_copy[42..46].copy_from_slice(&cloned_local_at.to_le_bytes());
    let mut duplicate = Vec::new();
    duplicate.extend_from_slice(&single[..central_at]);
    duplicate.extend_from_slice(&local_copy);
    duplicate.extend_from_slice(&single[central_at..central_at + central_len]);
    duplicate.extend_from_slice(&central_copy);
    let mut end = single[end_at..].to_vec();
    end[8..10].copy_from_slice(&2u16.to_le_bytes());
    end[10..12].copy_from_slice(&2u16.to_le_bytes());
    end[12..16].copy_from_slice(&((central_len * 2) as u32).to_le_bytes());
    end[16..20].copy_from_slice(&((central_at * 2) as u32).to_le_bytes());
    duplicate.extend_from_slice(&end);
    assert!(unpack_bundle(&duplicate, limits()).is_err());
}

#[test]
fn limits_are_enforced_before_returning_a_bundle() {
    let bundle = sample();
    let mut budget = limits();
    budget.max_asset_bytes = 2;
    assert!(pack_bundle(&bundle, budget).is_err());
    let mut archive_budget = limits();
    archive_budget.max_archive_bytes = 128;
    assert!(pack_bundle(&bundle, archive_budget).is_err());
}

#[test]
fn parses_zip64_entry_fields_and_zip64_end_records() {
    let bundle = sample();
    let manifest = swotvibe_format::to_json(&bundle.document).unwrap();
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Stored)
        .large_file(true);
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    writer.start_file("document.json", options).unwrap();
    writer.write_all(&manifest).unwrap();
    let (id, payload) = bundle.assets.iter().next().unwrap();
    writer.start_file(format!("assets/{id}"), options).unwrap();
    writer.write_all(payload).unwrap();
    let entry_level_zip64 = writer.finish().unwrap().into_inner();
    assert_eq!(unpack_bundle(&entry_level_zip64, limits()).unwrap(), bundle);

    // Convert the ordinary archive's ending records to ZIP64 without changing
    // entry bytes. This exercises ZIP64 EOCD/locator parsing without a >4 GiB
    // test allocation.
    let ordinary = pack_bundle(&bundle, limits()).unwrap();
    let eocd = ordinary.len() - 22;
    let count = u16::from_le_bytes(ordinary[eocd + 10..eocd + 12].try_into().unwrap()) as u64;
    let cd_size = u32::from_le_bytes(ordinary[eocd + 12..eocd + 16].try_into().unwrap()) as u64;
    let cd_offset = u32::from_le_bytes(ordinary[eocd + 16..eocd + 20].try_into().unwrap()) as u64;
    let mut zip64 = ordinary[..eocd].to_vec();
    let zip64_offset = zip64.len() as u64;
    zip64.extend_from_slice(&0x0606_4b50u32.to_le_bytes());
    zip64.extend_from_slice(&44u64.to_le_bytes());
    zip64.extend_from_slice(&45u16.to_le_bytes());
    zip64.extend_from_slice(&45u16.to_le_bytes());
    zip64.extend_from_slice(&0u32.to_le_bytes());
    zip64.extend_from_slice(&0u32.to_le_bytes());
    zip64.extend_from_slice(&count.to_le_bytes());
    zip64.extend_from_slice(&count.to_le_bytes());
    zip64.extend_from_slice(&cd_size.to_le_bytes());
    zip64.extend_from_slice(&cd_offset.to_le_bytes());
    zip64.extend_from_slice(&0x0706_4b50u32.to_le_bytes());
    zip64.extend_from_slice(&0u32.to_le_bytes());
    zip64.extend_from_slice(&zip64_offset.to_le_bytes());
    zip64.extend_from_slice(&1u32.to_le_bytes());
    let mut end = ordinary[eocd..].to_vec();
    end[8..10].copy_from_slice(&u16::MAX.to_le_bytes());
    end[10..12].copy_from_slice(&u16::MAX.to_le_bytes());
    end[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
    end[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
    zip64.extend_from_slice(&end);
    assert_eq!(unpack_bundle(&zip64, limits()).unwrap(), bundle);
}

#[test]
fn representative_synthetic_profiles_round_trip_at_resource_boundaries() {
    // Product reference files do not exist yet. These synthetic profiles
    // exercise empty, count-heavy, and payload-heavy bundles without claiming
    // to represent customer workloads.
    let profiles: [(&str, &[usize]); 3] = [
        ("empty", &[]),
        ("many-small-assets", &[64; 64]),
        ("few-large-assets", &[192 * 1024; 4]),
    ];
    let profile_limits = BundleLimits {
        max_archive_bytes: 1_100_000,
        max_manifest_bytes: 100_000,
        max_assets: 64,
        max_asset_bytes: 192 * 1024,
        max_total_asset_bytes: 768 * 1024,
    };

    for (profile_name, lengths) in profiles {
        let mut bundle = sample();
        bundle.document.assets.clear();
        bundle.assets.clear();
        for (index, &length) in lengths.iter().enumerate() {
            let id = AssetId::deterministic("bundle-profile", &format!("{profile_name}-{index}"));
            bundle.document.assets.push(swotvibe_format::DtoAsset {
                id: id.to_string(),
                name: format!("{profile_name}-{index}"),
                extensions: BTreeMap::new(),
            });
            bundle
                .assets
                .insert(id, (0..length).map(|byte| (byte % 251) as u8).collect());
        }

        let packed = pack_bundle(&bundle, profile_limits)
            .unwrap_or_else(|error| panic!("profile {profile_name}: {error}"));
        assert_eq!(
            unpack_bundle(&packed, profile_limits).unwrap(),
            bundle,
            "profile {profile_name}"
        );
    }

    let mut count_over_limit = sample();
    count_over_limit.document.assets.clear();
    count_over_limit.assets.clear();
    for index in 0..65 {
        let id = AssetId::deterministic("bundle-profile", &format!("count-{index}"));
        count_over_limit
            .document
            .assets
            .push(swotvibe_format::DtoAsset {
                id: id.to_string(),
                name: format!("count-{index}"),
                extensions: BTreeMap::new(),
            });
        count_over_limit.assets.insert(id, vec![0; 1]);
    }
    assert!(pack_bundle(&count_over_limit, profile_limits).is_err());

    let mut total_over_limit = sample();
    total_over_limit.document.assets.clear();
    total_over_limit.assets.clear();
    for index in 0..5 {
        let id = AssetId::deterministic("bundle-profile", &format!("total-{index}"));
        total_over_limit
            .document
            .assets
            .push(swotvibe_format::DtoAsset {
                id: id.to_string(),
                name: format!("total-{index}"),
                extensions: BTreeMap::new(),
            });
        total_over_limit.assets.insert(id, vec![0; 192 * 1024]);
    }
    assert!(pack_bundle(&total_over_limit, profile_limits).is_err());
}
