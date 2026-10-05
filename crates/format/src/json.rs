//! JSON serialization for the persisted schema.
//!
//! JSON is the versioned logical content for schema v1. The ZIP64 file-container
//! profile is decided in ADR-0003 (`CORE-FORMAT-01`), but bundling a document
//! and binary assets is deliberately not implemented in this module; it only
//! converts a [`DtoDocument`] to and from bytes.
//!
//! Unknown top-level fields are preserved through
//! [`DtoDocument::extensions`] rather than dropped, so opening a file that
//! carries data from a sibling tool does not silently discard it. The same is
//! true for unknown fields on a page, node, or asset: those DTOs capture
//! unknown keys through `#[serde(flatten)]`.
//!
//! Reading is bounded by [`ReadLimits`] so a hostile file cannot make the
//! process allocate an unbounded tree; see [`from_json_with_limits`].

use std::collections::BTreeMap;
use std::fmt;

use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};

use crate::dto::DtoDocument;

/// Why a JSON document could not be parsed.
#[derive(Debug)]
#[non_exhaustive]
pub enum JsonError {
    /// The bytes were not valid UTF-8 JSON, or did not match the schema shape.
    Parse(serde_json::Error),
    /// The top-level JSON value was not an object.
    NotAnObject,
    /// A field that must be an object carried a different JSON type.
    WrongFieldType {
        /// The field name, as it appears in the file.
        field: &'static str,
        /// The JSON type actually found.
        found: &'static str,
    },
    /// An unknown top-level field duplicates a key already present in the
    /// `extensions` object, so preserving both values would be ambiguous.
    DuplicateExtensionKey {
        /// The duplicated key.
        key: String,
    },
    /// An entity extension key would serialize over a field owned by this
    /// schema version.
    ReservedExtensionKey {
        /// Entity kind whose field name is reserved.
        record: &'static str,
        /// The colliding key.
        key: String,
    },
    /// The input exceeded the configured resource limits.
    TooLarge {
        /// The limit that was exceeded.
        limit: &'static str,
        /// The measured size.
        found: usize,
        /// The configured maximum.
        allowed: usize,
    },
}

impl std::fmt::Display for JsonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(error) => write!(f, "invalid document JSON: {error}"),
            Self::NotAnObject => f.write_str("document JSON must be an object"),
            Self::WrongFieldType { field, found } => {
                write!(f, "field `{field}` must be an object, found {found}")
            }
            Self::DuplicateExtensionKey { key } => write!(
                f,
                "unknown top-level field `{key}` conflicts with the same key in `extensions`"
            ),
            Self::ReservedExtensionKey { record, key } => write!(
                f,
                "extension key `{key}` is reserved by a modeled {record} field"
            ),
            Self::TooLarge {
                limit,
                found,
                allowed,
            } => write!(f, "{limit} {found} exceeds the limit of {allowed}"),
        }
    }
}

impl std::error::Error for JsonError {}

impl From<serde_json::Error> for JsonError {
    fn from(error: serde_json::Error) -> Self {
        Self::Parse(error)
    }
}

/// The keys the schema models directly. Anything else is preserved verbatim in
/// [`DtoDocument::extensions`].
const KNOWN_KEYS: &[&str] = &[
    "schema_version",
    "id",
    "pages",
    "assets",
    "nodes",
    "extensions",
];

/// Limits applied while reading a document that may come from an untrusted
/// source.
///
/// Parsing is the first place a hostile file is touched, so the byte and array
/// budgets are enforced here, before the structural walk in
/// [`crate::import`] ever builds a runtime document. A JSON parser that
/// rejects an oversized input cannot be made to allocate the whole tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadLimits {
    /// Maximum input size in bytes.
    pub max_bytes: Option<usize>,
    /// Maximum number of nodes accepted.
    pub max_nodes: Option<usize>,
    /// Maximum number of pages accepted.
    pub max_pages: Option<usize>,
}

impl ReadLimits {
    /// No limits. For input this build produced and already trusts.
    pub const UNLIMITED: Self = Self {
        max_bytes: None,
        max_nodes: None,
        max_pages: None,
    };

    /// Defaults suitable for a file arriving from outside the process.
    ///
    /// Chosen to be generous for real designs while still bounding a hostile
    /// file: 64 MiB of JSON, one million nodes, ten thousand pages.
    pub const UNTRUSTED: Self = Self {
        max_bytes: Some(64 * 1024 * 1024),
        max_nodes: Some(1_000_000),
        max_pages: Some(10_000),
    };
}

impl Default for ReadLimits {
    /// Equivalent to [`ReadLimits::UNTRUSTED`].
    fn default() -> Self {
        Self::UNTRUSTED
    }
}

/// Serializes a document to pretty JSON bytes.
///
/// # Errors
///
/// Returns [`JsonError::ReservedExtensionKey`] if an entity extension tries to
/// overwrite a schema-owned field, or [`JsonError::Parse`] if serialization
/// fails.
pub fn to_json(document: &DtoDocument) -> Result<Vec<u8>, JsonError> {
    for page in &document.pages {
        check_extension_keys("page", &page.extensions, &["id", "name", "roots"])?;
    }
    for node in &document.nodes {
        check_extension_keys(
            "node",
            &node.extensions,
            &["id", "kind", "name", "children"],
        )?;
    }
    for asset in &document.assets {
        check_extension_keys("asset", &asset.extensions, &["id", "name"])?;
    }
    Ok(serde_json::to_vec_pretty(document)?)
}

fn check_extension_keys(
    record: &'static str,
    extensions: &BTreeMap<String, Value>,
    reserved: &[&str],
) -> Result<(), JsonError> {
    if let Some(key) = extensions
        .keys()
        .find(|key| reserved.contains(&key.as_str()))
    {
        return Err(JsonError::ReservedExtensionKey {
            record,
            key: key.clone(),
        });
    }
    Ok(())
}

/// Parses JSON bytes into a document, applying [`ReadLimits::UNTRUSTED`].
///
/// # Errors
///
/// Returns [`JsonError::TooLarge`] when an input budget is exceeded,
/// [`JsonError::Parse`] when the bytes are not valid JSON or do not match the
/// schema, [`JsonError::NotAnObject`] when the top level is not an object, and
/// [`JsonError::WrongFieldType`] when `extensions` is present but is not an
/// object. Duplicate object keys at any depth are a parse error.
pub fn from_json(bytes: &[u8]) -> Result<DtoDocument, JsonError> {
    from_json_with_limits(bytes, ReadLimits::UNTRUSTED)
}

/// Parses JSON bytes into a document under caller-supplied limits, collecting
/// unknown top-level keys into `extensions` so a later save round-trips them.
///
/// # Errors
///
/// See [`from_json`].
pub fn from_json_with_limits(bytes: &[u8], limits: ReadLimits) -> Result<DtoDocument, JsonError> {
    if let Some(max) = limits.max_bytes
        && bytes.len() > max
    {
        return Err(JsonError::TooLarge {
            limit: "input size in bytes",
            found: bytes.len(),
            allowed: max,
        });
    }

    let UniqueValue(value) = serde_json::from_slice::<UniqueValue>(bytes)?;
    let Value::Object(mut object) = value else {
        return Err(JsonError::NotAnObject);
    };

    let mut preserved: BTreeMap<String, Value> = BTreeMap::new();

    // Fold anything the schema does not model into `extensions`, including any
    // entries the file already carried there. Core keys keep their own slot.
    // A present-but-wrong-typed `extensions` is an error, not data to delete:
    // silently dropping it is exactly the loss this schema forbids.
    match object.remove("extensions") {
        None => {}
        Some(Value::Object(existing)) => {
            for (key, value) in existing {
                preserved.insert(key, value);
            }
        }
        Some(other) => {
            return Err(JsonError::WrongFieldType {
                field: "extensions",
                found: json_type_name(&other),
            });
        }
    }

    let unknown: Vec<String> = object
        .keys()
        .filter(|key| !KNOWN_KEYS.contains(&key.as_str()))
        .cloned()
        .collect();
    for key in unknown {
        if let Some(value) = object.remove(&key)
            && preserved.insert(key.clone(), value).is_some()
        {
            return Err(JsonError::DuplicateExtensionKey { key });
        }
    }

    // Re-attach the merged map under `extensions` and let Serde validate the
    // modelled fields, so a malformed known field is still an error.
    object.insert(
        "extensions".to_owned(),
        Value::Object(preserved.into_iter().collect()),
    );

    let document: DtoDocument = serde_json::from_value(Value::Object(object))?;
    check_record_counts(&document, limits)?;
    Ok(document)
}

/// A JSON value parser that rejects duplicate keys at every object depth.
/// Parsing into `serde_json::Value` directly would otherwise collapse a map
/// before the format layer had a chance to preserve or diagnose both values.
struct UniqueValue(Value);

impl<'de> Deserialize<'de> for UniqueValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(UniqueValueVisitor)
    }
}

struct UniqueValueVisitor;

impl<'de> Visitor<'de> for UniqueValueVisitor {
    type Value = UniqueValue;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON value with unique object keys")
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(UniqueValue(Value::Null))
    }

    fn visit_none<E>(self) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.visit_unit()
    }

    fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
        Ok(UniqueValue(Value::Bool(value)))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
        Ok(UniqueValue(Value::Number(Number::from(value))))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
        Ok(UniqueValue(Value::Number(Number::from(value))))
    }

    fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Number::from_f64(value)
            .map(Value::Number)
            .map(UniqueValue)
            .ok_or_else(|| E::custom("JSON numbers must be finite"))
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E> {
        Ok(UniqueValue(Value::String(value.to_owned())))
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E> {
        Ok(UniqueValue(Value::String(value)))
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut values = Vec::new();
        while let Some(UniqueValue(value)) = sequence.next_element()? {
            values.push(value);
        }
        Ok(UniqueValue(Value::Array(values)))
    }

    fn visit_map<A>(self, mut object: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut values = Map::new();
        while let Some(key) = object.next_key::<String>()? {
            if values.contains_key(&key) {
                return Err(de::Error::custom(format!(
                    "duplicate JSON object key `{key}`"
                )));
            }
            let UniqueValue(value) = object.next_value()?;
            values.insert(key, value);
        }
        Ok(UniqueValue(Value::Object(values)))
    }
}

fn check_record_counts(document: &DtoDocument, limits: ReadLimits) -> Result<(), JsonError> {
    if let Some(max) = limits.max_nodes
        && document.nodes.len() > max
    {
        return Err(JsonError::TooLarge {
            limit: "node count",
            found: document.nodes.len(),
            allowed: max,
        });
    }
    if let Some(max) = limits.max_pages
        && document.pages.len() > max
    {
        return Err(JsonError::TooLarge {
            limit: "page count",
            found: document.pages.len(),
            allowed: max,
        });
    }
    Ok(())
}

/// The JSON type name of a value, for diagnostics.
fn json_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}
