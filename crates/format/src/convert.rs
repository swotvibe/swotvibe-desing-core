//! Conversion between the runtime model and the persisted DTOs.
//!
//! The runtime model and the schema are deliberately different types. This
//! module is the only place that knows both, so the on-disk contract can be
//! versioned and tested independently of the editor's internal structs.
//!
//! ## Import path
//!
//! Import builds a fresh [`Document`] and replays the file's content as
//! commands through [`swotvibe_core::apply_batch`]. That keeps a single
//! validated write surface: a structurally invalid file (a cycle, a dangling
//! child, an unknown kind) is rejected by the same rules that guard a live
//! edit, and a rejected file never yields a partially built document.
//!
//! The DTO's children and root lists are the canonical structure, so import
//! walks them depth-first from each page's roots. A node is therefore always
//! created after its parent and placed directly at its final parent and index,
//! and children are created in declared order, which reproduces sibling order
//! with no corrective pass.

use std::collections::{HashMap, HashSet};
use std::fmt;

use serde_json::Value;
use swotvibe_core::{
    AssetId, Command, Document, DocumentId, NodeId, NodeKind, NodePlacement, NodeProps, PageId,
    Position, apply_batch_at_current,
};

use crate::dto::{DtoAsset, DtoDocument, DtoNode, DtoPage, SCHEMA_VERSION};

/// Why a document could not be read into the runtime model.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ImportError {
    /// The file was written by a newer schema than this build understands.
    ///
    /// Returned instead of ignoring fields, so a newer file is never silently
    /// downgraded (§8.2).
    UnsupportedFutureVersion {
        /// The version found in the file.
        found: u32,
        /// The newest version this build supports.
        supported: u32,
    },
    /// The file names a schema version older than the oldest this build can
    /// migrate from.
    ///
    /// A version below `1` has no defined schema, so it cannot be opened as if
    /// it were current (§8.2). Distinct from [`Self::UnsupportedFutureVersion`]
    /// because the remedy differs: a future file needs a newer build, a past
    /// file needs a converter this build does not have.
    UnsupportedPastVersion {
        /// The version found in the file.
        found: u32,
        /// The oldest version this build can read or migrate from.
        oldest_supported: u32,
    },
    /// The input exceeded a configured resource limit.
    ResourceLimit {
        /// The limit that was exceeded.
        limit: String,
        /// The measured size or count.
        found: usize,
        /// The configured maximum.
        allowed: usize,
    },
    /// A field held text that is not a well-formed identity of the expected
    /// kind.
    InvalidId {
        /// The field or context, for diagnostics.
        context: String,
    },
    /// A node named a kind this schema does not know.
    UnknownNodeKind {
        /// The name found in the file.
        found: String,
    },
    /// A node referenced a parent or sibling that is not in the file.
    DanglingReference {
        /// The node whose reference did not resolve.
        from: String,
        /// The identity that was referenced.
        to: String,
    },
    /// The file referred to the same identity twice.
    DuplicateId {
        /// The repeated identity.
        id: String,
    },
    /// A node is its own ancestor, or is reachable by more than one path.
    Cycle {
        /// The node that closed the cycle.
        node: String,
    },
    /// The content was structurally invalid under the kernel's rules.
    InvalidDocument {
        /// A human-readable description of the first violation.
        message: String,
    },
    /// A node's persisted properties could not become runtime properties.
    InvalidProps {
        /// The node whose record was rejected.
        node: String,
        /// A human-readable description of the violation.
        message: String,
    },
}

impl fmt::Display for ImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedFutureVersion { found, supported } => write!(
                f,
                "file schema version {found} is newer than the supported version {supported}"
            ),
            Self::UnsupportedPastVersion {
                found,
                oldest_supported,
            } => write!(
                f,
                "file schema version {found} is older than the oldest supported version {oldest_supported}"
            ),
            Self::ResourceLimit {
                limit,
                found,
                allowed,
            } => write!(f, "{limit} {found} exceeds the limit of {allowed}"),
            Self::InvalidId { context } => write!(f, "invalid identity in {context}"),
            Self::UnknownNodeKind { found } => write!(f, "unknown node kind `{found}`"),
            Self::DanglingReference { from, to } => {
                write!(f, "`{from}` references `{to}`, which is not in the file")
            }
            Self::DuplicateId { id } => write!(f, "identity `{id}` appears more than once"),
            Self::Cycle { node } => write!(f, "`{node}` is its own ancestor or appears twice"),
            Self::InvalidDocument { message } => write!(f, "invalid document: {message}"),
            Self::InvalidProps { node, message } => {
                write!(f, "invalid properties on `{node}`: {message}")
            }
        }
    }
}

impl std::error::Error for ImportError {}

fn parse_id<T: std::str::FromStr<Err = swotvibe_core::IdError>>(
    text: &str,
    context: &str,
) -> Result<T, ImportError> {
    text.parse::<T>().map_err(|_| ImportError::InvalidId {
        context: format!("{context} (`{text}`)"),
    })
}

/// The oldest schema version this build can read.
///
/// Versions below this have no defined schema; `import` refuses them instead of
/// interpreting them as the current version. v1 files are read through the
/// migration in [`crate::migrate`], which fills in the properties v1 did not
/// carry.
pub const OLDEST_SUPPORTED_VERSION: u32 = 1;

/// The default limits applied while importing an untrusted file.
///
/// Depth is bounded so a file that nests deeper than the validator's own limit
/// is refused before a runtime document is built, rather than overflowing the
/// stack during the structural walk.
const IMPORT_MAX_DEPTH: usize = 512;

/// Projects a runtime document into its persisted form.
///
/// Nodes are emitted in a deterministic structural order: a depth-first walk of
/// each page's roots, in declared order, then any node not reachable from a
/// root (which a valid document never has). The scene's own node store is a
/// `HashMap`, so iterating it directly would make save output depend on the
/// hash seed; walking the canonical root and children lists instead makes the
/// same document always serialize to the same bytes.
#[must_use]
pub fn export(document: &Document) -> DtoDocument {
    let pages: Vec<DtoPage> = document
        .pages()
        .iter()
        .map(|page| DtoPage {
            id: page.id.to_string(),
            name: page.name.clone(),
            roots: page.roots().iter().map(ToString::to_string).collect(),
            extensions: document.page_extensions(page.id).clone(),
        })
        .collect();

    let assets: Vec<DtoAsset> = document
        .assets()
        .iter()
        .map(|asset| DtoAsset {
            id: asset.id.to_string(),
            name: asset.name.clone(),
            extensions: document.asset_extensions(asset.id).clone(),
        })
        .collect();

    let mut nodes: Vec<DtoNode> = Vec::with_capacity(document.node_count());
    let mut emitted: HashSet<NodeId> = HashSet::with_capacity(document.node_count());

    for page in document.pages() {
        for root in page.roots() {
            push_subtree(document, *root, &mut emitted, &mut nodes);
        }
    }

    // Defensive: a node not reachable from any root would otherwise be dropped
    // silently, which is exactly the data loss this schema forbids. Emit the
    // remainder in identity order so the output stays deterministic.
    let mut leftover: Vec<NodeId> = document
        .nodes()
        .map(|node| node.id)
        .filter(|id| !emitted.contains(id))
        .collect();
    leftover.sort_by_key(ToString::to_string);
    for id in leftover {
        push_subtree(document, id, &mut emitted, &mut nodes);
    }

    DtoDocument {
        schema_version: SCHEMA_VERSION,
        id: document.id().to_string(),
        pages,
        assets,
        nodes,
        extensions: document.extensions().clone(),
    }
}

/// Appends a node and its subtree to `out` in depth-first, declared order.
///
/// Iterative on purpose: `export` runs on the kernel's own document, which the
/// validator has already bounded, but a shared helper that recurses on input
/// is one refactor away from an unbounded walk. An explicit work stack keeps
/// the frame count independent of document depth.
fn push_subtree(
    document: &Document,
    root: NodeId,
    emitted: &mut HashSet<NodeId>,
    out: &mut Vec<DtoNode>,
) {
    let mut stack: Vec<NodeId> = vec![root];
    while let Some(id) = stack.pop() {
        if !emitted.insert(id) {
            continue;
        }
        let Some(node) = document.node(id) else {
            continue;
        };
        let children: Vec<NodeId> = node.children().to_vec();
        out.push(DtoNode {
            id: node.id.to_string(),
            kind: node.kind.as_str().to_owned(),
            name: node.name.clone(),
            children: children.iter().map(ToString::to_string).collect(),
            props: Some(crate::node_props::props_to_dto(&node.props)),
            extensions: document.node_extensions(id).clone(),
        });
        // Push in reverse so children are visited in declared order.
        for child in children.into_iter().rev() {
            stack.push(child);
        }
    }
}

/// Rebuilds a runtime document from its persisted form using the default
/// untrusted-input limits.
///
/// # Errors
///
/// Returns [`ImportError::UnsupportedFutureVersion`] for a newer schema,
/// [`ImportError::UnsupportedPastVersion`] for a version below
/// [`OLDEST_SUPPORTED_VERSION`], [`ImportError::InvalidId`] for malformed
/// identities, and [`ImportError::InvalidDocument`] when the content violates
/// an integrity rule the kernel enforces.
pub fn import(dto: &DtoDocument) -> Result<Document, ImportError> {
    import_with_limits(dto, ImportLimits::DEFAULT)
}

/// Limits applied to the structural import walk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImportLimits {
    /// Maximum nesting depth accepted.
    pub max_depth: usize,
    /// Maximum number of nodes accepted.
    pub max_nodes: Option<usize>,
    /// Maximum number of pages accepted.
    pub max_pages: Option<usize>,
}

impl ImportLimits {
    /// Default limits for import, suitable for a file arriving from outside
    /// the process.
    ///
    /// The counts match [`crate::json::ReadLimits::UNTRUSTED`] so the two
    /// layers agree and a file accepted by one is not refused by the other.
    pub const DEFAULT: Self = Self {
        max_depth: IMPORT_MAX_DEPTH,
        max_nodes: Some(1_000_000),
        max_pages: Some(10_000),
    };

    /// An explicit alias for [`ImportLimits::DEFAULT`].
    pub const UNTRUSTED: Self = Self::DEFAULT;
}

impl Default for ImportLimits {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Rebuilds a runtime document from its persisted form under caller-supplied
/// limits.
///
/// The version floor is checked before anything else: a file naming a version
/// this build cannot interpret is refused rather than read as if it were
/// current (§8.2). The structural walk is depth-bounded and iterative, so a
/// deep file returns [`ImportError::ResourceLimit`] instead of exhausting the
/// stack.
///
/// # Errors
///
/// See [`import`], plus [`ImportError::ResourceLimit`] when a limit is
/// exceeded.
pub fn import_with_limits(
    dto: &DtoDocument,
    limits: ImportLimits,
) -> Result<Document, ImportError> {
    if dto.schema_version > SCHEMA_VERSION {
        return Err(ImportError::UnsupportedFutureVersion {
            found: dto.schema_version,
            supported: SCHEMA_VERSION,
        });
    }
    if dto.schema_version < OLDEST_SUPPORTED_VERSION {
        return Err(ImportError::UnsupportedPastVersion {
            found: dto.schema_version,
            oldest_supported: OLDEST_SUPPORTED_VERSION,
        });
    }

    if let Some(max) = limits.max_nodes
        && dto.nodes.len() > max
    {
        return Err(ImportError::ResourceLimit {
            limit: "node count".to_owned(),
            found: dto.nodes.len(),
            allowed: max,
        });
    }
    if let Some(max) = limits.max_pages
        && dto.pages.len() > max
    {
        return Err(ImportError::ResourceLimit {
            limit: "page count".to_owned(),
            found: dto.pages.len(),
            allowed: max,
        });
    }

    let document_id: DocumentId = parse_id(&dto.id, "document.id")?;
    let mut document = Document::with_id(document_id);
    // Extensions are metadata the kernel does not interpret, so they ride
    // beside the document rather than through the command surface, which only
    // models structure the kernel understands.
    *document.extensions_mut() = dto.extensions.clone();

    let index = NodeIndex::build(dto, limits.max_depth)?;

    let mut commands: Vec<Command> = Vec::new();
    for page in &index.pages {
        commands.push(Command::CreatePage {
            id: page.id,
            name: page.name.clone(),
        });
    }
    for asset in &dto.assets {
        let id: AssetId = parse_id(&asset.id, "asset.id")?;
        commands.push(Command::CreateAsset {
            id,
            name: asset.name.clone(),
        });
    }
    commands.extend(index.node_commands()?);

    apply_or_invalid(&mut document, &commands)?;
    index.attach_extensions(&mut document);
    Ok(document)
}

/// A node's fully resolved placement, ready to become a `CreateNode`.
struct PlannedNode {
    id: NodeId,
    kind: NodeKind,
    name: Option<String>,
    /// The node's properties, or `None` when the record carried none and the
    /// kind's defaults apply.
    props: Option<NodeProps>,
    placement: NodePlacement,
}

/// The document's structure resolved from its canonical children/root lists.
struct NodeIndex {
    pages: Vec<DtoPageRef>,
    order: Vec<PlannedNode>,
    /// Extensions attached to a page, node, or asset identity, kept here until
    /// the runtime entities exist to receive them.
    page_extensions: Vec<(PageId, crate::dto::Extensions)>,
    node_extensions: Vec<(NodeId, crate::dto::Extensions)>,
    asset_extensions: Vec<(AssetId, crate::dto::Extensions)>,
}

struct DtoPageRef {
    id: PageId,
    name: String,
}

impl NodeIndex {
    fn build(dto: &DtoDocument, max_depth: usize) -> Result<Self, ImportError> {
        let mut kinds: HashMap<NodeId, NodeKind> = HashMap::new();
        let mut by_id: HashMap<NodeId, &DtoNode> = HashMap::new();
        for node in &dto.nodes {
            let id: NodeId = parse_id(&node.id, "node.id")?;
            let kind = NodeKind::from_str_name(&node.kind).ok_or_else(|| {
                ImportError::UnknownNodeKind {
                    found: node.kind.clone(),
                }
            })?;
            if kinds.insert(id, kind).is_some() {
                return Err(ImportError::DuplicateId {
                    id: node.id.clone(),
                });
            }
            by_id.insert(id, node);
        }

        let mut pages: Vec<DtoPageRef> = Vec::with_capacity(dto.pages.len());
        let mut page_roots: Vec<(PageId, Vec<NodeId>)> = Vec::with_capacity(dto.pages.len());
        let mut page_extensions: Vec<(PageId, crate::dto::Extensions)> =
            Vec::with_capacity(dto.pages.len());
        let mut seen_pages: HashSet<PageId> = HashSet::new();
        for page in &dto.pages {
            let id: PageId = parse_id(&page.id, "page.id")?;
            if !seen_pages.insert(id) {
                return Err(ImportError::DuplicateId {
                    id: page.id.clone(),
                });
            }
            let mut roots = Vec::with_capacity(page.roots.len());
            for root in &page.roots {
                let root_id: NodeId = parse_id(root, "page.roots")?;
                if !kinds.contains_key(&root_id) {
                    return Err(ImportError::DanglingReference {
                        from: page.id.clone(),
                        to: root.clone(),
                    });
                }
                roots.push(root_id);
            }
            pages.push(DtoPageRef {
                id,
                name: page.name.clone(),
            });
            if !page.extensions.is_empty() {
                page_extensions.push((id, page.extensions.clone()));
            }
            page_roots.push((id, roots));
        }

        let mut asset_extensions: Vec<(AssetId, crate::dto::Extensions)> = Vec::new();
        for asset in &dto.assets {
            if !asset.extensions.is_empty() {
                let id: AssetId = parse_id(&asset.id, "asset.id")?;
                asset_extensions.push((id, asset.extensions.clone()));
            }
        }

        let mut order: Vec<PlannedNode> = Vec::with_capacity(dto.nodes.len());
        let mut emitted: HashSet<NodeId> = HashSet::new();

        for (page_id, roots) in &page_roots {
            for (index, root) in roots.iter().enumerate() {
                walk_subtree(
                    *root,
                    NodePlacement::PageRoot {
                        page: *page_id,
                        position: Position::At(index),
                    },
                    &kinds,
                    &by_id,
                    max_depth,
                    &mut emitted,
                    &mut order,
                )?;
            }
        }

        if emitted.len() != by_id.len() {
            // A node that is neither a page root nor a declared child is
            // orphaned; the first such node is reported so the file author can
            // find it.
            let orphan = by_id
                .keys()
                .find(|id| !emitted.contains(id))
                .expect("a shortfall implies an unemitted node");
            return Err(ImportError::DanglingReference {
                from: orphan.to_string(),
                to: "page-or-parent".to_owned(),
            });
        }

        let mut node_extensions: Vec<(NodeId, crate::dto::Extensions)> = Vec::new();
        for node in &order {
            if let Some(dto_node) = by_id.get(&node.id)
                && !dto_node.extensions.is_empty()
            {
                node_extensions.push((node.id, dto_node.extensions.clone()));
            }
        }

        Ok(Self {
            pages,
            order,
            page_extensions,
            node_extensions,
            asset_extensions,
        })
    }

    /// Copies preserved extension maps onto the runtime entities that carry the
    /// matching identity.
    fn attach_extensions(&self, document: &mut Document) {
        for (id, extensions) in &self.page_extensions {
            document.set_page_extensions(*id, extensions.clone());
        }
        for (id, extensions) in &self.node_extensions {
            document.set_node_extensions(*id, extensions.clone());
        }
        for (id, extensions) in &self.asset_extensions {
            document.set_asset_extensions(*id, extensions.clone());
        }
    }

    fn node_commands(&self) -> Result<Vec<Command>, ImportError> {
        let mut commands: Vec<Command> = Vec::with_capacity(self.order.len());
        for planned in &self.order {
            commands.push(Command::CreateNode {
                id: planned.id,
                kind: planned.kind,
                name: planned.name.clone(),
                parent: planned.placement,
            });
            // Creation always installs the kind's defaults; a record that
            // carries explicit properties follows with one replacement. The
            // pair keeps every property write on the same validated path a live
            // edit uses, so an invalid record is rejected by the same rule.
            if let Some(props) = &planned.props
                && *props != NodeProps::default_for(planned.kind)
            {
                commands.push(Command::SetNodeProps {
                    id: planned.id,
                    props: Box::new(props.clone()),
                });
            }
        }
        Ok(commands)
    }
}

/// Resolves a node and its subtree into import order.
///
/// Iterative, and depth-bounded against `max_depth`. The bound is checked as
/// each node is pushed, so a file that nests past the limit is refused with
/// [`ImportError::ResourceLimit`] before the stack grows with the file's depth.
#[allow(clippy::too_many_arguments)]
fn walk_subtree(
    root: NodeId,
    root_placement: NodePlacement,
    kinds: &HashMap<NodeId, NodeKind>,
    by_id: &HashMap<NodeId, &DtoNode>,
    max_depth: usize,
    emitted: &mut HashSet<NodeId>,
    order: &mut Vec<PlannedNode>,
) -> Result<(), ImportError> {
    // (node, placement, depth). Pushed with children reversed so the declared
    // order is preserved, matching the recursive depth-first order.
    let mut stack: Vec<(NodeId, NodePlacement, usize)> = vec![(root, root_placement, 0)];

    while let Some((id, placement, depth)) = stack.pop() {
        if depth > max_depth {
            return Err(ImportError::ResourceLimit {
                limit: "nesting depth".to_owned(),
                found: depth,
                allowed: max_depth,
            });
        }

        if !emitted.insert(id) {
            return Err(ImportError::Cycle {
                node: id.to_string(),
            });
        }

        let dto_node = by_id[&id];
        let kind = kinds[&id];
        if !dto_node.children.is_empty() && !kind.can_own_children() {
            return Err(ImportError::InvalidDocument {
                message: format!(
                    "`{}` of kind `{}` cannot own children",
                    dto_node.id, dto_node.kind
                ),
            });
        }

        order.push(PlannedNode {
            id,
            kind,
            name: dto_node.name.clone(),
            props: match &dto_node.props {
                Some(dto) => Some(crate::node_props::props_from_dto(kind, dto).map_err(
                    |error| ImportError::InvalidProps {
                        node: dto_node.id.clone(),
                        message: error.to_string(),
                    },
                )?),
                None => None,
            },
            placement,
        });

        for (index, child) in dto_node.children.iter().enumerate().rev() {
            let child_id: NodeId = parse_id(child, "node.children")?;
            if !kinds.contains_key(&child_id) {
                return Err(ImportError::DanglingReference {
                    from: dto_node.id.clone(),
                    to: child.clone(),
                });
            }
            stack.push((
                child_id,
                NodePlacement::Child {
                    parent: id,
                    position: Position::At(index),
                },
                depth + 1,
            ));
        }
    }

    Ok(())
}

fn apply_or_invalid(document: &mut Document, commands: &[Command]) -> Result<(), ImportError> {
    if commands.is_empty() {
        return Ok(());
    }
    apply_batch_at_current(document, commands).map_err(|error| ImportError::InvalidDocument {
        message: error.to_string(),
    })?;
    Ok(())
}

/// Reports whether an older-but-supported schema needs migration.
#[must_use]
pub fn needs_migration(dto: &DtoDocument) -> bool {
    dto.schema_version < SCHEMA_VERSION
}

/// Preserves an unknown top-level key by routing it through `extensions`.
///
/// Called by the host's raw-file reader before typed deserialization, so a
/// field this build does not model survives a save round-trip instead of being
/// dropped silently (§8.2).
pub fn preserve_unknown(key: impl Into<String>, value: Value) -> (String, Value) {
    (key.into(), value)
}
