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
//! Schema [`SCHEMA_VERSION`](crate::dto::SCHEMA_VERSION) is the first version,
//! so there are no steps yet. The entry point exists so the migration seam is
//! exercised and grows in one place rather than being rediscovered with v2.

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
    // mistaken for a current one. No steps exist before v2.
    while dto.schema_version < crate::dto::SCHEMA_VERSION {
        let next = step(dto)?;
        dto = next;
    }

    Ok(dto)
}

/// Applies exactly one migration step.
///
/// With only v1 defined, the only reachable case is "already current", which is
/// handled by the caller's loop condition. A version below
/// [`OLDEST_SUPPORTED_VERSION`](crate::convert::OLDEST_SUPPORTED_VERSION) has no
/// defined schema and is reported as a past version, not a future one.
fn step(dto: DtoDocument) -> Result<DtoDocument, ImportError> {
    if dto.schema_version < crate::convert::OLDEST_SUPPORTED_VERSION {
        return Err(ImportError::UnsupportedPastVersion {
            found: dto.schema_version,
            oldest_supported: crate::convert::OLDEST_SUPPORTED_VERSION,
        });
    }
    Err(ImportError::UnsupportedFutureVersion {
        found: dto.schema_version,
        supported: crate::dto::SCHEMA_VERSION,
    })
}

/// Migrates a DTO to the current schema and rebuilds the runtime document.
///
/// # Errors
///
/// As [`migrate_to_current`] and [`convert::import`].
pub fn import_migrated(dto: DtoDocument) -> Result<Document, ImportError> {
    convert::import(&migrate_to_current(dto)?)
}
