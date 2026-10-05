//! Document model: pages, scene nodes, styles, and relations.
//!
//! Ownership: the document, its pages, nodes, styles, and relations.
//! Not owned: pixel units or GPU memory.
//!
//! The runtime model here is deliberately separate from the persisted schema
//! DTOs in `swotvibe-format`. Do not derive serialization on runtime types as
//! if that were the file-format contract.
//!
//! ## Canonical parent/order representation
//!
//! The technical specification (§5.2) requires exactly one canonical
//! representation for the parent relation and sibling order, and forbids
//! storing both in a way that could disagree. This module chooses the
//! **ordered children list owned by the parent** and derives the parent index
//! from it:
//!
//! - A node stores its children in an ordered `Vec<NodeId>`.
//! - The parent of a node is never stored on the node; it is derived and kept
//!   in a rebuildable index inside [`Document`].
//!
//! This keeps a single source of truth: the children list. Moving a node is a
//! single transaction that changes parent and order together and preserves the
//! `NodeId` (see §5.2). The derived index is an optimization, not a second
//! authority, and can be rebuilt from the children lists at any time.

use std::collections::{HashMap, HashSet};

use crate::ids::{AssetId, DocumentId, NodeId, PageId};
use crate::props::NodeProps;

/// A revision counter for a document.
///
/// Every committed batch increments it exactly once, regardless of how many
/// commands the batch contains (see §6.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Revision(u64);

impl Revision {
    /// The initial revision of a fresh document.
    pub const INITIAL: Self = Self(0);

    /// Creates a revision from a raw counter value.
    #[must_use]
    pub const fn from_raw(value: u64) -> Self {
        Self(value)
    }

    /// Returns the raw counter value.
    #[must_use]
    pub const fn as_u64(self) -> u64 {
        self.0
    }

    /// Returns the next revision.
    ///
    /// Saturates at [`u64::MAX`]. The transaction layer refuses to commit when
    /// the current revision is already at the ceiling, so a saturated value is
    /// never published as the revision of a new commit.
    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

impl std::fmt::Display for Revision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// The kind of a scene node.
///
/// The first schema carries the primitive set from §5.2. This is a starting
/// list for testing, not a promise that every variant ships in the first
/// release.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodeKind {
    /// A container that establishes a layout context.
    Frame,
    /// A container that groups existing nodes without its own layout.
    Group,
    /// A leaf shape (rectangle, ellipse, path).
    Shape,
    /// A text node.
    Text,
    /// A bitmap image node.
    Image,
}

impl NodeKind {
    /// The stable name used in diagnostics and persisted files.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Frame => "frame",
            Self::Group => "group",
            Self::Shape => "shape",
            Self::Text => "text",
            Self::Image => "image",
        }
    }

    /// Whether a node of this kind may own children.
    #[must_use]
    pub const fn can_have_children(self) -> bool {
        matches!(self, Self::Frame | Self::Group)
    }

    /// Whether a node of this kind may own children.
    ///
    /// This states the ownership contract independently of the current schema
    /// vocabulary, so callers that validate structure do not need to change
    /// when the primitive set is revised (see §5.2).
    #[must_use]
    pub const fn can_own_children(self) -> bool {
        self.can_have_children()
    }

    /// Whether a node of this kind may sit directly at the top level of a page.
    ///
    /// Every primitive in the first schema may be a page root; the predicate
    /// exists so the rule has one declared home rather than being implied by
    /// call sites.
    #[must_use]
    pub const fn can_be_root(self) -> bool {
        true
    }

    /// Parses the stable kind name written by [`NodeKind::as_str`].
    ///
    /// Used by the file-format importer. Unknown names return `None` rather
    /// than a default, so a newer file is reported instead of silently
    /// downgraded.
    #[must_use]
    pub fn from_str_name(name: &str) -> Option<Self> {
        match name {
            "frame" => Some(Self::Frame),
            "group" => Some(Self::Group),
            "shape" => Some(Self::Shape),
            "text" => Some(Self::Text),
            "image" => Some(Self::Image),
            _ => None,
        }
    }

    /// Every kind this schema knows, in declaration order.
    pub const ALL: &'static [Self] = &[
        Self::Frame,
        Self::Group,
        Self::Shape,
        Self::Text,
        Self::Image,
    ];
}

/// A document-scoped asset reference.
///
/// Nodes refer to assets by identity; they never copy the target's identity
/// into the node (see §5.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asset {
    /// The asset identity.
    pub id: AssetId,
    /// A human-readable name, for diagnostics and UI.
    pub name: String,
}

impl Asset {
    /// Creates an asset entry.
    #[must_use]
    pub fn new(id: AssetId, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
        }
    }

    /// The asset identity.
    #[must_use]
    pub const fn id(&self) -> AssetId {
        self.id
    }
}

/// A scene node.
///
/// Nodes carry a permanent identity, a known kind, and properties. The parent
/// relation and sibling order are owned by the parent's children list, not by
/// this struct.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// The permanent identity. It never changes on move or rename.
    pub id: NodeId,
    /// The kind of node.
    pub kind: NodeKind,
    /// An optional human-readable name. Not an identity.
    pub name: Option<String>,
    /// Geometry, paint, and kind-specific properties.
    pub props: NodeProps,
    /// Ordered children. Empty for leaf kinds.
    children: Vec<NodeId>,
}

impl Node {
    /// Creates a node with no children and the default properties of its kind.
    #[must_use]
    pub fn new(id: NodeId, kind: NodeKind, name: Option<String>) -> Self {
        Self {
            id,
            kind,
            name,
            props: NodeProps::default_for(kind),
            children: Vec::new(),
        }
    }

    /// The ordered children of this node.
    #[must_use]
    pub fn children(&self) -> &[NodeId] {
        &self.children
    }

    /// Returns the position of `child` among this node's children.
    #[must_use]
    pub fn index_of(&self, child: NodeId) -> Option<usize> {
        self.children.iter().position(|&c| c == child)
    }

    /// The node's human-readable name, or the empty string when unnamed.
    #[must_use]
    pub fn name(&self) -> &str {
        self.name.as_deref().unwrap_or("")
    }

    /// Sets the node's display name, clearing it when `name` is empty.
    pub fn set_name(&mut self, name: &str) {
        self.name = if name.is_empty() {
            None
        } else {
            Some(name.to_owned())
        };
    }

    /// Whether two nodes carry the same stored value apart from their position.
    ///
    /// This compares the node's own data only. Parent and sibling order are
    /// owned by the parent, so they are deliberately excluded: a pure reparent
    /// or reorder must not be reported as a content change.
    #[must_use]
    pub fn same_content(&self, other: &Self) -> bool {
        self.id == other.id
            && self.kind == other.kind
            && self.name == other.name
            && self.props == other.props
    }

    pub(crate) fn children_mut(&mut self) -> &mut Vec<NodeId> {
        &mut self.children
    }
}

/// A page: an ordered list of scene roots.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    /// The page identity.
    pub id: PageId,
    /// A human-readable name.
    pub name: String,
    /// Ordered root nodes of the page's scene.
    roots: Vec<NodeId>,
}

impl Page {
    /// Creates an empty page.
    #[must_use]
    pub fn new(id: PageId, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            roots: Vec::new(),
        }
    }

    /// The ordered scene roots of this page.
    #[must_use]
    pub fn roots(&self) -> &[NodeId] {
        &self.roots
    }

    /// Returns the position of `root` among this page's roots.
    #[must_use]
    pub fn index_of_root(&self, root: NodeId) -> Option<usize> {
        self.roots.iter().position(|&r| r == root)
    }

    pub(crate) fn roots_mut(&mut self) -> &mut Vec<NodeId> {
        &mut self.roots
    }
}

/// The node container and its derived parent index.
///
/// `nodes` is the sole store of nodes. `parent_of` and `page_of` are derived
/// indexes rebuilt from the canonical children/roots lists; they must never be
/// treated as an independent source of truth.
#[derive(Debug, Clone, Default)]
pub(crate) struct Scene {
    nodes: HashMap<NodeId, Node>,
    parent_of: HashMap<NodeId, NodeId>,
    page_of: HashMap<NodeId, PageId>,
}

impl Scene {
    fn insert_node(&mut self, node: Node) {
        self.nodes.insert(node.id, node);
    }

    fn remove_node(&mut self, id: NodeId) -> Option<Node> {
        self.nodes.remove(&id)
    }

    fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(&id)
    }

    fn node_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.nodes.get_mut(&id)
    }

    fn contains(&self, id: NodeId) -> bool {
        self.nodes.contains_key(&id)
    }

    fn len(&self) -> usize {
        self.nodes.len()
    }

    fn set_parent(&mut self, child: NodeId, parent: NodeId) {
        self.parent_of.insert(child, parent);
    }

    fn clear_parent(&mut self, child: NodeId) {
        self.parent_of.remove(&child);
    }

    fn parent(&self, child: NodeId) -> Option<NodeId> {
        self.parent_of.get(&child).copied()
    }

    fn set_page(&mut self, node: NodeId, page: PageId) {
        self.page_of.insert(node, page);
    }

    fn clear_page(&mut self, node: NodeId) {
        self.page_of.remove(&node);
    }

    fn page(&self, node: NodeId) -> Option<PageId> {
        self.page_of.get(&node).copied()
    }

    /// Whether `candidate` lies inside the subtree rooted at `ancestor`.
    ///
    /// Walks the derived parent index upwards, so it terminates even if the
    /// index is momentarily inconsistent. A node is not its own descendant.
    fn is_descendant_of(&self, candidate: NodeId, ancestor: NodeId) -> bool {
        let mut current = self.parent(candidate);
        while let Some(node) = current {
            if node == ancestor {
                return true;
            }
            current = self.parent(node);
        }
        false
    }

    /// Rebuilds the derived parent index from the canonical children lists.
    /// This is the single authority for recovering the index after any bulk
    /// mutation; the page index is rebuilt by the document, which owns pages.
    #[allow(dead_code)] // reached only through Document::rebuild_derived_indexes
    fn rebuild_indexes(&mut self) {
        self.parent_of.clear();

        let node_ids: Vec<NodeId> = self.nodes.keys().copied().collect();
        for id in node_ids {
            if let Some(children) = self.nodes.get(&id).map(|n| n.children.clone()) {
                for child in children {
                    self.parent_of.insert(child, id);
                }
            }
        }
    }
}

/// A design document: the single source of truth for the design.
#[derive(Debug, Clone)]
pub struct Document {
    id: DocumentId,
    /// Ordered pages.
    pages: Vec<Page>,
    scene: Scene,
    /// Ordered assets.
    assets: Vec<Asset>,
    revision: Revision,
    /// Fields the kernel does not model, preserved so a load-then-save never
    /// discards another tool's data. See [`Document::extensions`].
    extensions: Extensions,
    /// Extensions attached to a page, node, or asset. Kept in side tables keyed
    /// by identity rather than on [`Page`]/[`Node`]/[`Asset`] themselves, so
    /// those types keep their exact value semantics: equality, `same_content`,
    /// and every constructor stay untouched by metadata the kernel ignores.
    page_extensions: HashMap<PageId, Extensions>,
    node_extensions: HashMap<NodeId, Extensions>,
    asset_extensions: HashMap<AssetId, Extensions>,
}

/// A bag of fields the schema does not model, keyed by the JSON field name as
/// it appeared on disk.
///
/// The kernel never reads these values; it only guarantees they survive a load
/// and a save. They are not part of the document's structure, so they are
/// excluded from equality, from change detection, and from the command
/// surface: an edit to a live document cannot mutate them, and nothing the
/// kernel does depends on their content.
pub type Extensions = std::collections::BTreeMap<String, serde_json::Value>;

impl Document {
    /// Creates an empty document with a fresh identity.
    #[must_use]
    pub fn new() -> Self {
        Self::with_id(DocumentId::new())
    }

    /// Creates an empty document with a caller-supplied identity.
    ///
    /// Importers use this to preserve an existing identity; they must resolve
    /// collisions before inserting data (see §5.3).
    #[must_use]
    pub fn with_id(id: DocumentId) -> Self {
        Self {
            id,
            pages: Vec::new(),
            scene: Scene::default(),
            assets: Vec::new(),
            revision: Revision::INITIAL,
            extensions: Extensions::new(),
            page_extensions: HashMap::new(),
            node_extensions: HashMap::new(),
            asset_extensions: HashMap::new(),
        }
    }

    /// Fields the schema does not model, preserved for the caller to own.
    ///
    /// The kernel neither interprets nor validates these values. They exist so
    /// a file that carries data from a sibling tool round-trips unchanged
    /// (§8.2) instead of losing it on the first save.
    #[must_use]
    pub const fn extensions(&self) -> &Extensions {
        &self.extensions
    }

    /// Replaces the document-level preserved fields.
    pub fn extensions_mut(&mut self) -> &mut Extensions {
        &mut self.extensions
    }

    /// Preserved fields attached to a page, or an empty map.
    #[must_use]
    pub fn page_extensions(&self, id: PageId) -> &Extensions {
        self.page_extensions.get(&id).unwrap_or(empty_extensions())
    }

    /// Replaces the preserved fields attached to a page.
    pub fn set_page_extensions(&mut self, id: PageId, extensions: Extensions) {
        if extensions.is_empty() {
            self.page_extensions.remove(&id);
        } else {
            self.page_extensions.insert(id, extensions);
        }
    }

    /// Preserved fields attached to a node, or an empty map.
    #[must_use]
    pub fn node_extensions(&self, id: NodeId) -> &Extensions {
        self.node_extensions.get(&id).unwrap_or(empty_extensions())
    }

    /// Replaces the preserved fields attached to a node.
    pub fn set_node_extensions(&mut self, id: NodeId, extensions: Extensions) {
        if extensions.is_empty() {
            self.node_extensions.remove(&id);
        } else {
            self.node_extensions.insert(id, extensions);
        }
    }

    /// Preserved fields attached to an asset, or an empty map.
    #[must_use]
    pub fn asset_extensions(&self, id: AssetId) -> &Extensions {
        self.asset_extensions.get(&id).unwrap_or(empty_extensions())
    }

    /// Replaces the preserved fields attached to an asset.
    pub fn set_asset_extensions(&mut self, id: AssetId, extensions: Extensions) {
        if extensions.is_empty() {
            self.asset_extensions.remove(&id);
        } else {
            self.asset_extensions.insert(id, extensions);
        }
    }

    /// The document identity.
    #[must_use]
    pub const fn id(&self) -> DocumentId {
        self.id
    }

    /// The current revision.
    #[must_use]
    pub const fn revision(&self) -> Revision {
        self.revision
    }

    /// The ordered pages.
    #[must_use]
    pub fn pages(&self) -> &[Page] {
        &self.pages
    }

    /// The ordered assets.
    #[must_use]
    pub fn assets(&self) -> &[Asset] {
        &self.assets
    }

    /// Looks up an asset by identity.
    #[must_use]
    pub fn asset(&self, id: AssetId) -> Option<&Asset> {
        self.assets.iter().find(|a| a.id == id)
    }

    /// The number of nodes in the document.
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.scene.len()
    }

    /// The scene store with its derived parent and page indexes.
    ///
    /// The scene is derived, read-only state: callers may query structure but
    /// must mutate the document through its command surface so the indexes
    /// cannot drift from the canonical children and root lists.
    #[allow(dead_code)] // reached only from the importer and tests
    pub(crate) fn scene(&self) -> &Scene {
        &self.scene
    }

    /// Sets the document revision.
    ///
    /// Only the batch entry point may call this, and only to advance the
    /// revision by exactly one per committed batch (§6.2).
    pub(crate) fn set_revision(&mut self, revision: Revision) {
        self.revision = revision;
    }

    /// Replaces the child order of `parent`, rejecting any list that is not a
    /// permutation of the current children.
    pub(crate) fn set_child_order(&mut self, parent: NodeId, order: &[NodeId]) -> bool {
        let Some(node) = self.scene.node_mut(parent) else {
            return false;
        };
        if order.len() != node.children.len() {
            return false;
        }
        node.children.clear();
        node.children.extend_from_slice(order);
        true
    }

    /// Replaces the root order of `page`, rejecting any list that is not a
    /// permutation of the current roots.
    pub(crate) fn set_root_order(&mut self, page: PageId, order: &[NodeId]) -> bool {
        let Some(page) = self.pages.iter_mut().find(|p| p.id == page) else {
            return false;
        };
        if order.len() != page.roots.len() {
            return false;
        }
        page.roots.clear();
        page.roots.extend_from_slice(order);
        true
    }

    /// Looks up a page by identity.
    #[must_use]
    pub fn page(&self, id: PageId) -> Option<&Page> {
        self.pages.iter().find(|p| p.id == id)
    }

    /// Looks up a node by identity.
    #[must_use]
    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.scene.node(id)
    }

    /// Returns the derived parent of a node, if any.
    #[must_use]
    pub fn parent_of(&self, id: NodeId) -> Option<NodeId> {
        self.scene.parent(id)
    }

    /// Returns the page a node belongs to, if any.
    #[must_use]
    pub fn page_of(&self, id: NodeId) -> Option<PageId> {
        self.scene.page(id)
    }

    /// Returns whether a node identity exists in the document.
    #[must_use]
    pub fn contains_node(&self, id: NodeId) -> bool {
        self.scene.contains(id)
    }

    /// Whether `candidate` lies inside the subtree rooted at `ancestor`.
    ///
    /// A node is not its own descendant. Used to reject a move that would place
    /// a node inside the subtree it is moving.
    #[must_use]
    pub fn is_descendant_of(&self, candidate: NodeId, ancestor: NodeId) -> bool {
        self.scene.is_descendant_of(candidate, ancestor)
    }

    /// Iterates over all nodes in unspecified order.
    pub fn nodes(&self) -> impl Iterator<Item = &Node> {
        self.scene.nodes.values()
    }

    // --- Mutation surface ---
    //
    // Used by the transaction/history layer and by the module's own tests. A
    // fresh index entry is written on every structural edge change so a
    // committed edit never leaves a stale index; `rebuild_derived_indexes` is
    // only for recovering after an importer or a bulk load.
    //
    // None of these validate. Validation is a separate pass so a whole batch
    // can be checked before any of it is applied (§6.3).
    //
    // Some members are currently reached only from test modules, which the
    // compiler does not count as uses when it builds the library target alone.
    // They carry `allow(dead_code)` with the reason that they belong to one
    // contract — the write surface the transaction and history layers consume —
    // and are removed once those layers are complete.

    /// The mutable revision counter, advanced once per committed batch.
    #[allow(dead_code)] // used only by history, which is not written yet
    pub(crate) fn revision_mut(&mut self) -> &mut Revision {
        &mut self.revision
    }

    /// Appends a page.
    pub(crate) fn add_page(&mut self, page: Page) {
        self.pages.push(page);
    }

    /// Looks up a page mutably by identity.
    pub(crate) fn page_mut(&mut self, id: PageId) -> Option<&mut Page> {
        self.pages.iter_mut().find(|p| p.id == id)
    }

    /// Appends an asset.
    ///
    /// Consumed by the file-format importer, which lands after M0's command
    /// path; kept now so the asset list has one writer from the start.
    #[allow(dead_code)] // reached only from the importer
    pub(crate) fn add_asset(&mut self, asset: Asset) {
        self.assets.push(asset);
    }

    /// Removes a page. The caller must have emptied it first.
    ///
    /// Refusing a non-empty page is the caller's job (see the transaction's
    /// `RemovePage`): this method only owns the page list, and silently
    /// orphaning a page's roots is exactly the kind of hidden cascade an undo
    /// must never rely on.
    pub(crate) fn remove_page(&mut self, id: PageId) -> Option<Page> {
        let index = self.pages.iter().position(|p| p.id == id)?;
        Some(self.pages.remove(index))
    }

    /// Stores a node.
    pub(crate) fn insert_node(&mut self, node: Node) {
        self.scene.insert_node(node);
    }

    /// Removes a node and its entire subtree, clearing every index entry that
    /// referenced any removed member.
    ///
    /// The subtree walk is iterative with a visited set so a corrupted document
    /// containing a cycle cannot hang the removal.
    pub(crate) fn remove_node(&mut self, id: NodeId) -> Option<Node> {
        // The subtree walk must read the parent's children before the parent
        // itself is dropped from the store.
        let mut pending: Vec<NodeId> = self
            .scene
            .node(id)
            .map(|n| n.children.clone())
            .unwrap_or_default();
        let mut visited = HashSet::new();
        visited.insert(id);

        while let Some(current) = pending.pop() {
            if !visited.insert(current) {
                continue;
            }
            self.scene.clear_parent(current);
            self.scene.clear_page(current);
            if let Some(children) = self.scene.node(current).map(|n| n.children.clone()) {
                pending.extend(children);
            }
        }

        let node = self.scene.remove_node(id)?;
        self.scene.clear_parent(id);
        self.scene.clear_page(id);
        for doomed in visited {
            if doomed != id {
                self.scene.remove_node(doomed);
            }
        }
        Some(node)
    }

    /// Looks up a node mutably by identity.
    pub(crate) fn node_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.scene.node_mut(id)
    }

    /// Overwrites the derived parent entry for `child`.
    ///
    /// A direct index write, used when corrupting the index deliberately in
    /// tests and by the transaction layer when replaying a recorded edge
    /// change. Prefer [`Document::append_child`] otherwise.
    #[allow(dead_code)] // used only by history, which is not written yet
    pub(crate) fn set_parent(&mut self, child: NodeId, parent: NodeId) {
        self.scene.set_parent(child, parent);
    }

    /// Forgets the derived parent entry for `child`.
    #[allow(dead_code)] // used only by history, which is not written yet
    pub(crate) fn clear_parent(&mut self, child: NodeId) {
        self.scene.clear_parent(child);
    }

    /// Overwrites the derived page entry for `node`.
    #[allow(dead_code)] // used only by history, which is not written yet
    pub(crate) fn set_page(&mut self, node: NodeId, page: PageId) {
        self.scene.set_page(node, page);
    }

    /// Forgets the derived page entry for `node`.
    ///
    /// The direct counterpart of [`Document::propagate_page`]; used by tests
    /// that corrupt the index on purpose and by the importer when rebuilding.
    #[allow(dead_code)] // reached only from tests and the importer
    pub(crate) fn clear_page(&mut self, node: NodeId) {
        self.scene.clear_page(node);
    }

    /// Appends a node to a parent's children, updating the derived index.
    ///
    /// Returns `false` when `parent` does not exist.
    #[allow(dead_code)] // used only by history, which is not written yet
    pub(crate) fn append_child(&mut self, parent: NodeId, child: NodeId) -> bool {
        if !self.scene.contains(parent) {
            return false;
        }
        let len = self.scene.node(parent).map_or(0, |n| n.children.len());
        self.insert_child_at(parent, len, child)
    }

    /// Removes a node from a parent's children, updating the derived index.
    ///
    /// Returns `false` when the edge does not exist.
    pub(crate) fn detach_child(&mut self, parent: NodeId, child: NodeId) -> bool {
        let Some(node) = self.scene.node_mut(parent) else {
            return false;
        };
        let Some(pos) = node.index_of(child) else {
            return false;
        };
        node.children_mut().remove(pos);
        self.scene.clear_parent(child);
        true
    }

    /// Appends a node to a page's roots, updating the derived page index.
    ///
    /// Returns `false` when `page` does not exist.
    #[allow(dead_code)] // used only by history, which is not written yet
    pub(crate) fn append_root(&mut self, page: PageId, root: NodeId) -> bool {
        if !self.pages.iter().any(|p| p.id == page) {
            return false;
        }
        let len = self.page(page).map_or(0, |p| p.roots.len());
        self.insert_root_at(page, len, root)
    }

    /// Removes a node from a page's roots, updating the derived page index.
    ///
    /// Returns `false` when the edge does not exist.
    pub(crate) fn detach_root(&mut self, page: PageId, root: NodeId) -> bool {
        let Some(p) = self.page_mut(page) else {
            return false;
        };
        let Some(pos) = p.index_of_root(root) else {
            return false;
        };
        p.roots_mut().remove(pos);
        self.scene.page_of.remove(&root);
        true
    }

    /// Inserts a node at `index` among a parent's children, shifting the rest.
    ///
    /// `index` is clamped to the children length, so an out-of-range index
    /// appends. Returns `false` when `parent` does not exist.
    pub(crate) fn insert_child_at(&mut self, parent: NodeId, index: usize, child: NodeId) -> bool {
        if !self.scene.contains(parent) {
            return false;
        }
        self.scene.parent_of.remove(&child);
        if let Some(node) = self.scene.node_mut(parent) {
            let children = node.children_mut();
            let at = index.min(children.len());
            children.insert(at, child);
        }
        self.scene.set_parent(child, parent);

        // A node's page is inherited from the container it hangs under, so
        // attaching a subtree must refresh the page of every member. Without
        // this, a node moved from a page root into a frame would keep a stale
        // page entry and a freshly created child would have none at all.
        let page = self.scene.page(parent);
        match page {
            Some(page) => self.propagate_page(child, page),
            None => self.clear_page_recursive(child),
        }
        true
    }

    /// Inserts a node at `index` among a page's roots, shifting the rest.
    ///
    /// `index` is clamped to the roots length, so an out-of-range index
    /// appends. Returns `false` when `page` does not exist.
    pub(crate) fn insert_root_at(&mut self, page: PageId, index: usize, root: NodeId) -> bool {
        if !self.pages.iter().any(|p| p.id == page) {
            return false;
        }
        self.scene.clear_parent(root);
        if let Some(p) = self.page_mut(page) {
            let roots = p.roots_mut();
            let at = index.min(roots.len());
            roots.insert(at, root);
        }
        self.propagate_page(root, page);
        true
    }

    /// Sets the derived page entry for a node and every member of its subtree.
    ///
    /// Iterative with a visited set: a corrupted document can contain a cycle,
    /// and index maintenance must not hang or overflow the stack while the
    /// validator is still on its way to reporting the cycle.
    pub(crate) fn propagate_page(&mut self, node: NodeId, page: PageId) {
        let mut pending = vec![node];
        let mut visited = HashSet::new();
        while let Some(current) = pending.pop() {
            if !visited.insert(current) {
                continue;
            }
            self.scene.set_page(current, page);
            if let Some(children) = self.scene.node(current).map(|n| n.children.clone()) {
                pending.extend(children);
            }
        }
    }

    /// Clears the derived page entry for a node and every member of its subtree.
    fn clear_page_recursive(&mut self, node: NodeId) {
        let mut pending = vec![node];
        let mut visited = HashSet::new();
        while let Some(current) = pending.pop() {
            if !visited.insert(current) {
                continue;
            }
            self.scene.clear_page(current);
            if let Some(children) = self.scene.node(current).map(|n| n.children.clone()) {
                pending.extend(children);
            }
        }
    }

    /// Rebuilds the derived indexes from the canonical children and roots
    /// lists. Called after bulk mutations; never a substitute for keeping the
    /// canonical lists correct.
    #[allow(dead_code)] // reached only from tests and the importer
    pub(crate) fn rebuild_derived_indexes(&mut self) {
        self.scene.rebuild_indexes();
        // Roots establish the page mapping.
        let pages: Vec<(PageId, Vec<NodeId>)> =
            self.pages.iter().map(|p| (p.id, p.roots.clone())).collect();
        for (page_id, roots) in pages {
            for root in roots {
                self.scene.set_page(root, page_id);
            }
        }
    }
}

/// A shared empty extension map, so the accessors can return a reference
/// without allocating or storing an empty map per entity.
fn empty_extensions() -> &'static Extensions {
    static EMPTY: std::sync::OnceLock<Extensions> = std::sync::OnceLock::new();
    EMPTY.get_or_init(Extensions::new)
}

impl Default for Document {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf(kind: NodeKind, name: &str) -> Node {
        Node::new(NodeId::new(), kind, Some(name.to_owned()))
    }

    struct Fixture {
        doc: Document,
        page_id: PageId,
        frame: NodeId,
        a: NodeId,
        b: NodeId,
    }

    /// One page containing a frame with two shape children: `frame[ a, b ]`.
    fn fixture() -> Fixture {
        let page_id = PageId::new();
        let frame = leaf(NodeKind::Frame, "frame");
        let a = leaf(NodeKind::Shape, "a");
        let b = leaf(NodeKind::Shape, "b");
        let (frame_id, a_id, b_id) = (frame.id, a.id, b.id);

        let mut doc = Document::new();
        doc.insert_node(frame);
        doc.insert_node(a);
        doc.insert_node(b);
        doc.add_page(Page::new(page_id, "Page 1"));

        assert!(doc.append_child(frame_id, a_id));
        assert!(doc.append_child(frame_id, b_id));
        assert!(doc.append_root(page_id, frame_id));

        Fixture {
            doc,
            page_id,
            frame: frame_id,
            a: a_id,
            b: b_id,
        }
    }

    #[test]
    fn new_document_starts_at_the_initial_revision() {
        let doc = Document::new();
        assert_eq!(doc.revision(), Revision::INITIAL);
        assert_eq!(doc.revision().as_u64(), 0);
    }

    #[test]
    fn new_document_has_a_fresh_identity_and_no_content() {
        let a = Document::new();
        let b = Document::new();
        assert_ne!(a.id(), b.id(), "identities must not collide");
        assert_eq!(a.pages().len(), 0);
        assert_eq!(a.node_count(), 0);
        assert_eq!(a.assets().len(), 0);
    }

    #[test]
    fn with_id_preserves_a_caller_supplied_identity() {
        let wanted = DocumentId::new();
        assert_eq!(Document::with_id(wanted).id(), wanted);
    }

    #[test]
    fn default_document_equals_a_new_document_in_shape() {
        let doc = Document::default();
        assert_eq!(doc.revision(), Revision::INITIAL);
        assert_eq!(doc.node_count(), 0);
    }

    #[test]
    fn revision_advances_by_exactly_one_per_step() {
        assert_eq!(Revision::INITIAL.next(), Revision::from_raw(1));
        assert_eq!(Revision::from_raw(41).next(), Revision::from_raw(42));
    }

    #[test]
    fn revisions_are_totally_ordered_for_optimistic_concurrency() {
        assert!(Revision::from_raw(1) < Revision::from_raw(2));
        assert!(Revision::from_raw(2) > Revision::from_raw(1));
        assert_eq!(Revision::from_raw(7), Revision::from_raw(7));
    }

    #[test]
    fn revision_displays_as_its_raw_value() {
        assert_eq!(Revision::from_raw(123).to_string(), "123");
    }

    #[test]
    fn node_kind_child_capability_matches_the_primitive_set() {
        assert!(NodeKind::Frame.can_have_children());
        assert!(NodeKind::Group.can_have_children());
        assert!(!NodeKind::Shape.can_have_children());
        assert!(!NodeKind::Text.can_have_children());
        assert!(!NodeKind::Image.can_have_children());
    }

    #[test]
    fn node_kind_str_names_are_stable() {
        assert_eq!(NodeKind::Frame.as_str(), "frame");
        assert_eq!(NodeKind::Group.as_str(), "group");
        assert_eq!(NodeKind::Shape.as_str(), "shape");
        assert_eq!(NodeKind::Text.as_str(), "text");
        assert_eq!(NodeKind::Image.as_str(), "image");
    }

    #[test]
    fn children_preserve_insertion_order() {
        let f = fixture();
        let frame = f.doc.node(f.frame).expect("frame exists");
        assert_eq!(frame.children(), &[f.a, f.b]);
        assert_eq!(frame.index_of(f.a), Some(0));
        assert_eq!(frame.index_of(f.b), Some(1));
    }

    #[test]
    fn parent_lookup_is_derived_from_the_children_list() {
        let f = fixture();
        assert_eq!(f.doc.parent_of(f.a), Some(f.frame));
        assert_eq!(f.doc.parent_of(f.b), Some(f.frame));
        assert_eq!(
            f.doc.parent_of(f.frame),
            None,
            "a page root has no parent node"
        );
    }

    #[test]
    fn page_lookup_is_derived_from_the_roots_list() {
        let f = fixture();
        assert_eq!(f.doc.page_of(f.frame), Some(f.page_id));
        assert_eq!(
            f.doc.page_of(f.a),
            Some(f.page_id),
            "a descendant belongs to the page its root is on"
        );
    }

    #[test]
    fn append_child_rejects_a_missing_parent() {
        let mut f = fixture();
        let orphan = leaf(NodeKind::Shape, "orphan");
        assert!(!f.doc.append_child(NodeId::new(), orphan.id));
    }

    #[test]
    fn detach_child_removes_only_the_named_edge() {
        let mut f = fixture();
        assert!(f.doc.detach_child(f.frame, f.a));
        assert_eq!(f.doc.parent_of(f.a), None);
        assert_eq!(f.doc.parent_of(f.b), Some(f.frame));

        assert!(
            !f.doc.detach_child(f.frame, f.a),
            "detaching twice must report failure rather than silently succeed"
        );
    }

    #[test]
    fn insert_child_at_honours_the_requested_position() {
        let mut f = fixture();
        let c = leaf(NodeKind::Shape, "c");
        f.doc.insert_node(c.clone());
        assert!(f.doc.insert_child_at(f.frame, 0, c.id));

        let frame = f.doc.node(f.frame).expect("frame exists");
        assert_eq!(frame.children(), &[c.id, f.a, f.b]);
        assert_eq!(f.doc.parent_of(c.id), Some(f.frame));
    }

    #[test]
    fn insert_child_at_clamps_an_out_of_range_index() {
        let mut f = fixture();
        let c = leaf(NodeKind::Shape, "c");
        f.doc.insert_node(c.clone());
        assert!(f.doc.insert_child_at(f.frame, 9_999, c.id));

        let frame = f.doc.node(f.frame).expect("frame exists");
        assert_eq!(frame.children(), &[f.a, f.b, c.id]);
    }

    #[test]
    fn append_root_rejects_a_missing_page() {
        let mut f = fixture();
        let c = leaf(NodeKind::Shape, "c");
        f.doc.insert_node(c.clone());
        assert!(!f.doc.append_root(PageId::new(), c.id));
    }

    #[test]
    fn rebuild_derived_indexes_is_idempotent() {
        let mut f = fixture();
        let before = [f.doc.parent_of(f.a), f.doc.parent_of(f.b)];
        let page_before = f.doc.page_of(f.frame);

        f.doc.rebuild_derived_indexes();
        f.doc.rebuild_derived_indexes();

        assert_eq!(before, [f.doc.parent_of(f.a), f.doc.parent_of(f.b)]);
        assert_eq!(f.doc.page_of(f.frame), page_before);
    }

    #[test]
    fn rebuild_derived_indexes_recovers_a_corrupted_parent_index() {
        let mut f = fixture();
        f.doc.set_parent(f.a, f.b);
        assert_eq!(f.doc.parent_of(f.a), Some(f.b), "index is corrupted");

        f.doc.rebuild_derived_indexes();

        assert_eq!(
            f.doc.parent_of(f.a),
            Some(f.frame),
            "the canonical children list must win over the stale index"
        );
    }

    #[test]
    fn rebuild_derived_indexes_recovers_a_corrupted_page_index() {
        let mut f = fixture();
        f.doc.scene.page_of.clear();

        f.doc.rebuild_derived_indexes();

        assert_eq!(f.doc.page_of(f.frame), Some(f.page_id));
    }

    #[test]
    fn a_moved_node_keeps_its_identity() {
        let mut f = fixture();

        // Detach `a` from the frame and re-parent it under a new group.
        assert!(f.doc.detach_child(f.frame, f.a));
        let group = leaf(NodeKind::Group, "group");
        f.doc.insert_node(group.clone());
        assert!(f.doc.append_child(group.id, f.a));
        assert!(f.doc.append_root(f.page_id, group.id));

        assert_eq!(f.doc.parent_of(f.a), Some(group.id));
        assert_eq!(
            f.doc.node(f.a).expect("still present").id,
            f.a,
            "moving must not change identity"
        );
        assert_eq!(f.doc.node(f.frame).expect("frame").children(), &[f.b]);
        assert_eq!(
            f.doc.page_of(f.a),
            Some(f.page_id),
            "the node follows its new parent onto the page"
        );
    }

    #[test]
    fn page_lookup_is_by_identity() {
        let f = fixture();
        let page = f.doc.page(f.page_id).expect("page exists");
        assert_eq!(page.name, "Page 1");
        assert_eq!(page.roots(), &[f.frame]);
        assert_eq!(page.index_of_root(f.frame), Some(0));
        assert!(f.doc.page(PageId::new()).is_none());
    }

    #[test]
    fn node_lookup_is_by_identity() {
        let f = fixture();
        let n = f.doc.node(f.a).expect("node exists");
        assert_eq!(n.kind, NodeKind::Shape);
        assert_eq!(n.name.as_deref(), Some("a"));
        assert_eq!(n.id, f.a);

        assert!(f.doc.node(NodeId::new()).is_none());
        assert!(f.doc.contains_node(f.a));
        assert!(!f.doc.contains_node(NodeId::new()));
    }

    #[test]
    fn nodes_iterates_every_node_exactly_once() {
        let f = fixture();
        assert_eq!(f.doc.node_count(), 3);
        assert_eq!(f.doc.nodes().count(), 3);
    }

    #[test]
    fn assets_are_document_scoped_and_identified() {
        let mut doc = Document::new();
        let asset = Asset {
            id: AssetId::new(),
            name: "logo.png".to_owned(),
        };
        let asset_id = asset.id;
        doc.add_asset(asset);

        assert_eq!(doc.assets().len(), 1);
        assert_eq!(
            doc.asset(asset_id).map(|a| a.name.as_str()),
            Some("logo.png")
        );
        assert!(doc.asset(AssetId::new()).is_none());
    }

    #[test]
    fn detach_root_clears_the_page_mapping() {
        let mut f = fixture();
        assert!(f.doc.detach_root(f.page_id, f.frame));
        assert_eq!(f.doc.page_of(f.frame), None);
        assert!(
            !f.doc.detach_root(f.page_id, f.frame),
            "detaching twice must report failure"
        );
    }

    #[test]
    fn revision_mut_advances_the_counter() {
        let mut doc = Document::new();
        *doc.revision_mut() = doc.revision().next();
        assert_eq!(doc.revision(), Revision::from_raw(1));
    }

    #[test]
    fn node_mut_edits_a_node_in_place() {
        let mut f = fixture();
        f.doc.node_mut(f.a).expect("node exists").name = Some("renamed".to_owned());
        assert_eq!(
            f.doc.node(f.a).and_then(|n| n.name.as_deref()),
            Some("renamed")
        );
        assert!(f.doc.node_mut(NodeId::new()).is_none());
    }

    #[test]
    fn remove_node_drops_the_node_and_its_index_entries() {
        let mut f = fixture();
        assert!(f.doc.detach_child(f.frame, f.a));

        let removed = f.doc.remove_node(f.a).expect("node was present");
        assert_eq!(removed.id, f.a);
        assert!(!f.doc.contains_node(f.a));
        assert_eq!(f.doc.parent_of(f.a), None);
        assert_eq!(f.doc.page_of(f.a), None);
        assert_eq!(f.doc.node_count(), 2);
        assert!(
            f.doc.remove_node(f.a).is_none(),
            "removing twice is a no-op"
        );
    }

    #[test]
    fn clear_parent_forgets_a_single_edge() {
        let mut f = fixture();
        f.doc.clear_parent(f.a);
        assert_eq!(f.doc.parent_of(f.a), None);
        assert_eq!(f.doc.parent_of(f.b), Some(f.frame));
    }

    #[test]
    fn page_mut_edits_a_page_in_place() {
        let mut f = fixture();
        f.doc.page_mut(f.page_id).expect("page exists").name = "Renamed".to_owned();
        assert_eq!(
            f.doc.page(f.page_id).map(|p| p.name.as_str()),
            Some("Renamed")
        );
        assert!(f.doc.page_mut(PageId::new()).is_none());
    }

    #[test]
    fn removing_the_last_child_leaves_an_empty_ordered_list() {
        let mut f = fixture();
        assert!(f.doc.detach_child(f.frame, f.a));
        assert!(f.doc.detach_child(f.frame, f.b));
        assert!(f.doc.node(f.frame).expect("frame").children().is_empty());
        assert_eq!(f.doc.parent_of(f.a), None);
        assert_eq!(f.doc.parent_of(f.b), None);
    }

    #[test]
    fn a_frame_can_be_reparented_under_another_frame() {
        let mut f = fixture();
        let outer = leaf(NodeKind::Frame, "outer");
        f.doc.insert_node(outer.clone());
        assert!(f.doc.detach_root(f.page_id, f.frame));
        assert!(f.doc.append_child(outer.id, f.frame));
        assert!(f.doc.append_root(f.page_id, outer.id));

        assert_eq!(f.doc.parent_of(f.frame), Some(outer.id));
        assert_eq!(f.doc.page_of(outer.id), Some(f.page_id));
        assert_eq!(
            f.doc.node(outer.id).expect("outer").children(),
            &[f.frame],
            "the nested frame keeps its own children"
        );
    }
}
