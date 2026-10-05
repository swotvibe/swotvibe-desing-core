//! Versioned data-transfer objects for the persisted schema.
//!
//! DTOs are the only types that cross the storage boundary. They carry an
//! explicit schema version and are converted to/from the runtime model in
//! [`swotvibe_core`]. The runtime model never derives serialization, so a
//! change to a runtime struct cannot silently change the on-disk contract.
//!
//! ## Rules owned here
//!
//! - An older version never opens a newer file by silently ignoring fields;
//!   it returns [`crate::ImportError::UnsupportedFutureVersion`] with the
//!   version number.
//! - Core schema fields that are unknown are never dropped silently; the
//!   namespaced [`DtoDocument::extensions`] field carries deliberately
//!   preserved extra data.
//! - Identities are written as their RFC 9562 hyphenated text form, which is
//!   stable and independent of the runtime newtypes.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::node_props::DtoProps;

/// The schema version this crate reads and writes.
///
/// Independent of the application version and the crate version (§8.2).
///
/// - **v1** — identity, kind, name, and structure only.
/// - **v2** — adds [`DtoNode::props`], the node's geometry, paint, and
///   kind-specific properties. Absent means "the default for the node's kind",
///   which is what lets a v1 file open unchanged and what will let a later
///   additive field open in this build.
pub const SCHEMA_VERSION: u32 = 2;

/// The persisted form of a whole document.
///
/// Field names are part of the on-disk contract; renaming one is a breaking
/// schema change, not a refactor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DtoDocument {
    /// The schema version that governs every field below.
    pub schema_version: u32,
    /// The document identity, as hyphenated UUID text.
    pub id: String,
    /// Ordered pages.
    pub pages: Vec<DtoPage>,
    /// Ordered assets.
    pub assets: Vec<DtoAsset>,
    /// Every node in the document, in unspecified order. Parent and sibling
    /// order live in [`DtoPage::roots`] and [`DtoNode::children`], which are
    /// the canonical source of truth.
    pub nodes: Vec<DtoNode>,
    /// Namespaced carrier for data this version does not understand but must
    /// preserve. A core field is never moved into here silently.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extensions: Extensions,
}

/// A namespaced carrier for fields this version does not model but must
/// preserve, keyed by the JSON field name exactly as it appeared on disk.
///
/// `#[serde(flatten)]` makes Serde capture every key the struct does not name
/// and re-emit it at the same level, so an unknown field on any record
/// round-trips through load and save instead of being dropped silently (§8.2).
pub type Extensions = BTreeMap<String, serde_json::Value>;

/// The persisted form of a page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DtoPage {
    /// The page identity, as hyphenated UUID text.
    pub id: String,
    /// A human-readable name.
    pub name: String,
    /// Ordered scene roots.
    pub roots: Vec<String>,
    /// Unknown fields preserved verbatim from the file.
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// The persisted form of a node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DtoNode {
    /// The node identity, as hyphenated UUID text.
    pub id: String,
    /// The node kind, as its stable string name.
    pub kind: String,
    /// A human-readable name. Absent or empty both mean unnamed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Ordered child identities. Empty for leaf kinds.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<String>,
    /// The node's visual properties.
    ///
    /// Absent means the defaults for [`DtoNode::kind`] (see
    /// [`SCHEMA_VERSION`]): a v1 file, which carries no properties at all, is
    /// therefore read as a document of default-styled nodes rather than being
    /// refused. A present record is validated in full; only absence has a
    /// default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub props: Option<DtoProps>,
    /// Unknown fields preserved verbatim from the file.
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// The persisted form of an asset reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DtoAsset {
    /// The asset identity, as hyphenated UUID text.
    pub id: String,
    /// A human-readable name.
    pub name: String,
    /// Unknown fields preserved verbatim from the file.
    #[serde(flatten)]
    pub extensions: Extensions,
}
