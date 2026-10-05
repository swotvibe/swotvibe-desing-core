use sha2::{Digest, Sha256};
use std::fs;
#[cfg(windows)]
use std::fs::OpenOptions;
use std::path::Path;
use std::process::Command;

fn limits() -> [&'static str; 10] {
    [
        "--max-archive-bytes",
        "1000000",
        "--max-manifest-bytes",
        "100000",
        "--max-assets",
        "20",
        "--max-asset-bytes",
        "500000",
        "--max-total-asset-bytes",
        "900000",
    ]
}
fn prefix(command: &[&str]) -> Vec<String> {
    let mut args = vec!["bundle".to_owned()];
    args.extend(command.iter().map(|s| (*s).to_owned()));
    args.extend(limits().iter().map(|s| (*s).to_owned()));
    args
}

#[test]
fn pack_verify_unpack_replace_and_restore_backup_work() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("document.json"), r#"{"schema_version":1,"id":"00000000-0000-4000-8000-000000000001","pages":[],"assets":[],"nodes":[]}"#).unwrap();
    let bundle = temp.path().join("design.bundle");
    let args = prefix(&["pack", source.to_str().unwrap(), bundle.to_str().unwrap()]);
    assert!(
        Command::new(env!("CARGO_BIN_EXE_swotvibe"))
            .args(args)
            .status()
            .unwrap()
            .success()
    );
    let packed_bytes = fs::read(&bundle).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&packed_bytes)),
        "b510aa7bf4c0d1c6ccbe334d1e48e5acde7460278384e09bf9f79cd99157f2f7",
        "deterministic bundle bytes changed; review cross-platform output deliberately"
    );
    let verify = prefix(&["verify", bundle.to_str().unwrap()]);
    assert!(
        Command::new(env!("CARGO_BIN_EXE_swotvibe"))
            .args(verify)
            .status()
            .unwrap()
            .success()
    );

    // The strict native codec also interoperates with an independent ZIP reader.
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/validate_bundle.py");
    assert!(
        Command::new("python")
            .arg(script)
            .arg(&bundle)
            .status()
            .unwrap()
            .success()
    );

    let unpacked = temp.path().join("unpacked");
    let args = prefix(&[
        "unpack",
        bundle.to_str().unwrap(),
        unpacked.to_str().unwrap(),
    ]);
    assert!(
        Command::new(env!("CARGO_BIN_EXE_swotvibe"))
            .args(args)
            .status()
            .unwrap()
            .success()
    );
    assert!(unpacked.join("document.json").is_file());

    // Replacement keeps one validated prior version; restore swaps the two.
    let replacement = prefix(&["pack", unpacked.to_str().unwrap(), bundle.to_str().unwrap()]);
    assert!(
        Command::new(env!("CARGO_BIN_EXE_swotvibe"))
            .args(replacement)
            .status()
            .unwrap()
            .success()
    );
    assert!(bundle.with_extension("bundle.bak").is_file());
    #[cfg(windows)]
    let acl_before_restore = acl_sddl(&bundle);
    let restore = prefix(&["restore-backup", bundle.to_str().unwrap()]);
    assert!(
        Command::new(env!("CARGO_BIN_EXE_swotvibe"))
            .args(restore)
            .status()
            .unwrap()
            .success()
    );
    #[cfg(windows)]
    assert_eq!(
        acl_sddl(&bundle),
        acl_before_restore,
        "ReplaceFileW must preserve the target ACL"
    );

    let bad_backup = bundle.with_extension("bundle.bak");
    fs::write(&bad_backup, b"not a bundle").unwrap();
    let prior_target = fs::read(&bundle).unwrap();
    let failed_save = prefix(&["pack", unpacked.to_str().unwrap(), bundle.to_str().unwrap()]);
    assert!(
        !Command::new(env!("CARGO_BIN_EXE_swotvibe"))
            .args(failed_save)
            .status()
            .unwrap()
            .success()
    );
    assert_eq!(fs::read(&bundle).unwrap(), prior_target);
    assert_eq!(fs::read(&bad_backup).unwrap(), b"not a bundle");
}

#[test]
fn asset_bundle_survives_unpack_replace_and_backup_restore() {
    use std::collections::BTreeMap;
    use swotvibe_core::{AssetId, DocumentId};
    use swotvibe_format::{DtoAsset, DtoDocument, to_json};

    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source-with-asset");
    let asset_dir = source.join("assets");
    fs::create_dir_all(&asset_dir).unwrap();

    let id = AssetId::deterministic("bundle-cli", "large-payload");
    let document = DtoDocument {
        schema_version: 1,
        id: DocumentId::deterministic("bundle-cli", "document").to_string(),
        pages: vec![],
        assets: vec![DtoAsset {
            id: id.to_string(),
            name: "synthetic 256 KiB asset".into(),
            extensions: BTreeMap::new(),
        }],
        nodes: vec![],
        extensions: BTreeMap::new(),
    };
    fs::write(source.join("document.json"), to_json(&document).unwrap()).unwrap();
    let original_payload: Vec<u8> = (0..256 * 1024).map(|byte| (byte % 251) as u8).collect();
    let asset_path = asset_dir.join(id.to_string());
    fs::write(&asset_path, &original_payload).unwrap();

    let bundle = temp.path().join("with-asset.swv");
    let pack = prefix(&["pack", source.to_str().unwrap(), bundle.to_str().unwrap()]);
    assert!(
        Command::new(env!("CARGO_BIN_EXE_swotvibe"))
            .args(pack)
            .status()
            .unwrap()
            .success()
    );
    let validator = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/validate_bundle.py");
    assert!(
        Command::new("python")
            .arg(validator)
            .arg(&bundle)
            .status()
            .unwrap()
            .success()
    );

    let unpacked = temp.path().join("unpacked-with-asset");
    let unpack = prefix(&[
        "unpack",
        bundle.to_str().unwrap(),
        unpacked.to_str().unwrap(),
    ]);
    assert!(
        Command::new(env!("CARGO_BIN_EXE_swotvibe"))
            .args(unpack)
            .status()
            .unwrap()
            .success()
    );
    assert_eq!(
        fs::read(unpacked.join("assets").join(id.to_string())).unwrap(),
        original_payload
    );

    let replacement_payload = vec![0xA5; 128 * 1024];
    fs::write(&asset_path, &replacement_payload).unwrap();
    let replace = prefix(&["pack", source.to_str().unwrap(), bundle.to_str().unwrap()]);
    assert!(
        Command::new(env!("CARGO_BIN_EXE_swotvibe"))
            .args(replace)
            .status()
            .unwrap()
            .success()
    );

    let backup = bundle.with_extension("swv.bak");
    assert!(backup.is_file());
    let restore = prefix(&["restore-backup", bundle.to_str().unwrap()]);
    assert!(
        Command::new(env!("CARGO_BIN_EXE_swotvibe"))
            .args(restore)
            .status()
            .unwrap()
            .success()
    );

    let restored = temp.path().join("restored-with-asset");
    let unpack_restored = prefix(&[
        "unpack",
        bundle.to_str().unwrap(),
        restored.to_str().unwrap(),
    ]);
    assert!(
        Command::new(env!("CARGO_BIN_EXE_swotvibe"))
            .args(unpack_restored)
            .status()
            .unwrap()
            .success()
    );
    assert_eq!(
        fs::read(restored.join("assets").join(id.to_string())).unwrap(),
        original_payload
    );
}

#[cfg(windows)]
fn acl_sddl(path: &Path) -> String {
    let result = Command::new("icacls").arg(path).output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    String::from_utf8(result.stdout).unwrap().trim().to_owned()
}

#[cfg(windows)]
#[test]
fn locked_target_and_symlink_destination_are_rejected_without_modification() {
    use std::os::windows::fs::{OpenOptionsExt, symlink_file};
    const FILE_SHARE_READ: u32 = 1;
    const FILE_SHARE_WRITE: u32 = 2;
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("document.json"), r#"{"schema_version":1,"id":"00000000-0000-4000-8000-000000000001","pages":[],"assets":[],"nodes":[]}"#).unwrap();
    let target = temp.path().join("locked.swv");
    let seed = temp.path().join("seed.swv");
    let pack_seed = prefix(&["pack", source.to_str().unwrap(), seed.to_str().unwrap()]);
    assert!(
        Command::new(env!("CARGO_BIN_EXE_swotvibe"))
            .args(pack_seed)
            .status()
            .unwrap()
            .success()
    );
    fs::copy(&seed, &target).unwrap();
    let original = fs::read(&target).unwrap();
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .open(&target)
        .unwrap();
    let args = prefix(&["pack", source.to_str().unwrap(), target.to_str().unwrap()]);
    assert!(
        !Command::new(env!("CARGO_BIN_EXE_swotvibe"))
            .args(args)
            .status()
            .unwrap()
            .success()
    );
    drop(lock);
    assert_eq!(fs::read(&target).unwrap(), original);

    let link = temp.path().join("link.swv");
    if symlink_file(&seed, &link).is_ok() {
        let args = prefix(&["pack", source.to_str().unwrap(), link.to_str().unwrap()]);
        assert!(
            !Command::new(env!("CARGO_BIN_EXE_swotvibe"))
                .args(args)
                .status()
                .unwrap()
                .success()
        );
        assert_eq!(fs::read(&seed).unwrap(), original);
    }
}
