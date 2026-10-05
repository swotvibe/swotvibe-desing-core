//! Bounded codec for the project's deliberately small ZIP64 bundle profile.
//!
//! The input is first checked as a contiguous, single-disk ZIP byte layout.
//! Archive names are never used as paths; only the manifest and canonical
//! `assets/<uuid>` names are accepted.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Read, Write};

use swotvibe_core::AssetId;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipArchive, ZipWriter};

use crate::{DtoDocument, from_json_with_limits, json::ReadLimits, to_json};

/// A document DTO and its optional embedded asset bytes.
#[derive(Debug, Clone, PartialEq)]
pub struct Bundle {
    /// Persisted schema document.
    pub document: DtoDocument,
    /// Payload bytes keyed by the referenced asset identity.
    pub assets: BTreeMap<AssetId, Vec<u8>>,
}

/// Caller-supplied finite resource budgets for reading and writing bundles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BundleLimits {
    /// Maximum complete archive size.
    pub max_archive_bytes: usize,
    /// Maximum JSON manifest size.
    pub max_manifest_bytes: usize,
    /// Maximum number of payload entries.
    pub max_assets: usize,
    /// Maximum bytes in any one asset.
    pub max_asset_bytes: usize,
    /// Maximum bytes across all asset payloads.
    pub max_total_asset_bytes: usize,
}

/// Errors produced while validating or encoding the native bundle profile.
#[derive(Debug)]
#[non_exhaustive]
pub enum BundleError {
    /// A configured limit was exceeded.
    Limit {
        /// Name of the exhausted budget.
        name: &'static str,
        /// Observed size or count.
        found: usize,
        /// Caller-provided maximum.
        max: usize,
    },
    /// The manifest or an identity is malformed.
    Manifest(String),
    /// The archive does not match the project's restricted ZIP profile.
    InvalidArchive(String),
    /// The underlying ZIP reader or writer rejected the data.
    Zip(zip::result::ZipError),
    /// Reading or writing the in-memory archive failed.
    Io(std::io::Error),
}

impl std::fmt::Display for BundleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Limit { name, found, max } => write!(f, "{name} {found} exceeds limit {max}"),
            Self::Manifest(message) => write!(f, "invalid bundle manifest: {message}"),
            Self::InvalidArchive(message) => write!(f, "invalid bundle archive: {message}"),
            Self::Zip(error) => write!(f, "ZIP error: {error}"),
            Self::Io(error) => write!(f, "bundle I/O error: {error}"),
        }
    }
}
impl std::error::Error for BundleError {}
impl From<zip::result::ZipError> for BundleError {
    fn from(value: zip::result::ZipError) -> Self {
        Self::Zip(value)
    }
}
impl From<std::io::Error> for BundleError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

/// Encodes one deterministic project-profile bundle.
pub fn pack(bundle: &Bundle, limits: BundleLimits) -> Result<Vec<u8>, BundleError> {
    validate_bundle(bundle, limits)?;
    let manifest = to_json(&bundle.document).map_err(|e| BundleError::Manifest(e.to_string()))?;
    check_limit(
        "estimated archive bytes",
        estimated_archive_size(bundle, manifest.len())?,
        limits.max_archive_bytes,
    )?;
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .last_modified_time(
            DateTime::from_date_and_time(2000, 1, 1, 0, 0, 0).expect("fixed valid timestamp"),
        )
        .system(zip::System::Unix)
        .unix_permissions(0o100644);
    writer
        .start_file(
            "document.json",
            options.large_file(manifest.len() as u64 >= zip::ZIP64_BYTES_THR),
        )
        .map_err(BundleError::Zip)?;
    writer.write_all(&manifest)?;
    for (id, payload) in &bundle.assets {
        let name = format!("assets/{id}");
        writer
            .start_file(
                name,
                options.large_file(payload.len() as u64 >= zip::ZIP64_BYTES_THR),
            )
            .map_err(BundleError::Zip)?;
        writer.write_all(payload)?;
    }
    let bytes = writer.finish()?.into_inner();
    check_limit("archive bytes", bytes.len(), limits.max_archive_bytes)?;
    Ok(bytes)
}

/// Validates and decodes a project-profile bundle under caller budgets.
pub fn unpack(bytes: &[u8], limits: BundleLimits) -> Result<Bundle, BundleError> {
    check_limit("archive bytes", bytes.len(), limits.max_archive_bytes)?;
    validate_raw_layout(bytes, limits.max_assets.saturating_add(1))?;
    let mut archive = ZipArchive::new(Cursor::new(bytes))?;
    if archive.is_empty() {
        return Err(BundleError::InvalidArchive("missing document.json".into()));
    }
    let mut names = BTreeSet::new();
    let mut manifest = None;
    let mut assets = BTreeMap::new();
    let mut total = 0usize;
    for index in 0..archive.len() {
        let file = archive.by_index(index)?;
        let name = file.name().to_owned();
        if !names.insert(name.clone()) {
            return Err(BundleError::InvalidArchive(format!(
                "duplicate entry `{name}`"
            )));
        }
        if file.is_dir() || file.enclosed_name().is_none() {
            return Err(BundleError::InvalidArchive(
                "non-regular or unsafe entry".into(),
            ));
        }
        if file.compression() != CompressionMethod::Stored {
            return Err(BundleError::InvalidArchive(
                "only Stored entries are accepted".into(),
            ));
        }
        if file.encrypted() {
            return Err(BundleError::InvalidArchive("encrypted entry".into()));
        }
        if let Some(mode) = file.unix_mode()
            && (mode & 0o170000) != 0
            && (mode & 0o170000) != 0o100000
        {
            return Err(BundleError::InvalidArchive("non-regular Unix entry".into()));
        }
        if name == "document.json" {
            if manifest.is_some() {
                return Err(BundleError::InvalidArchive(
                    "duplicate document.json".into(),
                ));
            }
            let declared = usize::try_from(file.size()).map_err(|_| {
                BundleError::InvalidArchive("manifest does not fit address space".into())
            })?;
            check_limit("manifest bytes", declared, limits.max_manifest_bytes)?;
            let mut data = Vec::with_capacity(declared);
            file.take(limits.max_manifest_bytes.saturating_add(1) as u64)
                .read_to_end(&mut data)?;
            check_limit("manifest bytes", data.len(), limits.max_manifest_bytes)?;
            manifest = Some(data);
        } else if let Some(text_id) = name.strip_prefix("assets/") {
            let id: AssetId = text_id
                .parse()
                .map_err(|e| BundleError::Manifest(format!("invalid asset id in entry: {e}")))?;
            if id.to_string() != text_id {
                return Err(BundleError::InvalidArchive(
                    "asset entry id is not lowercase canonical UUID".into(),
                ));
            }
            check_limit("asset count", assets.len() + 1, limits.max_assets)?;
            let declared = usize::try_from(file.size()).map_err(|_| {
                BundleError::InvalidArchive("asset does not fit address space".into())
            })?;
            check_limit("asset bytes", declared, limits.max_asset_bytes)?;
            let mut data = Vec::with_capacity(declared);
            file.take(limits.max_asset_bytes.saturating_add(1) as u64)
                .read_to_end(&mut data)?;
            check_limit("asset bytes", data.len(), limits.max_asset_bytes)?;
            total = total.checked_add(data.len()).ok_or(BundleError::Limit {
                name: "total asset bytes",
                found: usize::MAX,
                max: limits.max_total_asset_bytes,
            })?;
            check_limit("total asset bytes", total, limits.max_total_asset_bytes)?;
            if assets.insert(id, data).is_some() {
                return Err(BundleError::InvalidArchive(
                    "duplicate asset payload".into(),
                ));
            }
        } else {
            return Err(BundleError::InvalidArchive(format!(
                "unknown entry `{name}`"
            )));
        }
    }
    let raw =
        manifest.ok_or_else(|| BundleError::InvalidArchive("missing document.json".into()))?;
    let doc = from_json_with_limits(
        &raw,
        ReadLimits {
            max_bytes: Some(limits.max_manifest_bytes),
            ..ReadLimits::UNTRUSTED
        },
    )
    .map_err(|e| BundleError::Manifest(e.to_string()))?;
    let mut referenced = BTreeSet::new();
    for asset in &doc.assets {
        let id: AssetId = asset
            .id
            .parse()
            .map_err(|e| BundleError::Manifest(format!("invalid asset id: {e}")))?;
        if id.to_string() != asset.id {
            return Err(BundleError::Manifest(format!(
                "asset id is not lowercase canonical UUID `{}`",
                asset.id
            )));
        }
        if !referenced.insert(id) {
            return Err(BundleError::Manifest(format!(
                "duplicate asset id `{}`",
                asset.id
            )));
        }
    }
    if assets.keys().any(|id| !referenced.contains(id)) {
        return Err(BundleError::InvalidArchive(
            "unreferenced asset payload".into(),
        ));
    }
    Ok(Bundle {
        document: doc,
        assets,
    })
}

fn validate_bundle(bundle: &Bundle, limits: BundleLimits) -> Result<(), BundleError> {
    let mut ids = BTreeSet::new();
    for asset in &bundle.document.assets {
        let id: AssetId = asset
            .id
            .parse()
            .map_err(|e| BundleError::Manifest(format!("invalid asset id: {e}")))?;
        if id.to_string() != asset.id || !ids.insert(id) {
            return Err(BundleError::Manifest(format!(
                "non-canonical or duplicate asset id `{}`",
                asset.id
            )));
        }
    }
    if bundle.assets.keys().any(|id| !ids.contains(id)) {
        return Err(BundleError::Manifest(
            "payload refers to an undeclared asset".into(),
        ));
    }
    check_limit("asset count", bundle.assets.len(), limits.max_assets)?;
    let mut total = 0usize;
    for payload in bundle.assets.values() {
        check_limit("asset bytes", payload.len(), limits.max_asset_bytes)?;
        total = total.checked_add(payload.len()).ok_or(BundleError::Limit {
            name: "total asset bytes",
            found: usize::MAX,
            max: limits.max_total_asset_bytes,
        })?;
    }
    check_limit("total asset bytes", total, limits.max_total_asset_bytes)?;
    let manifest = to_json(&bundle.document).map_err(|e| BundleError::Manifest(e.to_string()))?;
    check_limit("manifest bytes", manifest.len(), limits.max_manifest_bytes)
}

fn estimated_archive_size(bundle: &Bundle, manifest_len: usize) -> Result<usize, BundleError> {
    let mut size = manifest_len
        .checked_add(30 + "document.json".len() + 46 + "document.json".len() + 22)
        .ok_or(BundleError::Limit {
            name: "estimated archive bytes",
            found: usize::MAX,
            max: usize::MAX,
        })?;
    let mut has_large_entry = manifest_len as u64 >= zip::ZIP64_BYTES_THR;
    for bytes in bundle.assets.values() {
        let large = bytes.len() as u64 >= zip::ZIP64_BYTES_THR;
        has_large_entry |= large;
        let entry = bytes
            .len()
            .checked_add(30 + 43 + 46 + 43 + if large { 40 } else { 0 })
            .ok_or(BundleError::Limit {
                name: "estimated archive bytes",
                found: usize::MAX,
                max: usize::MAX,
            })?;
        size = size.checked_add(entry).ok_or(BundleError::Limit {
            name: "estimated archive bytes",
            found: usize::MAX,
            max: usize::MAX,
        })?;
    }
    let zip64_end = has_large_entry
        || bundle.assets.len() + 1 > zip::ZIP64_ENTRY_THR
        || size as u64 > zip::ZIP64_BYTES_THR;
    if zip64_end {
        size = size.checked_add(76).ok_or(BundleError::Limit {
            name: "estimated archive bytes",
            found: usize::MAX,
            max: usize::MAX,
        })?;
    }
    Ok(size)
}

fn check_limit(name: &'static str, found: usize, max: usize) -> Result<(), BundleError> {
    if found > max {
        Err(BundleError::Limit { name, found, max })
    } else {
        Ok(())
    }
}

/// Rejects non-contiguous layouts and ambiguous trailing/prefix data before the ZIP library parses it.
fn validate_raw_layout(data: &[u8], max_entries: usize) -> Result<(), BundleError> {
    const EOCD: u32 = 0x06054b50;
    const CEN: u32 = 0x02014b50;
    const LOC: u32 = 0x04034b50;
    const ZIP64_EOCD: u32 = 0x06064b50;
    const ZIP64_LOC: u32 = 0x07064b50;
    let invalid = |s: &str| BundleError::InvalidArchive(s.to_owned());
    if data.len() < 22 {
        return Err(invalid("truncated archive"));
    }
    let eocd_at = data.len() - 22;
    if le32(data, eocd_at)? != EOCD || le16(data, eocd_at + 20)? != 0 {
        return Err(invalid(
            "archive comment, trailing bytes, or missing end record",
        ));
    }
    if le16(data, eocd_at + 4)? != 0 || le16(data, eocd_at + 6)? != 0 {
        return Err(invalid("multi-disk archive is forbidden"));
    }
    let disk_count16 = le16(data, eocd_at + 8)?;
    let count16 = le16(data, eocd_at + 10)?;
    if disk_count16 != count16 && disk_count16 != u16::MAX && count16 != u16::MAX {
        return Err(invalid("EOCD entry counts differ"));
    }
    let cd_size32 = le32(data, eocd_at + 12)?;
    let cd_off32 = le32(data, eocd_at + 16)?;
    let mut count = count16 as u64;
    let mut cd_size = cd_size32 as u64;
    let mut cd_off = cd_off32 as u64;
    let mut cd_end = eocd_at as u64;
    if count16 == u16::MAX || cd_size32 == u32::MAX || cd_off32 == u32::MAX {
        if eocd_at < 20 || le32(data, eocd_at - 20)? != ZIP64_LOC {
            return Err(invalid("missing ZIP64 locator"));
        }
        let loc = eocd_at - 20;
        if le32(data, loc + 4)? != 0 || le32(data, loc + 16)? != 1 {
            return Err(invalid("multi-disk ZIP64 locator"));
        }
        let zoff = usize::try_from(le64(data, loc + 8)?)
            .map_err(|_| invalid("ZIP64 offset exceeds address space"))?;
        if le32(data, zoff)? != ZIP64_EOCD {
            return Err(invalid("bad ZIP64 end record"));
        }
        let zlen = le64(data, zoff + 4)?;
        if zlen != 44 {
            return Err(invalid(
                "ZIP64 extensible data or malformed length is unsupported",
            ));
        }
        if zoff.checked_add(56) != Some(loc) {
            return Err(invalid("gap or overlap before ZIP64 locator"));
        }
        if le32(data, zoff + 16)? != 0 || le32(data, zoff + 20)? != 0 {
            return Err(invalid("multi-disk ZIP64 archive is forbidden"));
        }
        count = le64(data, zoff + 32)?;
        if le64(data, zoff + 24)? != count {
            return Err(invalid("split archive counts differ"));
        }
        if (disk_count16 == u16::MAX) != (count16 == u16::MAX) {
            return Err(invalid("inconsistent ZIP64 entry count sentinels"));
        }
        if disk_count16 != u16::MAX && disk_count16 as u64 != count {
            return Err(invalid("ZIP64 disk entry count contradicts EOCD"));
        }
        cd_size = le64(data, zoff + 40)?;
        cd_off = le64(data, zoff + 48)?;
        cd_end = zoff as u64;
        if count16 != u16::MAX && count16 as u64 != count {
            return Err(invalid("ZIP64 entry count contradicts EOCD"));
        }
        if cd_size32 != u32::MAX && cd_size32 as u64 != cd_size {
            return Err(invalid("ZIP64 central size contradicts EOCD"));
        }
        if cd_off32 != u32::MAX && cd_off32 as u64 != cd_off {
            return Err(invalid("ZIP64 central offset contradicts EOCD"));
        }
    }
    if count == 0 || cd_off.checked_add(cd_size) != Some(cd_end) || cd_end > data.len() as u64 {
        return Err(invalid(
            "central directory bounds do not match archive layout",
        ));
    }
    if count > max_entries as u64 {
        return Err(invalid("entry count exceeds configured asset limit"));
    }
    let mut pos = cd_off as usize;
    let mut locals = Vec::new();
    let mut entry_names = BTreeSet::new();
    for _ in 0..count {
        if le32(data, pos)? != CEN {
            return Err(invalid("malformed central directory entry"));
        }
        if le16(data, pos + 34)? != 0 {
            return Err(invalid("multi-disk entry is forbidden"));
        }
        let flags = le16(data, pos + 8)?;
        let method = le16(data, pos + 10)?;
        if flags != 0 || method != 0 {
            return Err(invalid("flags or compression method outside profile"));
        }
        let name_len = le16(data, pos + 28)? as usize;
        let extra_len = le16(data, pos + 30)? as usize;
        let comment_len = le16(data, pos + 32)? as usize;
        if comment_len != 0 {
            return Err(invalid("entry comments are forbidden"));
        }
        if le16(data, pos + 34)? != 0 {
            return Err(invalid("multi-disk entry is forbidden"));
        }
        let made_by = data[pos + 5];
        let attributes = le32(data, pos + 38)?;
        if made_by == 3 {
            let mode = (attributes >> 16) as u16;
            if mode & 0o170000 != 0o100000 {
                return Err(invalid("Unix entry is not a regular file"));
            }
        } else if attributes & 0x10 != 0 {
            return Err(invalid("directory entry is forbidden"));
        }
        let next = pos
            .checked_add(46 + name_len + extra_len + comment_len)
            .ok_or_else(|| invalid("directory overflow"))?;
        if next > cd_end as usize {
            return Err(invalid("truncated central directory"));
        }
        let name = &data[pos + 46..pos + 46 + name_len];
        if !entry_names.insert(name.to_vec()) {
            return Err(invalid("duplicate ZIP entry name"));
        }
        if name != b"document.json" && !(name.starts_with(b"assets/") && name.len() == 43) {
            return Err(invalid("entry name outside canonical profile"));
        }
        let local = le32(data, pos + 42)? as usize;
        validate_extra_fields(data, pos + 46 + name_len, extra_len)?;
        locals.push((local, pos, name_len, extra_len));
        pos = next;
    }
    if pos as u64 != cd_off + cd_size {
        return Err(invalid("unparsed central directory bytes"));
    }
    locals.sort_by_key(|x| x.0);
    if locals[0].0 != 0 {
        return Err(invalid("prepended bytes are forbidden"));
    }
    let mut end = 0usize;
    for (local, cen, nlen, xlen) in locals {
        if local != end || le32(data, local)? != LOC {
            return Err(invalid("local entries overlap or contain gaps"));
        }
        let local_header_end = local
            .checked_add(30)
            .ok_or_else(|| invalid("local header offset overflow"))?;
        if local_header_end > data.len() {
            return Err(invalid("truncated local header"));
        }
        let lname = le16(data, local + 26)? as usize;
        let lextra = le16(data, local + 28)? as usize;
        let local_name_start = local_header_end;
        let local_extra_start = local_name_start
            .checked_add(lname)
            .ok_or_else(|| invalid("local filename bounds overflow"))?;
        let data_start = local_extra_start
            .checked_add(lextra)
            .ok_or_else(|| invalid("local extra field bounds overflow"))?;
        validate_extra_fields(data, local_extra_start, lextra)?;
        if lname != nlen
            || data.get(local_name_start..local_extra_start) != data.get(cen + 46..cen + 46 + nlen)
        {
            return Err(invalid("local and central names differ"));
        }
        let lflags = le16(data, local + 6)?;
        let lmethod = le16(data, local + 8)?;
        if le16(data, local + 4)? != le16(data, cen + 6)?
            || le16(data, local + 10)? != le16(data, cen + 12)?
            || le16(data, local + 12)? != le16(data, cen + 14)?
            || lflags != le16(data, cen + 8)?
            || lmethod != le16(data, cen + 10)?
        {
            return Err(invalid("local and central headers differ"));
        }
        let crc_local = le32(data, local + 14)?;
        let crc_cen = le32(data, cen + 16)?;
        if crc_local != crc_cen {
            return Err(invalid("local and central CRC differ"));
        }
        let csize = le32(data, cen + 20)? as u64;
        let lsize = le32(data, local + 18)? as u64;
        let usize_c = le32(data, cen + 24)? as u64;
        let usize_l = le32(data, local + 22)? as u64;
        let central_sizes = resolve_zip64_sizes(data, cen + 46 + nlen, xlen, usize_c, csize)?;
        let local_sizes = resolve_zip64_sizes(data, local_extra_start, lextra, usize_l, lsize)?;
        if central_sizes != local_sizes {
            return Err(invalid("local and central sizes differ"));
        }
        let span = central_sizes.1;
        end = data_start
            .checked_add(
                usize::try_from(span).map_err(|_| invalid("entry too large for address space"))?,
            )
            .ok_or_else(|| invalid("entry bounds overflow"))?;
        if end > cen {
            return Err(invalid("entry data overlaps central directory"));
        }
    }
    if end as u64 != cd_off {
        return Err(invalid("gap before central directory"));
    }
    Ok(())
}

fn validate_extra_fields(data: &[u8], mut at: usize, extra_len: usize) -> Result<(), BundleError> {
    let end = at
        .checked_add(extra_len)
        .ok_or_else(|| BundleError::InvalidArchive("extra field overflow".into()))?;
    let mut zip64_seen = false;
    while at < end {
        if at + 4 > end {
            return Err(BundleError::InvalidArchive("truncated extra field".into()));
        }
        let id = le16(data, at)?;
        let len = le16(data, at + 2)? as usize;
        if at + 4 + len > end {
            return Err(BundleError::InvalidArchive("malformed extra field".into()));
        }
        if id != 1 || zip64_seen {
            return Err(BundleError::InvalidArchive(
                "only one ZIP64 extra field is permitted".into(),
            ));
        }
        zip64_seen = true;
        at += 4 + len;
    }
    Ok(())
}

fn resolve_zip64_sizes(
    data: &[u8],
    extra_at: usize,
    extra_len: usize,
    uncompressed32: u64,
    compressed32: u64,
) -> Result<(u64, u64), BundleError> {
    let end = extra_at
        .checked_add(extra_len)
        .ok_or_else(|| BundleError::InvalidArchive("extra field overflow".into()))?;
    let mut cursor = extra_at;
    let mut field = None;
    while cursor < end {
        let id = le16(data, cursor)?;
        let len = le16(data, cursor + 2)? as usize;
        if id == 1 {
            field = Some((cursor + 4, len));
            break;
        }
        cursor += 4 + len;
    }
    let (mut at, mut remaining) = field.unwrap_or((0, 0));
    let mut read_size = |required: bool| -> Result<u64, BundleError> {
        if !required {
            return Ok(0);
        }
        if remaining < 8 {
            return Err(BundleError::InvalidArchive(
                "truncated ZIP64 size field".into(),
            ));
        }
        let value = le64(data, at)?;
        at += 8;
        remaining -= 8;
        Ok(value)
    };
    let uncompressed = if uncompressed32 == u32::MAX as u64 {
        read_size(true)?
    } else {
        uncompressed32
    };
    let compressed = if compressed32 == u32::MAX as u64 {
        read_size(true)?
    } else {
        compressed32
    };
    if (uncompressed32 == u32::MAX as u64 || compressed32 == u32::MAX as u64) && field.is_none() {
        return Err(BundleError::InvalidArchive("missing ZIP64 sizes".into()));
    }
    if remaining != 0 {
        return Err(BundleError::InvalidArchive(
            "unexpected ZIP64 values".into(),
        ));
    }
    Ok((uncompressed, compressed))
}
fn le16(d: &[u8], p: usize) -> Result<u16, BundleError> {
    let end = p
        .checked_add(2)
        .ok_or_else(|| BundleError::InvalidArchive("ZIP offset overflow".into()))?;
    Ok(u16::from_le_bytes(
        d.get(p..end)
            .ok_or_else(|| BundleError::InvalidArchive("truncated ZIP field".into()))?
            .try_into()
            .unwrap(),
    ))
}
fn le32(d: &[u8], p: usize) -> Result<u32, BundleError> {
    let end = p
        .checked_add(4)
        .ok_or_else(|| BundleError::InvalidArchive("ZIP offset overflow".into()))?;
    Ok(u32::from_le_bytes(
        d.get(p..end)
            .ok_or_else(|| BundleError::InvalidArchive("truncated ZIP field".into()))?
            .try_into()
            .unwrap(),
    ))
}
fn le64(d: &[u8], p: usize) -> Result<u64, BundleError> {
    let end = p
        .checked_add(8)
        .ok_or_else(|| BundleError::InvalidArchive("ZIP offset overflow".into()))?;
    Ok(u64::from_le_bytes(
        d.get(p..end)
            .ok_or_else(|| BundleError::InvalidArchive("truncated ZIP field".into()))?
            .try_into()
            .unwrap(),
    ))
}
