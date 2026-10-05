//! Schema migrations.
//!
//! Every migration is deterministic, never touches the network, and reports any
//! value it could not preserve. Older versions migrate through explicit,
//! ordered steps up to the current runtime.
//!
//! The engine never overwrites the user's file: the host must keep the original
//! before a risky migration, or offer a recoverable copy.
//!
//! ## Status
//!
//! - **v1 → v2** adds node properties. The step is a version bump: v2 reads an
//!   absent `props` record as "the defaults for this node's kind", so a v1 file
//!   needs no data translation and a re-save writes the properties explicitly.
//!   Keeping the rule in one place means a later additive field needs no
//!   migration step either.

use crate::convert::{self, ImportError};
use crate::dto::DtoDocument;
use swotvibe_core::Document;

/// Brings a DTO of any supported older version up to the current schema.
///
/// Returns the DTO unchanged when it is already current. A newer version is
/// rejected with [`ImportError::UnsupportedFutureVersion`] rather than opened
/// optimistically (§8.2).
///
/// # Errors
///
/// As [`convert::import`]: a future version or a document that cannot be
/// reconciled with the current schema.
pub fn migrate_to_current(mut dto: DtoDocument) -> Result<DtoDocument, ImportError> {
    if dto.schema_version > crate::dto::SCHEMA_VERSION {
        return Err(ImportError::UnsupportedFutureVersion {
            found: dto.schema_version,
            supported: crate::dto::SCHEMA_VERSION,
        });
    }
    if dto.schema_version < crate::convert::OLDEST_SUPPORTED_VERSION {
        return Err(ImportError::UnsupportedPastVersion {
            found: dto.schema_version,
            oldest_supported: crate::convert::OLDEST_SUPPORTED_VERSION,
        });
    }

    // Ordered, explicit steps. Each step must be a pure function of the DTO
    // and must bump `schema_version` so a partially migrated file is never
    // mistaken for a current one.
    while dto.schema_version < crate::dto::SCHEMA_VERSION {
        let next = step(dto)?;
        dto = next;
    }

    Ok(dto)
}

/// Applies exactly one migration step.
///
/// A version below
/// [`OLDEST_SUPPORTED_VERSION`](crate::convert::OLDEST_SUPPORTED_VERSION) has no
/// defined schema and is reported as a past version, not a future one. Any
/// other gap is a missing step, which is reported rather than skipped so a file
/// can never be opened as a version it is not.
fn step(mut dto: DtoDocument) -> Result<DtoDocument, ImportError> {
    if dto.schema_version < crate::convert::OLDEST_SUPPORTED_VERSION {
        return Err(ImportError::UnsupportedPastVersion {
            found: dto.schema_version,
            oldest_supported: crate::convert::OLDEST_SUPPORTED_VERSION,
        });
    }
    match dto.schema_version {
        // v1 carried no properties. v2 reads an absent record as the kind's
        // defaults, so the step only records that the file now includes them.
        1 => {
            dto.schema_version = 2;
            Ok(dto)
        }
        version => Err(ImportError::UnsupportedFutureVersion {
            found: version,
            supported: crate::dto::SCHEMA_VERSION,
        }),
    }
}

/// Migrates a DTO to the current schema and rebuilds the runtime document.
///
/// # Errors
///
/// As [`migrate_to_current`] and [`convert::import`].
pub fn import_migrated(dto: DtoDocument) -> Result<Document, ImportError> {
    convert::import(&migrate_to_current(dto)?)
}
