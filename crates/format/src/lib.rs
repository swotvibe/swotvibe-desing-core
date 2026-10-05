//! # swotvibe-format
//!
//! Persisted schema DTOs and migrations.
//!
//! This crate owns the boundary between the runtime document model in
//! [`swotvibe_core`] and everything that is written to or read from storage.
//! It holds three separate layers:
//!
//! 1. **Runtime model** — lives in [`swotvibe_core`], not here.
//! 2. **Logical persisted schema** — versioned, explicit, name-and-type fields
//!    ([`dto`]).
//! 3. **File container and asset store** — [`bundle`] implements the bounded
//!    ZIP64 profile accepted by ADR-0003 (`CORE-FORMAT-01`). [`json`] remains
//!    the versioned JSON *content* codec.
//!
//! Each persisted schema has its own `schema_version`, independent of the
//! application version and the crate version. Runtime types never derive
//! serialization as if that were the file-format contract; conversion happens
//! through the DTOs in this crate ([`convert`]).
//!
//! ## Status
//!
//! Schema v1, the restricted ZIP64 codec, binary asset store, migrations, and
//! unknown-field preservation are implemented. Release readiness remains
//! gated on representative-size, cross-platform, and extended fuzz campaigns.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod bundle;
pub mod convert;
pub mod dto;
pub mod json;
pub mod migrate;

pub use bundle::{Bundle, BundleError, BundleLimits, pack as pack_bundle, unpack as unpack_bundle};
pub use convert::{
    ImportError, ImportLimits, OLDEST_SUPPORTED_VERSION, export, import, import_with_limits,
    needs_migration, preserve_unknown,
};
pub use dto::{DtoAsset, DtoDocument, DtoNode, DtoPage, Extensions, SCHEMA_VERSION};
pub use json::{JsonError, ReadLimits, from_json, from_json_with_limits, to_json};
pub use migrate::{import_migrated, migrate_to_current};
