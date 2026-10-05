//! Headless bundle commands. Limits are explicit on every command so a CLI
//! invocation never silently adopts a resource budget that does not fit the
//! caller's corpus.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use swotvibe_core::AssetId;
use swotvibe_format::{Bundle, BundleLimits, from_json_with_limits, pack_bundle, unpack_bundle};

fn main() {
    if let Err(error) = run(std::env::args_os().skip(1).collect()) {
        eprintln!("error: {error}");
        std::process::exit(2);
    }
}

fn run(args: Vec<std::ffi::OsString>) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() < 2 || args[0] != "bundle" {
        return Err(usage().into());
    }
    let command = args[1].to_string_lossy();
    let mut positional = Vec::new();
    let mut vals = BTreeMap::new();
    let mut i = 2;
    while i < args.len() {
        let text = args[i].to_string_lossy();
        if text.starts_with("--") {
            let key = text.trim_start_matches('-').to_owned();
            if ![
                "max-archive-bytes",
                "max-manifest-bytes",
                "max-assets",
                "max-asset-bytes",
                "max-total-asset-bytes",
            ]
            .contains(&key.as_str())
            {
                return Err(format!("unknown option `--{key}`").into());
            }
            i += 1;
            let value = args
                .get(i)
                .ok_or_else(usage)?
                .to_string_lossy()
                .parse::<usize>()?;
            if vals.insert(key, value).is_some() {
                return Err(usage().into());
            }
        } else {
            positional.push(PathBuf::from(&args[i]));
        }
        i += 1;
    }
    let limits = BundleLimits {
        max_archive_bytes: required(&vals, "max-archive-bytes")?,
        max_manifest_bytes: required(&vals, "max-manifest-bytes")?,
        max_assets: required(&vals, "max-assets")?,
        max_asset_bytes: required(&vals, "max-asset-bytes")?,
        max_total_asset_bytes: required(&vals, "max-total-asset-bytes")?,
    };
    match command.as_ref() {
        "pack" if positional.len() == 2 => {
            let bundle = read_source(&positional[0], limits)?;
            let bytes = pack_bundle(&bundle, limits)?;
            safe_save(&positional[1], &bytes, limits)?;
        }
        "verify" if positional.len() == 1 => {
            let bytes = read_bounded(&positional[0], limits.max_archive_bytes)?;
            unpack_bundle(&bytes, limits)?;
            println!("bundle valid");
        }
        "unpack" if positional.len() == 2 => {
            let bytes = read_bounded(&positional[0], limits.max_archive_bytes)?;
            let bundle = unpack_bundle(&bytes, limits)?;
            write_source(&positional[1], &bundle)?;
        }
        "restore-backup" if positional.len() == 1 => restore_backup(&positional[0], limits)?,
        _ => return Err(usage().into()),
    }
    Ok(())
}

fn usage() -> &'static str {
    "usage: swotvibe bundle {pack <source-dir> <bundle>|verify <bundle>|unpack <bundle> <new-dir>|restore-backup <bundle>} --max-archive-bytes N --max-manifest-bytes N --max-assets N --max-asset-bytes N --max-total-asset-bytes N"
}
fn required(
    values: &BTreeMap<String, usize>,
    key: &str,
) -> Result<usize, Box<dyn std::error::Error>> {
    values
        .get(key)
        .copied()
        .ok_or_else(|| format!("missing --{key}").into())
}

fn read_source(path: &Path, limits: BundleLimits) -> Result<Bundle, Box<dyn std::error::Error>> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err("source must be a real directory".into());
    }
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let name = entry.file_name();
        if name != "document.json" && name != "assets" {
            return Err(format!("unrecognized source entry `{}`", name.to_string_lossy()).into());
        }
    }
    let manifest_path = path.join("document.json");
    ensure_regular(&manifest_path)?;
    let manifest = read_bounded(&manifest_path, limits.max_manifest_bytes)?;
    let document = from_json_with_limits(
        &manifest,
        swotvibe_format::ReadLimits {
            max_bytes: Some(limits.max_manifest_bytes),
            ..swotvibe_format::ReadLimits::UNTRUSTED
        },
    )?;
    let assets_dir = path.join("assets");
    let mut assets = BTreeMap::new();
    let assets_metadata = match fs::symlink_metadata(&assets_dir) {
        Ok(metadata) => Some(metadata),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    if let Some(md) = assets_metadata {
        if !md.is_dir() || md.file_type().is_symlink() {
            return Err("assets must be a real directory".into());
        }
        let mut total_asset_bytes = 0usize;
        for entry in fs::read_dir(&assets_dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let id: AssetId = name
                .parse()
                .map_err(|e| format!("invalid asset filename {name}: {e}"))?;
            if id.to_string() != name {
                return Err(
                    format!("asset filename must be lowercase canonical UUID: {name}").into(),
                );
            }
            if assets.len() >= limits.max_assets {
                return Err(format!("asset count exceeds limit {}", limits.max_assets).into());
            }
            ensure_regular(&entry.path())?;
            let size = usize::try_from(entry.metadata()?.len())
                .map_err(|_| format!("asset `{name}` does not fit address space"))?;
            if size > limits.max_asset_bytes {
                return Err(format!(
                    "asset `{name}` size {size} exceeds limit {}",
                    limits.max_asset_bytes
                )
                .into());
            }
            let remaining_total = limits
                .max_total_asset_bytes
                .saturating_sub(total_asset_bytes);
            let bytes = read_bounded(&entry.path(), limits.max_asset_bytes.min(remaining_total))?;
            total_asset_bytes = total_asset_bytes
                .checked_add(bytes.len())
                .ok_or("total asset byte count overflow")?;
            if assets.insert(id, bytes).is_some() {
                return Err("duplicate asset file".into());
            }
        }
    }
    Ok(Bundle { document, assets })
}

fn write_source(path: &Path, bundle: &Bundle) -> Result<(), Box<dyn std::error::Error>> {
    if path.exists() {
        return Err("unpack destination must not already exist".into());
    }
    fs::create_dir(path)?;
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        fs::write(
            path.join("document.json"),
            swotvibe_format::to_json(&bundle.document)?,
        )?;
        if !bundle.assets.is_empty() {
            fs::create_dir(path.join("assets"))?;
            for (id, bytes) in &bundle.assets {
                fs::write(path.join("assets").join(id.to_string()), bytes)?;
            }
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(path);
    }
    result
}

fn safe_save(
    path: &Path,
    bytes: &[u8],
    limits: BundleLimits,
) -> Result<(), Box<dyn std::error::Error>> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(bytes)?;
    temp.as_file().sync_all()?;
    let check = read_bounded(temp.path(), limits.max_archive_bytes)?;
    unpack_bundle(&check, limits)?;
    let bak = backup_path(path);
    let staged_old_backup = bak.with_extension("bak.rotate-stage");
    let staged_restore = bak.with_extension("bak.restore-stage");
    if !path.exists() {
        if staged_old_backup.exists() || staged_restore.exists() {
            return Err(
                "stale backup staging file exists; inspect or restore it before saving".into(),
            );
        }
        if bak.exists() {
            ensure_regular(&bak)?;
            let old = read_bounded(&bak, limits.max_archive_bytes)?;
            unpack_bundle(&old, limits)
                .map_err(|e| format!("existing backup is invalid; refusing to save: {e}"))?;
        }
        if let Err(e) = temp.persist_noclobber(path) {
            return Err(format!("installing new bundle failed: {e}").into());
        }
        return Ok(());
    }
    ensure_regular(path)?;
    let existing = read_bounded(path, limits.max_archive_bytes)?;
    unpack_bundle(&existing, limits)
        .map_err(|e| format!("existing target is not a valid bundle; refusing replacement: {e}"))?;
    if staged_old_backup.exists() {
        ensure_regular(&staged_old_backup)?;
        let staged = read_bounded(&staged_old_backup, limits.max_archive_bytes)?;
        unpack_bundle(&staged, limits)
            .map_err(|e| format!("staged backup is invalid; refusing replacement: {e}"))?;
        if bak.exists() {
            ensure_regular(&bak)?;
            let current_backup = read_bounded(&bak, limits.max_archive_bytes)?;
            unpack_bundle(&current_backup, limits)
                .map_err(|e| format!("existing backup is invalid; refusing replacement: {e}"))?;
            fs::remove_file(&staged_old_backup)?;
        } else {
            fs::rename(&staged_old_backup, &bak)?;
        }
    }
    if staged_old_backup.exists() {
        return Err("stale backup rotation file exists; inspect it before saving".into());
    }
    if staged_restore.exists() {
        ensure_regular(&staged_restore)?;
        if bak.exists() {
            return Err(
                "both backup and restore staging file exist; inspect them before saving".into(),
            );
        }
        fs::rename(&staged_restore, &bak)?;
    }
    if bak.exists() {
        ensure_regular(&bak)?;
        let old = read_bounded(&bak, limits.max_archive_bytes)?;
        unpack_bundle(&old, limits)
            .map_err(|e| format!("existing backup is invalid; refusing to overwrite it: {e}"))?;
        fs::rename(&bak, &staged_old_backup)?;
    }
    let temp_path = temp.into_temp_path(); // Close the temp handle before Win32 replacement.
    if let Err(error) = replace_existing(&temp_path, path, &bak) {
        if !bak.exists() && staged_old_backup.exists() {
            let _ = fs::rename(&staged_old_backup, &bak);
        }
        return Err(error);
    }
    if staged_old_backup.exists() {
        fs::remove_file(staged_old_backup)?;
    }
    // `persist` is intentionally not used: the platform replacement consumes
    // the temporary path and applies its documented metadata behavior.
    Ok(())
}

fn restore_backup(path: &Path, limits: BundleLimits) -> Result<(), Box<dyn std::error::Error>> {
    ensure_regular(path)?;
    let current = read_bounded(path, limits.max_archive_bytes)?;
    unpack_bundle(&current, limits)
        .map_err(|e| format!("current target is not a valid bundle: {e}"))?;
    let backup = backup_path(path);
    let staged = backup.with_extension("bak.restore-stage");
    let rotated = backup.with_extension("bak.rotate-stage");
    if staged.exists() && rotated.exists() {
        return Err(
            "both restore and rotation staging files exist; inspect them before recovery".into(),
        );
    }
    if !backup.exists() && staged.exists() {
        ensure_regular(&staged)?;
        fs::rename(&staged, &backup)?;
    } else if backup.exists() && staged.exists() {
        ensure_regular(&staged)?;
        let previous = read_bounded(&staged, limits.max_archive_bytes)?;
        unpack_bundle(&previous, limits).map_err(|e| format!("staged backup is invalid: {e}"))?;
        fs::remove_file(&staged)?;
    } else if !backup.exists() && rotated.exists() {
        ensure_regular(&rotated)?;
        fs::rename(&rotated, &backup)?;
    } else if backup.exists() && rotated.exists() {
        ensure_regular(&rotated)?;
        let previous = read_bounded(&rotated, limits.max_archive_bytes)?;
        unpack_bundle(&previous, limits).map_err(|e| format!("staged backup is invalid: {e}"))?;
        fs::remove_file(&rotated)?;
    }
    ensure_regular(&backup)?;
    let data = read_bounded(&backup, limits.max_archive_bytes)?;
    unpack_bundle(&data, limits)?;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(&data)?;
    temp.as_file().sync_all()?;
    let written = read_bounded(temp.path(), limits.max_archive_bytes)?;
    unpack_bundle(&written, limits)?;
    let temp_path = temp.into_temp_path();
    // Stage the saved backup name so ReplaceFileW can put the currently active
    // document back into `.bak`. If the replacement fails, restore the staged
    // backup so the recovery copy is still present.
    if staged.exists() {
        return Err("stale restore staging file exists; inspect it before retrying".into());
    }
    fs::rename(&backup, &staged)?;
    if let Err(error) = replace_existing(&temp_path, path, &backup) {
        let _ = fs::rename(&staged, &backup);
        return Err(error);
    }
    fs::remove_file(staged)?;
    Ok(())
}

fn replace_existing(
    new_path: &Path,
    target: &Path,
    backup: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(windows)]
    {
        windows_replace(new_path, target, backup)?;
    }
    #[cfg(not(windows))]
    {
        let old_permissions = fs::metadata(target)?.permissions();
        fs::set_permissions(new_path, old_permissions)?;
        fs::hard_link(target, backup)?;
        if let Err(e) = fs::rename(new_path, target) {
            let _ = fs::remove_file(backup);
            return Err(e.into());
        }
    }
    Ok(())
}

#[cfg(windows)]
fn windows_replace(
    new_path: &Path,
    target: &Path,
    backup: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::ReplaceFileW;
    let target: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
    let replacement: Vec<u16> = new_path.as_os_str().encode_wide().chain(Some(0)).collect();
    let backup: Vec<u16> = backup.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: each string is NUL-terminated and remains alive for the call;
    // ReplaceFileW is used only after both files were validated and colocated.
    let ok = unsafe {
        ReplaceFileW(
            target.as_ptr(),
            replacement.as_ptr(),
            backup.as_ptr(),
            0,
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    if ok == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}

fn backup_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".bak");
    PathBuf::from(name)
}
fn ensure_regular(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_file() || meta.file_type().is_symlink() {
        return Err(format!("{} must be a regular file, not a symlink", path.display()).into());
    }
    Ok(())
}
fn read_bounded(path: &Path, max: usize) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    ensure_regular(path)?;
    let file = File::open(path)?;
    let mut bytes = Vec::new();
    file.take(max.saturating_add(1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > max {
        return Err(format!("{} bytes exceeds limit {max}", path.display()).into());
    }
    Ok(bytes)
}
