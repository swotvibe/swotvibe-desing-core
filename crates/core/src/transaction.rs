//! Atomic application and change-set computation.
//!
//! Ownership: atomic batch application and the resulting change set.
//! Not owned: writing files or network I/O.
//!
//! A batch either satisfies all constraints and is committed in full, or is
//! rejected with no partial effect on the document, revision, or history.
//!
//! Atomicity is achieved by construction rather than by rollback: the batch is
//! applied to a detached working copy, and that copy replaces the live document
//! only after every command has succeeded. There is therefore no code path that
//! can leave a half-applied batch behind.

use crate::commands::{Command, CommandError, CommandErrorCode, NodePlacement, Position};
use crate::ids::{AssetId, NodeId, PageId};
use crate::model::{Asset, Document, Node, NodeKind, Page, Revision};
use crate::props::NodeProps;
use std::collections::BTreeSet;

/// A record of one command's observable effect, computed from the state that
/// actually existed at apply time.
///
/// Effects are the only sound basis for an inverse command: a caller's
/// requested values may differ from what was really applied (a `Position::Last`
/// request, or an index that had to be clamped). Recording the effect rather
/// than the request is what keeps undo honest (§6.4).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Effect {
    /// A page was created.
    PageCreated {
        /// The new page identity.
        page: PageId,
    },
    /// A page was removed.
    PageRemoved {
        /// The removed page identity.
        page: PageId,
    },
    /// An asset was registered.
    AssetCreated {
        /// The new asset identity.
        asset: AssetId,
    },
    /// A node was created and attached.
    NodeCreated {
        /// The new node identity.
        node: NodeId,
        /// Its parent, or `None` when it is a page root.
        parent: Option<NodeId>,
    },
    /// A node and its whole subtree were removed.
    NodeRemoved {
        /// The removed subtree root.
        node: NodeId,
        /// The parent it was attached to, or `None` when it was a page root.
        old_parent: Option<NodeId>,
        /// Its position before removal.
        old_index: usize,
        /// The page that owned it.
        old_page: PageId,
    },
    /// A node was reparented and/or repositioned.
    NodeMoved {
        /// The moved node.
        node: NodeId,
        /// Its parent before the move, or `None` when it was a page root.
        old_parent: Option<NodeId>,
        /// Its index before the move.
        old_index: usize,
        /// The page that owned it before the move.
        old_page: PageId,
    },
    /// A node's name changed.
    NodeRenamed {
        /// The renamed node.
        node: NodeId,
        /// The previous name, or `None` when it was unnamed.
        old_name: Option<String>,
    },
    /// A node's properties were replaced.
    NodePropsChanged {
        /// The node whose properties changed.
        node: NodeId,
        /// The properties before the change.
        old_props: Box<NodeProps>,
    },
    /// A node's sibling order changed without a reparent.
    NodeReordered {
        /// The parent whose order changed, or `None` for a page's roots.
        parent: Option<NodeId>,
        /// The page that owns the container.
        page: PageId,
        /// The previous order.
        old_order: Vec<NodeId>,
    },
}

/// A summary of everything a committed batch changed (§6.5).
///
/// The change set is derived, never authoritative: it is computed by comparing
/// the document before and after the batch, so it cannot drift from the state
/// that was actually committed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeSet {
    previous_revision: Revision,
    new_revision: Revision,
    added_nodes: BTreeSet<NodeId>,
    removed_nodes: BTreeSet<NodeId>,
    changed_nodes: BTreeSet<NodeId>,
    moved_nodes: BTreeSet<NodeId>,
    added_pages: BTreeSet<PageId>,
    changed_pages: BTreeSet<PageId>,
    added_assets: BTreeSet<AssetId>,
    affected_pages: BTreeSet<PageId>,
    effects: Vec<Effect>,
}

impl ChangeSet {
    /// The revision the document had before the batch.
    #[must_use]
    pub fn previous_revision(&self) -> Revision {
        self.previous_revision
    }

    /// The revision the document has after the batch.
    #[must_use]
    pub fn new_revision(&self) -> Revision {
        self.new_revision
    }

    /// Nodes that did not exist before the batch.
    pub fn added_nodes(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.added_nodes.iter().copied()
    }

    /// Nodes that no longer exist. Removing a subtree lists every member.
    pub fn removed_nodes(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.removed_nodes.iter().copied()
    }

    /// Nodes whose stored value changed without being added, removed, or moved.
    pub fn changed_nodes(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.changed_nodes.iter().copied()
    }

    /// Nodes whose parent or position within a parent changed.
    pub fn moved_nodes(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.moved_nodes.iter().copied()
    }

    /// Pages that were added.
    pub fn added_pages(&self) -> impl Iterator<Item = PageId> + '_ {
        self.added_pages.iter().copied()
    }

    /// Pages whose name or root set changed, including a page that was removed.
    pub fn changed_pages(&self) -> impl Iterator<Item = PageId> + '_ {
        self.changed_pages.iter().copied()
    }

    /// Assets that were added.
    pub fn added_assets(&self) -> impl Iterator<Item = AssetId> + '_ {
        self.added_assets.iter().copied()
    }

    /// Pages a consumer must repaint or recompute.
    ///
    /// Superset of [`ChangeSet::changed_pages`]: it also includes every page
    /// that gained or lost a node, so a cross-page move marks both the page a
    /// node left and the page it arrived on.
    #[must_use]
    pub fn affected_pages(&self) -> &BTreeSet<PageId> {
        &self.affected_pages
    }

    /// The ordered per-command effects that produced this change set.
    #[must_use]
    pub fn effects(&self) -> &[Effect] {
        &self.effects
    }

    /// Whether the batch changed nothing observable.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.added_nodes.is_empty()
            && self.removed_nodes.is_empty()
            && self.changed_nodes.is_empty()
            && self.moved_nodes.is_empty()
            && self.added_pages.is_empty()
            && self.changed_pages.is_empty()
            && self.added_assets.is_empty()
    }
}

/// The outcome of a committed batch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    change_set: ChangeSet,
}

impl Commit {
    /// The change set describing the batch.
    #[must_use]
    pub fn change_set(&self) -> &ChangeSet {
        &self.change_set
    }

    /// The per-command effects, in command order.
    #[must_use]
    pub fn effects(&self) -> &[Effect] {
        &self.change_set.effects
    }

    /// The revision the document now carries.
    #[must_use]
    pub fn revision(&self) -> Revision {
        self.change_set.new_revision
    }
}

/// Why a batch was rejected.
///
/// Every rejection leaves the document untouched: the caller's expectation did
/// not match ([`BatchError::RevisionConflict`]), a command was invalid
/// ([`BatchError::Command`]), or the revision counter cannot advance
/// ([`BatchError::RevisionExhausted`]).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BatchError {
    /// The caller's expected revision did not match the document's revision, so
    /// the batch was not attempted at all.
    #[error("batch expected revision {expected}, but the document is at {actual}")]
    RevisionConflict {
        /// The revision supplied by the caller.
        expected: Revision,
        /// The document's actual revision.
        actual: Revision,
    },
    /// The revision counter reached its maximum value and cannot advance
    /// without reusing a revision.
    #[error("document revision counter is exhausted")]
    RevisionExhausted,
    /// A command was rejected by state-level validation.
    #[error(transparent)]
    Command(#[from] CommandError),
}

impl BatchError {
    /// Returns the stable command error code when a command was rejected.
    /// Revision conflicts and revision exhaustion have no command code.
    #[must_use]
    pub const fn code(&self) -> Option<CommandErrorCode> {
        match self {
            Self::Command(error) => Some(error.code),
            Self::RevisionConflict { .. } | Self::RevisionExhausted => None,
        }
    }
}

/// Applies one batch of commands atomically to `doc`.
///
/// The batch runs against a detached working copy that becomes the live
/// document only after every command succeeds, so a rejected batch leaves
/// `doc`, its revision, and any history untouched. On success the revision
/// advances by exactly one regardless of how many commands the batch held.
///
/// # Errors
///
/// Returns [`BatchError::RevisionConflict`] when `expected_revision` is not the
/// document's current revision, [`BatchError::RevisionExhausted`] when the
/// revision counter cannot advance, or [`BatchError::Command`] when any command
/// is invalid. No error modifies `doc`.
///
/// # Examples
///
/// ```
/// use swotvibe_core::{apply_batch, Command, Document, PageId, Revision};
///
/// let mut doc = Document::new();
/// let page = PageId::new();
/// let commit = apply_batch(
///     &mut doc,
///     Revision::INITIAL,
///     &[Command::CreatePage {
///         id: page,
///         name: "Page 1".into(),
///     }],
/// )
/// .expect("a fresh document accepts a page creation");
///
/// assert_eq!(commit.revision(), Revision::INITIAL.next());
/// assert!(doc.page(page).is_some());
/// ```
pub fn apply_batch(
    doc: &mut Document,
    expected_revision: Revision,
    commands: &[Command],
) -> Result<Commit, BatchError> {
    if doc.revision() != expected_revision {
        return Err(BatchError::RevisionConflict {
            expected: expected_revision,
            actual: doc.revision(),
        });
    }

    if doc.revision().as_u64() == u64::MAX {
        return Err(BatchError::RevisionExhausted);
    }

    let before = doc.clone();
    let mut working = doc.clone();
    let mut effects = Vec::with_capacity(commands.len());
    let mut created_pages = Vec::new();

    for command in commands {
        let effect = apply_one(&mut working, command, &mut created_pages)?;
        effects.push(effect);
    }

    let change_set = diff(
        &before,
        &working,
        effects,
        created_pages,
        before.revision().next(),
    );
    working.set_revision(before.revision().next());
    *doc = working;

    Ok(Commit { change_set })
}

/// Applies a batch against the document's current revision.
///
/// A convenience over [`apply_batch`] for a sole writer, which reads the
/// revision so the caller cannot pass a stale one. A caller that shares the
/// document with another writer should capture the revision deliberately and
/// call [`apply_batch`] instead.
///
/// # Errors
///
/// As [`apply_batch`], minus the revision conflict this cannot itself produce
/// between reading and use.
pub fn apply_batch_at_current(
    doc: &mut Document,
    commands: &[Command],
) -> Result<Commit, BatchError> {
    let revision = doc.revision();
    apply_batch(doc, revision, commands)
}

fn apply_one(
    doc: &mut Document,
    command: &Command,
    created_pages: &mut Vec<PageId>,
) -> Result<Effect, CommandError> {
    match command {
        Command::CreatePage { id, name } => {
            if doc.page(*id).is_some() {
                return Err(CommandError::on_page(
                    CommandErrorCode::IdCollision,
                    *id,
                    "a page with this identity already exists",
                ));
            }
            doc.add_page(Page::new(*id, name.clone()));
            created_pages.push(*id);
            Ok(Effect::PageCreated { page: *id })
        }
        Command::CreateAsset { id, name } => {
            if doc.asset(*id).is_some() {
                return Err(CommandError::on_asset(
                    CommandErrorCode::IdCollision,
                    *id,
                    "an asset with this identity already exists",
                ));
            }
            doc.add_asset(Asset::new(*id, name.clone()));
            Ok(Effect::AssetCreated { asset: *id })
        }
        Command::CreateNode {
            id,
            kind,
            name,
            parent,
        } => apply_create_node(doc, *id, *kind, name.as_deref(), parent),
        Command::DeleteNode { id } => apply_delete_node(doc, *id),
        Command::MoveNode { id, parent } => apply_move_node(doc, *id, parent),
        Command::RenameNode { id, name } => apply_rename_node(doc, *id, name.as_deref()),
        Command::SetNodeProps { id, props } => apply_set_node_props(doc, *id, props),
        Command::ReorderNode { id, position } => apply_reorder_node(doc, *id, *position),
        Command::ReorderTo {
            parent,
            page,
            order,
        } => apply_reorder_to(doc, *parent, *page, order),
        Command::RemovePage { id } => apply_remove_page(doc, *id),
    }
}

/// Resolves the parent a new node should attach under, or `None` for a root.
fn resolve_new_parent(
    doc: &Document,
    kind: NodeKind,
    placement: &NodePlacement,
) -> Result<Option<NodeId>, CommandError> {
    match placement {
        NodePlacement::PageRoot { page, .. } => {
            require_page(doc, *page)?;
            if !kind.can_be_root() {
                return Err(CommandError::general(
                    CommandErrorCode::NotARoot,
                    "this node kind cannot be a page root",
                ));
            }
            Ok(None)
        }
        NodePlacement::Child { parent, .. } => {
            require_container(doc, *parent)?;
            Ok(Some(*parent))
        }
    }
}

fn require_page(doc: &Document, page: PageId) -> Result<(), CommandError> {
    if doc.page(page).is_some() {
        Ok(())
    } else {
        Err(CommandError::on_page(
            CommandErrorCode::PageNotFound,
            page,
            "the target page does not exist",
        ))
    }
}

fn require_node(doc: &Document, node: NodeId) -> Result<(), CommandError> {
    if doc.node(node).is_some() {
        Ok(())
    } else {
        Err(CommandError::on_node(
            CommandErrorCode::NodeNotFound,
            node,
            "the target node does not exist",
        ))
    }
}

fn require_container(doc: &Document, node: NodeId) -> Result<(), CommandError> {
    let target = doc.node(node).ok_or_else(|| {
        CommandError::on_node(
            CommandErrorCode::NodeNotFound,
            node,
            "the target parent node does not exist",
        )
    })?;
    if !target.kind.can_own_children() {
        return Err(CommandError::on_node(
            CommandErrorCode::KindCannotOwnChildren,
            node,
            "this node kind cannot own children",
        ));
    }
    Ok(())
}

/// Resolves a page-root position, which may name a sibling reference.
fn resolve_root_index(
    doc: &Document,
    page: PageId,
    position: Position,
) -> Result<usize, CommandError> {
    let roots: Vec<NodeId> = doc
        .page(page)
        .map(|p| p.roots().to_vec())
        .unwrap_or_default();
    let len = roots.len();
    match position {
        Position::First => Ok(0),
        Position::Last => Ok(len),
        Position::At(index) => Ok(index.min(len)),
        Position::Before(reference) | Position::After(reference) => {
            let Some(found) = roots.iter().position(|r| *r == reference) else {
                return Err(CommandError::on_node(
                    CommandErrorCode::NodeNotFound,
                    reference,
                    "the referenced sibling is not a root of this page",
                ));
            };
            Ok(if matches!(position, Position::After(_)) {
                found + 1
            } else {
                found
            })
        }
    }
}

/// Resolves a child position, excluding `exclude` so an in-place move does not
/// count the node against itself.
fn resolve_child_index(
    doc: &Document,
    parent: NodeId,
    position: Position,
    exclude: Option<NodeId>,
) -> Result<usize, CommandError> {
    let children: Vec<NodeId> = doc
        .node(parent)
        .map(|n| n.children().to_vec())
        .unwrap_or_default();
    let candidates: Vec<NodeId> = children
        .iter()
        .copied()
        .filter(|c| Some(*c) != exclude)
        .collect();
    let len = candidates.len();

    match position {
        Position::First => Ok(0),
        Position::Last => Ok(len),
        Position::At(index) => Ok(index.min(len)),
        Position::Before(reference) | Position::After(reference) => {
            let Some(found) = candidates.iter().position(|c| *c == reference) else {
                return Err(CommandError::on_node(
                    CommandErrorCode::NodeNotFound,
                    reference,
                    "the referenced sibling is not a child of this parent",
                ));
            };
            Ok(if matches!(position, Position::After(_)) {
                found + 1
            } else {
                found
            })
        }
    }
}

fn apply_create_node(
    doc: &mut Document,
    id: NodeId,
    kind: NodeKind,
    name: Option<&str>,
    placement: &NodePlacement,
) -> Result<Effect, CommandError> {
    if doc.node(id).is_some() {
        return Err(CommandError::on_node(
            CommandErrorCode::NodeAlreadyExists,
            id,
            "a node with this identity already exists",
        ));
    }

    let parent = resolve_new_parent(doc, kind, placement)?;
    let index = match placement {
        NodePlacement::Child { parent, position } => {
            resolve_child_index(doc, *parent, *position, None)?
        }
        NodePlacement::PageRoot { page, position } => resolve_root_index(doc, *page, *position)?,
    };

    doc.insert_node(Node::new(id, kind, name.map(str::to_owned)));
    match placement {
        NodePlacement::Child { parent, .. } => {
            doc.insert_child_at(*parent, index, id);
        }
        NodePlacement::PageRoot { page, .. } => {
            doc.insert_root_at(*page, index, id);
        }
    }

    Ok(Effect::NodeCreated { node: id, parent })
}

/// Returns the current parent and index of `node`, plus the page owning it.
fn current_position(doc: &Document, node: NodeId) -> Option<(Option<NodeId>, usize, PageId)> {
    let page = doc.page_of(node)?;
    let parent = doc.parent_of(node);
    let index = match parent {
        Some(parent) => doc.node(parent)?.index_of(node)?,
        None => doc.page(page)?.index_of_root(node)?,
    };
    Some((parent, index, page))
}

fn apply_delete_node(doc: &mut Document, id: NodeId) -> Result<Effect, CommandError> {
    require_node(doc, id)?;
    let Some((old_parent, old_index, old_page)) = current_position(doc, id) else {
        return Err(CommandError::on_node(
            CommandErrorCode::NodeNotFound,
            id,
            "the node is not reachable from any page",
        ));
    };

    detach(doc, id, old_parent, old_page);
    doc.remove_node(id);

    Ok(Effect::NodeRemoved {
        node: id,
        old_parent,
        old_index,
        old_page,
    })
}

fn detach(doc: &mut Document, node: NodeId, parent: Option<NodeId>, page: PageId) {
    match parent {
        Some(parent) => {
            doc.detach_child(parent, node);
        }
        None => {
            doc.detach_root(page, node);
        }
    }
}

fn apply_move_node(
    doc: &mut Document,
    id: NodeId,
    placement: &NodePlacement,
) -> Result<Effect, CommandError> {
    require_node(doc, id)?;
    let Some((old_parent, old_index, old_page)) = current_position(doc, id) else {
        return Err(CommandError::on_node(
            CommandErrorCode::NodeNotFound,
            id,
            "the node is not reachable from any page",
        ));
    };

    let kind = doc.node(id).map_or(NodeKind::Frame, |n| n.kind);
    let new_parent = resolve_new_parent(doc, kind, placement)?;

    // A node may not be moved into its own subtree: that would both create a
    // cycle and make the subtree unreachable from any page.
    if new_parent == Some(id) || new_parent.is_some_and(|p| doc.is_descendant_of(p, id)) {
        return Err(CommandError::on_node(
            CommandErrorCode::WouldCycle,
            id,
            "the destination lies inside the moved node's own subtree",
        ));
    }

    let new_page = match placement {
        NodePlacement::PageRoot { page, .. } => *page,
        NodePlacement::Child { .. } => {
            new_parent.and_then(|p| doc.page_of(p)).ok_or_else(|| {
                CommandError::general(
                    CommandErrorCode::PreconditionFailed,
                    "the destination parent is not reachable from any page",
                )
            })?
        }
    };

    // Resolve the index after detaching, so an in-container move does not count
    // the node against itself.
    let same_container = old_parent == new_parent && old_page == new_page;
    let exclude = if same_container { Some(id) } else { None };
    let position = placement.position();
    let index = match new_parent {
        Some(parent) => resolve_child_index(doc, parent, position, exclude)?,
        None => resolve_root_index(doc, new_page, position)?,
    };

    detach(doc, id, old_parent, old_page);
    match new_parent {
        Some(parent) => {
            doc.insert_child_at(parent, index, id);
        }
        None => {
            doc.insert_root_at(new_page, index, id);
        }
    }

    Ok(Effect::NodeMoved {
        node: id,
        old_parent,
        old_index,
        old_page,
    })
}

/// Reorders a node among its current siblings at its current parent.
fn apply_reorder_node(
    doc: &mut Document,
    id: NodeId,
    position: Position,
) -> Result<Effect, CommandError> {
    require_node(doc, id)?;
    let Some((parent, _, page)) = current_position(doc, id) else {
        return Err(CommandError::on_node(
            CommandErrorCode::NodeNotFound,
            id,
            "the node is not reachable from any page",
        ));
    };

    let old_order: Vec<NodeId> = match parent {
        Some(parent) => doc
            .node(parent)
            .map(|n| n.children().to_vec())
            .unwrap_or_default(),
        None => doc
            .page(page)
            .map(|p| p.roots().to_vec())
            .unwrap_or_default(),
    };

    let mut reordered: Vec<NodeId> = old_order.iter().copied().filter(|n| *n != id).collect();
    let index = match parent {
        Some(parent) => resolve_child_index(doc, parent, position, Some(id))?,
        None => {
            let len = reordered.len();
            match position {
                Position::First => 0,
                Position::Last => len,
                Position::At(index) => index.min(len),
                Position::Before(reference) | Position::After(reference) => {
                    let Some(found) = reordered.iter().position(|n| *n == reference) else {
                        return Err(CommandError::on_node(
                            CommandErrorCode::NodeNotFound,
                            reference,
                            "the referenced sibling is not a root of this page",
                        ));
                    };
                    if matches!(position, Position::After(_)) {
                        found + 1
                    } else {
                        found
                    }
                }
            }
        }
    };

    let index = index.min(reordered.len());
    reordered.insert(index, id);

    if reordered == old_order {
        return Err(CommandError::general(
            CommandErrorCode::PreconditionFailed,
            "the requested order is identical to the current order",
        ));
    }

    match parent {
        Some(parent) => {
            doc.set_child_order(parent, &reordered);
        }
        None => {
            doc.set_root_order(page, &reordered);
        }
    }

    Ok(Effect::NodeReordered {
        parent,
        page,
        old_order,
    })
}

fn apply_rename_node(
    doc: &mut Document,
    id: NodeId,
    name: Option<&str>,
) -> Result<Effect, CommandError> {
    let Some(node) = doc.node_mut(id) else {
        return Err(CommandError::on_node(
            CommandErrorCode::NodeNotFound,
            id,
            "the target node does not exist",
        ));
    };
    let old_name = node.name.clone();
    node.set_name(name.unwrap_or_default());

    Ok(Effect::NodeRenamed { node: id, old_name })
}

fn apply_set_node_props(
    doc: &mut Document,
    id: NodeId,
    props: &NodeProps,
) -> Result<Effect, CommandError> {
    let Some(kind) = doc.node(id).map(|node| node.kind) else {
        return Err(CommandError::on_node(
            CommandErrorCode::NodeNotFound,
            id,
            "the target node does not exist",
        ));
    };
    props.check(kind).map_err(|error| {
        CommandError::on_node(CommandErrorCode::InvalidProps, id, error.to_string())
    })?;
    if let Some(asset) = props.asset()
        && doc.asset(asset).is_none()
    {
        return Err(CommandError::on_node(
            CommandErrorCode::AssetNotFound,
            id,
            "the image refers to an asset that does not exist",
        ));
    }
    let Some(node) = doc.node_mut(id) else {
        return Err(CommandError::on_node(
            CommandErrorCode::NodeNotFound,
            id,
            "the target node does not exist",
        ));
    };
    let old_props = Box::new(std::mem::replace(&mut node.props, props.clone()));

    Ok(Effect::NodePropsChanged {
        node: id,
        old_props,
    })
}

/// Installs an exact sibling order, replacing whatever the container held.
///
/// The set of members must be unchanged — this restores a known order, it does
/// not add or drop nodes. Checking that is what stops a stale inverse from
/// quietly rearranging a container that has since gained or lost a sibling.
fn apply_reorder_to(
    doc: &mut Document,
    parent: Option<NodeId>,
    page: PageId,
    order: &[NodeId],
) -> Result<Effect, CommandError> {
    let (current, owner_page) = match parent {
        Some(parent) => {
            require_container(doc, parent)?;
            let Some(container) = doc.node(parent) else {
                return Err(CommandError::on_node(
                    CommandErrorCode::NodeNotFound,
                    parent,
                    "the container does not exist",
                ));
            };
            let Some(owner_page) = doc.page_of(parent) else {
                return Err(CommandError::on_node(
                    CommandErrorCode::PreconditionFailed,
                    parent,
                    "the container is not reachable from any page",
                ));
            };
            (container.children().to_vec(), owner_page)
        }
        None => {
            require_page(doc, page)?;
            let roots = doc
                .page(page)
                .map(|p| p.roots().to_vec())
                .unwrap_or_default();
            (roots, page)
        }
    };

    if owner_page != page {
        return Err(CommandError::on_page(
            CommandErrorCode::PreconditionFailed,
            page,
            "the page does not own the given container",
        ));
    }

    if order.len() != current.len() {
        return Err(CommandError::general(
            CommandErrorCode::PreconditionFailed,
            "the requested order has a different number of members than the container",
        ));
    }
    for member in order {
        if !current.contains(member) {
            return Err(CommandError::on_node(
                CommandErrorCode::NodeNotFound,
                *member,
                "the requested order names a node that is not in the container",
            ));
        }
    }
    if order == current.as_slice() {
        return Err(CommandError::general(
            CommandErrorCode::PreconditionFailed,
            "the requested order is identical to the current order",
        ));
    }

    let applied = match parent {
        Some(parent) => doc.set_child_order(parent, order),
        None => doc.set_root_order(page, order),
    };
    if !applied {
        return Err(CommandError::general(
            CommandErrorCode::PreconditionFailed,
            "the container no longer holds exactly the requested members",
        ));
    }

    Ok(Effect::NodeReordered {
        parent,
        page,
        old_order: current,
    })
}

/// Removes a page, refusing while it still holds nodes.
///
/// Cascading here would make an undo able to discard content that no recorded
/// effect mentions. Refusing instead forces the inverse to be built from the
/// recorded effects, which always removes nodes before their page.
fn apply_remove_page(doc: &mut Document, id: PageId) -> Result<Effect, CommandError> {
    require_page(doc, id)?;
    let Some(page) = doc.page(id) else {
        return Err(CommandError::on_page(
            CommandErrorCode::PageNotFound,
            id,
            "the page does not exist",
        ));
    };
    if !page.roots().is_empty() {
        return Err(CommandError::on_page(
            CommandErrorCode::PreconditionFailed,
            id,
            "the page still holds nodes, so it cannot be removed",
        ));
    }

    // `PageCreated` is the effect a page always has; the removal is recorded by
    // reporting the page as changed, which `diff` already derives.
    doc.remove_page(id);

    Ok(Effect::PageRemoved { page: id })
}

/// Computes the change set by comparing the document before and after a batch.
fn diff(
    before: &Document,
    after: &Document,
    effects: Vec<Effect>,
    created_pages: Vec<PageId>,
    new_revision: Revision,
) -> ChangeSet {
    let mut added_nodes = BTreeSet::new();
    let mut removed_nodes = BTreeSet::new();
    let mut changed_nodes = BTreeSet::new();
    let mut moved_nodes = BTreeSet::new();

    for id in before.nodes().map(|n| n.id) {
        if !after.contains_node(id) {
            removed_nodes.insert(id);
        }
    }
    for node in after.nodes() {
        match before.node(node.id) {
            None => {
                added_nodes.insert(node.id);
            }
            Some(previous) => {
                if !node.same_content(previous) {
                    changed_nodes.insert(node.id);
                } else if node.children() != previous.children() {
                    moved_nodes.insert(node.id);
                }
            }
        }
    }

    let mut added_pages = BTreeSet::new();
    let mut changed_pages = BTreeSet::new();
    let mut affected_pages_removed = BTreeSet::new();
    for page in after.pages() {
        match before.page(page.id) {
            None => {
                added_pages.insert(page.id);
            }
            Some(previous) => {
                if previous.name != page.name || previous.roots() != page.roots() {
                    changed_pages.insert(page.id);
                }
            }
        }
    }
    for page in before.pages() {
        if after.page(page.id).is_none() {
            changed_pages.insert(page.id);
            affected_pages_removed.insert(page.id);
        }
    }
    changed_pages.extend(created_pages);
    changed_pages.extend(effects.iter().filter_map(|effect| match effect {
        Effect::NodeReordered { page, .. } => Some(*page),
        _ => None,
    }));
    // A renamed node changes its page's content even though the page's own
    // root list is unchanged.
    for node in &changed_nodes {
        if let Some(page) = after.page_of(*node) {
            changed_pages.insert(page);
        }
    }

    // A page must be repainted whenever it gains or loses a node, which is
    // broader than a node merely changing value.
    let mut affected_pages = changed_pages.clone();
    affected_pages.extend(affected_pages_removed);
    for node in added_nodes.iter().chain(moved_nodes.iter()) {
        if let Some(page) = after.page_of(*node) {
            affected_pages.insert(page);
        }
    }
    for node in removed_nodes.iter().chain(moved_nodes.iter()) {
        if let Some(page) = before.page_of(*node) {
            affected_pages.insert(page);
        }
    }
    for effect in &effects {
        match effect {
            Effect::NodeRemoved { old_page, .. }
            | Effect::NodeMoved { old_page, .. }
            | Effect::NodeReordered { page: old_page, .. }
            | Effect::PageRemoved { page: old_page } => {
                affected_pages.insert(*old_page);
            }
            _ => {}
        }
    }

    let mut added_assets: BTreeSet<AssetId> = BTreeSet::new();
    for asset in after.assets() {
        if before.asset(asset.id).is_none() {
            added_assets.insert(asset.id);
        }
    }

    ChangeSet {
        previous_revision: before.revision(),
        new_revision,
        added_nodes,
        removed_nodes,
        changed_nodes,
        moved_nodes,
        added_pages,
        changed_pages,
        added_assets,
        affected_pages,
        effects,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validation::{ResourceLimits, validate};

    fn create_page(doc: &mut Document, name: &str) -> PageId {
        let page = PageId::new();
        let revision = doc.revision();
        apply_batch(
            doc,
            revision,
            &[Command::CreatePage {
                id: page,
                name: name.into(),
            }],
        )
        .expect("page creation succeeds");
        page
    }

    fn create_node(
        doc: &mut Document,
        kind: NodeKind,
        name: &str,
        parent: NodePlacement,
    ) -> NodeId {
        let node = NodeId::new();
        let revision = doc.revision();
        apply_batch(
            doc,
            revision,
            &[Command::CreateNode {
                id: node,
                kind,
                name: Some(name.into()),
                parent,
            }],
        )
        .expect("node creation succeeds");
        node
    }

    fn fixture() -> (Document, PageId, NodeId) {
        let mut doc = Document::new();
        let page = create_page(&mut doc, "Page 1");
        let frame = create_node(
            &mut doc,
            NodeKind::Frame,
            "Frame",
            NodePlacement::PageRoot {
                page,
                position: Position::Last,
            },
        );
        (doc, page, frame)
    }

    #[test]
    fn a_batch_is_rejected_when_the_expected_revision_is_stale() {
        let mut doc = Document::new();
        let error = apply_batch(
            &mut doc,
            Revision::from_raw(7),
            &[Command::CreatePage {
                id: PageId::new(),
                name: "Page".into(),
            }],
        )
        .expect_err("a stale revision is rejected");

        assert!(matches!(error, BatchError::RevisionConflict { .. }));
        if let BatchError::RevisionConflict { expected, actual } = error {
            assert_eq!(expected, Revision::from_raw(7));
            assert_eq!(actual, Revision::INITIAL);
        }
        assert_eq!(doc.revision(), Revision::INITIAL);
        assert!(doc.pages().is_empty());
    }

    #[test]
    fn the_revision_advances_once_per_batch_not_once_per_command() {
        let (mut doc, page, _) = fixture();
        assert_eq!(doc.revision().as_u64(), 2);

        let revision = doc.revision();
        let commit = apply_batch(
            &mut doc,
            revision,
            &[
                Command::CreateNode {
                    id: NodeId::new(),
                    kind: NodeKind::Shape,
                    name: Some("A".into()),
                    parent: NodePlacement::PageRoot {
                        page,
                        position: Position::Last,
                    },
                },
                Command::CreateNode {
                    id: NodeId::new(),
                    kind: NodeKind::Shape,
                    name: Some("B".into()),
                    parent: NodePlacement::PageRoot {
                        page,
                        position: Position::Last,
                    },
                },
                Command::CreateNode {
                    id: NodeId::new(),
                    kind: NodeKind::Text,
                    name: Some("C".into()),
                    parent: NodePlacement::PageRoot {
                        page,
                        position: Position::Last,
                    },
                },
            ],
        )
        .expect("the batch succeeds");

        assert_eq!(commit.revision().as_u64(), revision.as_u64() + 1);
        assert_eq!(doc.revision().as_u64(), 3);
        assert_eq!(doc.page(page).expect("page exists").roots().len(), 4);
    }

    #[test]
    fn a_failed_batch_leaves_no_trace_at_all() {
        let (mut doc, page, frame) = fixture();
        let revision_before = doc.revision();
        let roots_before = doc.page(page).expect("page exists").roots().to_vec();
        let nodes_before = doc.node_count();

        let missing = NodeId::new();
        let error = apply_batch(
            &mut doc,
            revision_before,
            &[
                Command::CreateNode {
                    id: NodeId::new(),
                    kind: NodeKind::Shape,
                    name: Some("Doomed".into()),
                    parent: NodePlacement::Child {
                        parent: frame,
                        position: Position::Last,
                    },
                },
                Command::RenameNode {
                    id: missing,
                    name: Some("Nope".into()),
                },
            ],
        )
        .expect_err("the second command fails");

        assert!(matches!(error, BatchError::Command(_)));
        assert_eq!(doc.revision(), revision_before, "revision must not move");
        assert_eq!(doc.node_count(), nodes_before, "no node may be added");
        assert_eq!(
            doc.page(page).expect("page exists").roots(),
            roots_before,
            "the page root list must be untouched"
        );
        assert!(
            doc.node(frame).expect("frame exists").children().is_empty(),
            "the partially-attached child must not survive"
        );
    }

    #[test]
    fn the_first_command_failing_also_leaves_no_trace() {
        let mut doc = Document::new();
        let page = create_page(&mut doc, "Page 1");
        let revision_before = doc.revision();

        let error = apply_batch(
            &mut doc,
            revision_before,
            &[
                Command::CreateNode {
                    id: NodeId::new(),
                    kind: NodeKind::Shape,
                    name: None,
                    parent: NodePlacement::PageRoot {
                        page: PageId::new(),
                        position: Position::Last,
                    },
                },
                Command::CreatePage {
                    id: PageId::new(),
                    name: "Page 2".into(),
                },
            ],
        )
        .expect_err("the first command targets a missing page");

        match error {
            BatchError::Command(error) => {
                assert_eq!(error.code, CommandErrorCode::PageNotFound);
            }
            other => panic!("unexpected error: {other:?}"),
        }
        assert_eq!(doc.revision(), revision_before);
        assert_eq!(doc.pages().len(), 1);
        assert!(doc.page(page).expect("page exists").roots().is_empty());
    }

    #[test]
    fn an_empty_batch_still_advances_the_revision_exactly_once() {
        let mut doc = Document::new();
        let commit =
            apply_batch(&mut doc, Revision::INITIAL, &[]).expect("an empty batch is valid");
        assert_eq!(commit.revision().as_u64(), 1);
        assert_eq!(doc.revision().as_u64(), 1);
        assert!(commit.change_set().is_empty());
    }

    #[test]
    fn a_duplicate_identity_is_rejected() {
        let (mut doc, page, frame) = fixture();
        let revision = doc.revision();
        let error = apply_batch(
            &mut doc,
            revision,
            &[Command::CreateNode {
                id: frame,
                kind: NodeKind::Shape,
                name: None,
                parent: NodePlacement::PageRoot {
                    page,
                    position: Position::Last,
                },
            }],
        )
        .expect_err("the identity is already taken");

        match error {
            BatchError::Command(error) => {
                assert_eq!(error.code, CommandErrorCode::NodeAlreadyExists);
            }
            other => panic!("unexpected error: {other:?}"),
        }
        assert_eq!(doc.revision(), revision);
    }

    #[test]
    fn a_move_preserves_the_node_identity_and_reports_the_old_position() {
        let (mut doc, page, frame) = fixture();
        let child = create_node(
            &mut doc,
            NodeKind::Shape,
            "Child",
            NodePlacement::Child {
                parent: frame,
                position: Position::Last,
            },
        );

        let revision = doc.revision();
        let commit = apply_batch(
            &mut doc,
            revision,
            &[Command::MoveNode {
                id: child,
                parent: NodePlacement::PageRoot {
                    page,
                    position: Position::First,
                },
            }],
        )
        .expect("the move succeeds");

        assert_eq!(doc.parent_of(child), None);
        assert_eq!(
            doc.page(page).expect("page exists").roots().first(),
            Some(&child)
        );
        assert!(doc.node(frame).expect("frame exists").children().is_empty());
        assert!(commit.effects().iter().any(|effect| matches!(
            effect,
            Effect::NodeMoved {
                node,
                old_parent: Some(parent),
                old_index: 0,
                ..
            } if *node == child && *parent == frame
        )));
    }

    #[test]
    fn a_move_into_ones_own_subtree_is_rejected_when_asked_for() {
        let (mut doc, page, outer) = fixture();
        let inner = create_node(
            &mut doc,
            NodeKind::Frame,
            "Inner",
            NodePlacement::Child {
                parent: outer,
                position: Position::Last,
            },
        );

        let revision = doc.revision();
        let error = apply_batch(
            &mut doc,
            revision,
            &[Command::MoveNode {
                id: outer,
                parent: NodePlacement::Child {
                    parent: inner,
                    position: Position::Last,
                },
            }],
        )
        .expect_err("an ancestor cannot move into its own subtree");

        match error {
            BatchError::Command(error) => {
                assert_eq!(error.code, CommandErrorCode::WouldCycle);
            }
            other => panic!("unexpected error: {other:?}"),
        }
        assert_eq!(doc.revision(), revision);
        assert_eq!(doc.parent_of(inner), Some(outer));
        let _ = page;
    }

    #[test]
    fn a_move_cannot_make_a_node_its_own_parent() {
        let (mut doc, _, frame) = fixture();
        let revision = doc.revision();
        let error = apply_batch(
            &mut doc,
            revision,
            &[Command::MoveNode {
                id: frame,
                parent: NodePlacement::Child {
                    parent: frame,
                    position: Position::Last,
                },
            }],
        )
        .expect_err("a node cannot parent itself");

        match error {
            BatchError::Command(error) => {
                assert_eq!(error.code, CommandErrorCode::WouldCycle);
            }
            other => panic!("unexpected error: {other:?}"),
        }
        assert_eq!(doc.revision(), revision);
    }

    #[test]
    fn deleting_a_subtree_reports_every_removed_member() {
        let (mut doc, page, frame) = fixture();
        let child = create_node(
            &mut doc,
            NodeKind::Shape,
            "Child",
            NodePlacement::Child {
                parent: frame,
                position: Position::Last,
            },
        );
        let grandchild = create_node(
            &mut doc,
            NodeKind::Frame,
            "Grandchild",
            NodePlacement::Child {
                parent: frame,
                position: Position::Last,
            },
        );
        let _ = child;

        let revision = doc.revision();
        let commit = apply_batch(&mut doc, revision, &[Command::DeleteNode { id: frame }])
            .expect("the delete succeeds");

        assert!(doc.node(frame).is_none());
        assert!(doc.node(grandchild).is_none());
        let removed: BTreeSet<NodeId> = commit.change_set().removed_nodes().collect();
        assert!(removed.contains(&frame));
        assert!(removed.contains(&grandchild));
        assert!(commit.change_set().affected_pages().contains(&page));
        assert!(
            validate(&doc, ResourceLimits::UNLIMITED).is_ok(),
            "the document must stay valid"
        );
    }

    #[test]
    fn a_cross_page_move_marks_both_pages_affected() {
        let (mut doc, first, _) = fixture();
        let second = create_page(&mut doc, "Page 2");
        let node = create_node(
            &mut doc,
            NodeKind::Shape,
            "Traveller",
            NodePlacement::PageRoot {
                page: first,
                position: Position::Last,
            },
        );

        let revision = doc.revision();
        let commit = apply_batch(
            &mut doc,
            revision,
            &[Command::MoveNode {
                id: node,
                parent: NodePlacement::PageRoot {
                    page: second,
                    position: Position::Last,
                },
            }],
        )
        .expect("the cross-page move succeeds");

        let affected = commit.change_set().affected_pages();
        assert!(affected.contains(&first), "the page it left");
        assert!(affected.contains(&second), "the page it arrived on");
        assert_eq!(doc.page_of(node), Some(second));
    }

    #[test]
    fn a_rename_is_reported_as_a_change_not_a_move() {
        let (mut doc, page, frame) = fixture();
        let revision = doc.revision();
        let commit = apply_batch(
            &mut doc,
            revision,
            &[Command::RenameNode {
                id: frame,
                name: Some("Renamed".into()),
            }],
        )
        .expect("the rename succeeds");

        assert!(commit.change_set().changed_nodes().any(|n| n == frame));
        assert!(!commit.change_set().moved_nodes().any(|n| n == frame));
        assert!(commit.change_set().affected_pages().contains(&page));
        assert_eq!(doc.node(frame).expect("frame exists").name(), "Renamed");
        assert!(matches!(
            commit.effects(),
            [Effect::NodeRenamed { node, old_name }]
                if *node == frame && old_name.as_deref() == Some("Frame")
        ));
    }

    #[test]
    fn reordering_a_root_changes_only_the_order() {
        let (mut doc, page, _) = fixture();
        let a = create_node(
            &mut doc,
            NodeKind::Shape,
            "A",
            NodePlacement::PageRoot {
                page,
                position: Position::Last,
            },
        );
        let b = create_node(
            &mut doc,
            NodeKind::Shape,
            "B",
            NodePlacement::PageRoot {
                page,
                position: Position::Last,
            },
        );

        let before = doc.page(page).expect("page exists").roots().to_vec();
        assert_eq!(before.last(), Some(&b));

        let revision = doc.revision();
        let commit = apply_batch(
            &mut doc,
            revision,
            &[Command::ReorderNode {
                id: b,
                position: Position::Before(a),
            }],
        )
        .expect("the reorder succeeds");

        let after = doc.page(page).expect("page exists").roots().to_vec();
        assert_ne!(after, before);
        assert!(commit.change_set().affected_pages().contains(&page));
        assert!(commit.effects().iter().any(|effect| matches!(
            effect,
            Effect::NodeReordered {
                parent: None,
                old_order,
                ..
            } if *old_order == before
        )));
        assert!(validate(&doc, ResourceLimits::UNLIMITED).is_ok());
    }

    #[test]
    fn a_reorder_that_changes_nothing_is_rejected() {
        let (mut doc, page, _) = fixture();
        let a = create_node(
            &mut doc,
            NodeKind::Shape,
            "A",
            NodePlacement::PageRoot {
                page,
                position: Position::Last,
            },
        );
        let b = create_node(
            &mut doc,
            NodeKind::Shape,
            "B",
            NodePlacement::PageRoot {
                page,
                position: Position::Last,
            },
        );
        let c = create_node(
            &mut doc,
            NodeKind::Shape,
            "C",
            NodePlacement::PageRoot {
                page,
                position: Position::Last,
            },
        );

        // `c` is already last, so moving it to Last is a genuine no-op.
        let revision = doc.revision();
        let error = apply_batch(
            &mut doc,
            revision,
            &[Command::ReorderNode {
                id: c,
                position: Position::Last,
            }],
        )
        .expect_err("the order is already correct");

        match error {
            BatchError::Command(error) => {
                assert_eq!(error.code, CommandErrorCode::PreconditionFailed);
            }
            other => panic!("unexpected error: {other:?}"),
        }
        assert_eq!(doc.revision(), revision);
        let _ = (a, b);
    }

    #[test]
    fn a_reorder_that_moves_a_node_earlier_is_accepted() {
        let (mut doc, page, _) = fixture();
        let a = create_node(
            &mut doc,
            NodeKind::Shape,
            "A",
            NodePlacement::PageRoot {
                page,
                position: Position::Last,
            },
        );
        let c = create_node(
            &mut doc,
            NodeKind::Shape,
            "C",
            NodePlacement::PageRoot {
                page,
                position: Position::Last,
            },
        );

        let before = doc.page(page).expect("page exists").roots().to_vec();
        assert_eq!(before.last(), Some(&c));

        let revision = doc.revision();
        apply_batch(
            &mut doc,
            revision,
            &[Command::ReorderNode {
                id: c,
                position: Position::First,
            }],
        )
        .expect("moving a node earlier is a real change");

        let after = doc.page(page).expect("page exists").roots().to_vec();
        assert_eq!(after.first(), Some(&c));
        assert_eq!(after.len(), before.len());
        assert!(after.contains(&a));
    }

    #[test]
    fn a_leaf_cannot_own_children() {
        let (mut doc, page, _) = fixture();
        let shape = create_node(
            &mut doc,
            NodeKind::Shape,
            "Leaf",
            NodePlacement::PageRoot {
                page,
                position: Position::Last,
            },
        );

        let revision = doc.revision();
        let error = apply_batch(
            &mut doc,
            revision,
            &[Command::CreateNode {
                id: NodeId::new(),
                kind: NodeKind::Shape,
                name: None,
                parent: NodePlacement::Child {
                    parent: shape,
                    position: Position::Last,
                },
            }],
        )
        .expect_err("a shape cannot own children");

        match error {
            BatchError::Command(error) => {
                assert_eq!(error.code, CommandErrorCode::KindCannotOwnChildren);
            }
            other => panic!("unexpected error: {other:?}"),
        }
        assert_eq!(doc.revision(), revision);
    }

    #[test]
    fn the_change_set_cannot_drift_from_the_committed_document() {
        let (mut doc, page, frame) = fixture();
        let revision = doc.revision();
        let commit = apply_batch(
            &mut doc,
            revision,
            &[
                Command::CreateNode {
                    id: NodeId::new(),
                    kind: NodeKind::Text,
                    name: Some("Text".into()),
                    parent: NodePlacement::Child {
                        parent: frame,
                        position: Position::First,
                    },
                },
                Command::RenameNode {
                    id: frame,
                    name: Some("Renamed".into()),
                },
            ],
        )
        .expect("the batch succeeds");

        let change_set = commit.change_set();
        for node in change_set.added_nodes() {
            assert!(doc.node(node).is_some(), "added node must exist");
        }
        for node in change_set.changed_nodes() {
            assert!(doc.node(node).is_some(), "changed node must exist");
        }
        for node in change_set.removed_nodes() {
            assert!(doc.node(node).is_none(), "removed node must be gone");
        }
        assert!(change_set.affected_pages().contains(&page));
        assert!(validate(&doc, ResourceLimits::UNLIMITED).is_ok());
    }

    #[test]
    fn a_sequence_of_batches_keeps_the_document_valid() {
        let (mut doc, page, frame) = fixture();
        let mut last = frame;

        for index in 0..5 {
            let child = NodeId::new();
            let revision = doc.revision();
            apply_batch(
                &mut doc,
                revision,
                &[Command::CreateNode {
                    id: child,
                    kind: if index % 2 == 0 {
                        NodeKind::Frame
                    } else {
                        NodeKind::Text
                    },
                    name: Some(format!("N{index}")),
                    parent: NodePlacement::Child {
                        parent: last,
                        position: Position::Last,
                    },
                }],
            )
            .expect("each batch succeeds");
            assert!(
                validate(&doc, ResourceLimits::UNLIMITED).is_ok(),
                "valid after batch {index}"
            );
            if index % 2 == 0 {
                last = child;
            }
        }

        assert_eq!(doc.revision().as_u64(), doc.revision().as_u64());
        assert_eq!(doc.page(page).expect("page exists").roots().len(), 1);
    }

    #[test]
    fn creating_an_asset_registers_it_and_reports_the_change() {
        let mut doc = Document::new();
        let asset = AssetId::new();

        let commit = apply_batch_at_current(
            &mut doc,
            &[Command::CreateAsset {
                id: asset,
                name: "logo".into(),
            }],
        )
        .expect("asset creation succeeds");

        assert_eq!(doc.asset(asset).expect("asset is stored").name, "logo");
        assert_eq!(commit.effects(), [Effect::AssetCreated { asset }]);
        assert_eq!(
            commit.change_set().added_assets().collect::<Vec<_>>(),
            vec![asset]
        );
    }

    #[test]
    fn a_duplicate_asset_identity_is_refused() {
        let mut doc = Document::new();
        let asset = AssetId::new();
        apply_batch_at_current(
            &mut doc,
            &[Command::CreateAsset {
                id: asset,
                name: "a".into(),
            }],
        )
        .unwrap();

        let revision = doc.revision();
        let error = apply_batch_at_current(
            &mut doc,
            &[Command::CreateAsset {
                id: asset,
                name: "b".into(),
            }],
        )
        .expect_err("the identity is already taken");

        match error {
            BatchError::Command(error) => {
                assert_eq!(error.code, CommandErrorCode::IdCollision);
                assert_eq!(error.asset, Some(asset));
            }
            other => panic!("unexpected error: {other:?}"),
        }
        assert_eq!(doc.revision(), revision);
        assert_eq!(doc.asset(asset).expect("asset remains").name, "a");
    }
}
