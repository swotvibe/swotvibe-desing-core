//! Undo and redo over committed batches.
//!
//! Ownership: the session undo/redo timeline and inverse-command generation.
//! Not owned: persisting history, and the write path itself (that is
//! [`crate::transaction`]).
//!
//! History records the *effect* a batch actually had, not the commands the
//! caller asked for, and builds the inverse from that record (§6.4). A request
//! carries intent — `Position::Last`, an index that had to be clamped, a name
//! that was already set — and intent is not a sound basis for an inverse.
//! Effects describe the state that really existed, so replaying their inverse
//! restores that state exactly.
//!
//! Undo and redo go back through [`apply_batch`], so they pass the same
//! validation as any other write. A later edit that invalidated an inverse's
//! precondition therefore surfaces as an ordinary refusal rather than as silent
//! corruption.
//!
//! History lives in the session, not in the document payload. Restoring a
//! timeline across a save/open cycle is a separate feature and is not attempted
//! here (§6.4).

use crate::commands::{Command, CommandError, CommandErrorCode, NodePlacement, Position};
use crate::ids::{NodeId, PageId};
use crate::model::{Document, Node, Page, Revision};
use crate::props::NodeProps;
use crate::transaction::{BatchError, Commit, Effect, apply_batch};

#[cfg(test)]
use crate::transaction::apply_batch_at_current;

/// A batch as the history sees it: the commands needed to undo it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryEntry {
    revision: Revision,
    inverse: Vec<Command>,
    /// Whether this entry is already the head of a coalescing run.
    ///
    /// Only a run head may absorb another merged batch, so the first merged
    /// batch always starts a new step instead of quietly folding into whatever
    /// its predecessor happened to be.
    coalesced: bool,
}

impl HistoryEntry {
    /// The revision the batch left the document at.
    #[must_use]
    pub const fn revision(&self) -> Revision {
        self.revision
    }

    /// The commands that undo the batch, in application order.
    #[must_use]
    pub fn inverse(&self) -> &[Command] {
        &self.inverse
    }

    /// Whether this entry would change nothing when replayed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inverse.is_empty()
    }
}

/// Why an undo or redo could not be performed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HistoryError {
    /// There is nothing to undo.
    #[error("there is nothing to undo")]
    NothingToUndo,
    /// There is nothing to redo.
    #[error("there is nothing to redo")]
    NothingToRedo,
    /// The document is not at the revision the timeline expects.
    ///
    /// This happens when the document was changed by a path that did not go
    /// through this history — another writer, or a fresh load. Applying the
    /// inverse now would rewind work this timeline never saw, so it is refused.
    #[error("history is at revision {expected}, but the document is at {actual}")]
    OutOfSync {
        /// The revision the timeline expects the document to be at.
        expected: Revision,
        /// The document's actual revision.
        actual: Revision,
    },
    /// The inverse was refused by the write path.
    ///
    /// The reason is carried verbatim so the caller can branch on its stable
    /// code; the common case is a later edit that removed a node the inverse
    /// still refers to.
    #[error(transparent)]
    Rejected(#[from] BatchError),
}

/// The default number of undoable batches a session keeps.
///
/// History is bounded because an editing session is long-lived and each entry
/// holds the commands needed to reverse a batch: an unbounded stack makes
/// memory grow with session length, not with document size. Dropping the oldest
/// step is the standard trade — a user can undo the recent past, not the whole
/// session — and it is recorded here rather than left implicit.
///
/// 256 steps is a comfortable working depth for a design tool while keeping the
/// worst-case retained command count bounded under a million-node document.
pub const DEFAULT_HISTORY_CAPACITY: usize = 256;

/// Undo/redo state for one document session.
///
/// Both stacks hold entries in application order, so `undo` pops from the end
/// of `done` and `redo` pops from the end of `undone`. Committing a batch
/// clears the redo stack, because that branch of the timeline is no longer
/// reachable.
///
/// The `done` stack is bounded by [`History::capacity`]; once full, the oldest
/// entry is discarded. The `undone` stack cannot exceed `done`'s depth, since
/// every redoable entry was pushed from it.
#[derive(Debug, Clone)]
pub struct History {
    done: Vec<HistoryEntry>,
    undone: Vec<HistoryEntry>,
    capacity: Option<usize>,
}

impl Default for History {
    fn default() -> Self {
        Self::new()
    }
}

impl History {
    /// Creates an empty timeline bounded by [`DEFAULT_HISTORY_CAPACITY`].
    #[must_use]
    pub fn new() -> Self {
        Self {
            done: Vec::new(),
            undone: Vec::new(),
            capacity: Some(DEFAULT_HISTORY_CAPACITY),
        }
    }

    /// Creates an empty timeline with no depth limit.
    ///
    /// Prefer [`History::new`] unless the caller has its own bound; an
    /// unbounded timeline grows with session length.
    #[must_use]
    pub fn unbounded() -> Self {
        Self {
            done: Vec::new(),
            undone: Vec::new(),
            capacity: None,
        }
    }

    /// Creates an empty timeline that keeps at most `capacity` undoable steps.
    ///
    /// A capacity of zero disables undo entirely, which is a valid way to
    /// record nothing. A capacity of `None` is [`History::unbounded`].
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            done: Vec::new(),
            undone: Vec::new(),
            capacity: Some(capacity),
        }
    }

    /// The maximum number of undoable steps retained, or `None` when unbounded.
    #[must_use]
    pub const fn capacity(&self) -> Option<usize> {
        self.capacity
    }

    /// Sets the retained-step limit, trimming immediately if it is lowered.
    ///
    /// Discarding the oldest steps is the only way to honor a lowered bound;
    /// the caller should expect that a subsequent undo cannot reach them.
    pub fn set_capacity(&mut self, capacity: Option<usize>) {
        self.capacity = capacity;
        self.trim();
    }

    /// Drops the oldest recorded steps until `done` is within capacity.
    ///
    /// Never touches `undone`: those steps still have to be re-applied before
    /// the timeline is coherent, and they were already counted against the
    /// bound when they were recorded.
    fn trim(&mut self) {
        if let Some(capacity) = self.capacity {
            while self.done.len() > capacity {
                self.done.remove(0);
            }
        }
    }

    /// Whether there is a batch to undo.
    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.done.is_empty()
    }

    /// Whether there is a batch to redo.
    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.undone.is_empty()
    }

    /// The number of batches that can be undone.
    #[must_use]
    pub fn undo_depth(&self) -> usize {
        self.done.len()
    }

    /// The number of batches that can be redone.
    #[must_use]
    pub fn redo_depth(&self) -> usize {
        self.undone.len()
    }

    /// The most recent undoable entry, or `None` when the timeline is empty.
    ///
    /// This is the entry that the next [`History::undo`] would apply.
    #[must_use]
    pub fn last_entry(&self) -> Option<&HistoryEntry> {
        self.done.last()
    }

    /// The most recent redoable entry, or `None` when nothing was undone.
    ///
    /// This is the entry that the next [`History::redo`] would reapply.
    #[must_use]
    pub fn next_redo_entry(&self) -> Option<&HistoryEntry> {
        self.undone.last()
    }

    /// Forgets all undo and redo state.
    ///
    /// Used when the document identity changes underneath the session, such as
    /// after opening a different file.
    pub fn clear(&mut self) {
        self.done.clear();
        self.undone.clear();
    }

    /// Applies a batch and records it as one undoable step.
    ///
    /// A user drag is a compound batch and becomes one step, not one step per
    /// command (§6.4). The batch is applied first; only a committed batch is
    /// recorded, so a rejected batch leaves the timeline untouched (§6.2).
    ///
    /// A batch that *deletes* a node is recorded, but its inverse cannot be
    /// complete: an effect does not carry the removed node's kind or its
    /// subtree. Use [`History::commit_with`] and pass [`inverse_with_subtree`]
    /// when a delete must be undoable in full.
    ///
    /// # Errors
    ///
    /// Propagates [`BatchError`] unchanged. On error the timeline is unchanged.
    pub fn commit(
        &mut self,
        doc: &mut Document,
        expected_revision: Revision,
        commands: &[Command],
    ) -> Result<Commit, BatchError> {
        self.commit_with(doc, expected_revision, commands, |_, commit| {
            inverse_entry(commit)
        })
    }

    /// Applies a batch and records it using a caller-supplied inverse builder.
    ///
    /// `build` receives the document as it was before the batch and the
    /// committed batch. This is the general entry point; pass
    /// [`inverse_with_subtree`] to make a delete undoable, because only the
    /// pre-batch document knows what the deleted subtree looked like.
    ///
    /// # Errors
    ///
    /// Propagates [`BatchError`] unchanged. On error the timeline is unchanged.
    pub fn commit_with(
        &mut self,
        doc: &mut Document,
        expected_revision: Revision,
        commands: &[Command],
        build: impl FnOnce(&Document, &Commit) -> HistoryEntry,
    ) -> Result<Commit, BatchError> {
        let before = doc.clone();
        let commit = apply_batch(doc, expected_revision, commands)?;
        self.undone.clear();
        self.done.push(build(&before, &commit));
        self.trim();
        Ok(commit)
    }

    /// Commits a batch and coalesces it into the previous step.
    ///
    /// This collapses a sequence of related writes — repeated nudges of the
    /// same node, say — into one user-visible undo. The batch is applied and
    /// the timeline gains no new step; `merge` then rebuilds the top entry so a
    /// single undo still restores the state from before the *first* coalesced
    /// batch.
    ///
    /// `merge` receives the document as it was before this batch and the batch
    /// itself, and returns an entry describing the combined step.
    ///
    /// # Errors
    ///
    /// Propagates [`BatchError`] unchanged. On error the timeline is unchanged.
    pub fn commit_merged(
        &mut self,
        doc: &mut Document,
        expected_revision: Revision,
        commands: &[Command],
        merge: impl FnOnce(&Document, &Commit) -> HistoryEntry,
    ) -> Result<Commit, BatchError> {
        let before = doc.clone();
        let commit = apply_batch(doc, expected_revision, commands)?;
        self.undone.clear();
        let fresh = merge(&before, &commit);
        match self.done.pop() {
            Some(existing) if existing.coalesced => {
                self.done.push(merge_entries(existing, fresh));
            }
            Some(existing) => {
                self.done.push(existing);
                self.done.push(HistoryEntry {
                    coalesced: true,
                    ..fresh
                });
            }
            None => self.done.push(HistoryEntry {
                coalesced: true,
                ..fresh
            }),
        }
        self.trim();
        Ok(commit)
    }

    /// Applies a batch against the document's current revision.
    ///
    /// A convenience over [`History::commit`] for callers that are the sole
    /// writer: it reads the revision for them, so they cannot pass a stale one
    /// by accident. A caller sharing the document with another writer should
    /// use [`History::commit`] with a revision it captured deliberately.
    ///
    /// # Errors
    ///
    /// Propagates [`BatchError`] unchanged.
    pub fn commit_at_current(
        &mut self,
        doc: &mut Document,
        commands: &[Command],
    ) -> Result<Commit, BatchError> {
        let revision = doc.revision();
        self.commit(doc, revision, commands)
    }

    /// Records a batch against the current revision with a custom inverse.
    ///
    /// The sole-writer counterpart to [`History::commit_with`].
    ///
    /// # Errors
    ///
    /// Propagates [`BatchError`] unchanged.
    pub fn commit_with_at_current(
        &mut self,
        doc: &mut Document,
        commands: &[Command],
        build: impl FnOnce(&Document, &Commit) -> HistoryEntry,
    ) -> Result<Commit, BatchError> {
        let revision = doc.revision();
        self.commit_with(doc, revision, commands, build)
    }

    /// Coalesces a batch into the previous step, against the current revision.
    /// The counterpart to [`History::commit_merged`] for a sole writer.
    ///
    /// # Errors
    ///
    /// Propagates [`BatchError`] unchanged.
    pub fn commit_merged_at_current(
        &mut self,
        doc: &mut Document,
        commands: &[Command],
        merge: impl FnOnce(&Document, &Commit) -> HistoryEntry,
    ) -> Result<Commit, BatchError> {
        let revision = doc.revision();
        self.commit_merged(doc, revision, commands, merge)
    }

    /// Undoes the most recent batch.
    ///
    /// The inverse runs through [`apply_batch`], so the document advances to a
    /// *new* revision rather than returning to the old one. Revisions are
    /// monotonic and never reused: undo is an edit forward in time that
    /// restores older content.
    ///
    /// # Errors
    ///
    /// [`HistoryError::NothingToUndo`] when the timeline is empty,
    /// [`HistoryError::OutOfSync`] when the document is not at the revision
    /// this entry produced, or [`HistoryError::Rejected`] when the write path
    /// refused the inverse. A refused entry stays on the undo stack, so it
    /// becomes available again once the blocking edit is itself undone.
    pub fn undo(&mut self, doc: &mut Document) -> Result<Commit, HistoryError> {
        let entry = self.done.pop().ok_or(HistoryError::NothingToUndo)?;
        let before = doc.clone();
        match replay(doc, &entry) {
            Ok(commit) => {
                // The redo entry describes how to get forward again, which is
                // the inverse of the inverse just applied. It is built from the
                // pre-undo document because a `NodeRemoved` effect does not
                // carry the removed kind or subtree: only that document can
                // rebuild them.
                self.undone.push(inverse_with_subtree(&before, &commit));
                // Undo is an edit forward, so the exposed step must now be
                // validated against the revision this undo produced.
                if let Some(head) = self.done.last_mut() {
                    head.revision = commit.revision();
                }
                Ok(commit)
            }
            Err(error) => {
                self.done.push(entry);
                Err(error)
            }
        }
    }

    /// Redoes the most recently undone batch.
    ///
    /// # Errors
    ///
    /// The same conditions as [`History::undo`], with
    /// [`HistoryError::NothingToRedo`] when there is nothing to redo.
    pub fn redo(&mut self, doc: &mut Document) -> Result<Commit, HistoryError> {
        let Some(entry) = self.undone.pop() else {
            return Err(HistoryError::NothingToRedo);
        };
        let before = doc.clone();
        match replay(doc, &entry) {
            Ok(commit) => {
                self.done.push(inverse_with_subtree(&before, &commit));
                // The exposed redo step must be validated against the revision
                // this redo produced, not the one it originally recorded.
                if let Some(head) = self.undone.last_mut() {
                    head.revision = commit.revision();
                }
                Ok(commit)
            }
            Err(error) => {
                self.undone.push(entry);
                Err(error)
            }
        }
    }
}

/// Applies one entry, refusing when the document has moved on.
fn replay(doc: &mut Document, entry: &HistoryEntry) -> Result<Commit, HistoryError> {
    let actual = doc.revision();
    if actual != entry.revision() {
        return Err(HistoryError::OutOfSync {
            expected: entry.revision(),
            actual,
        });
    }
    apply_batch(doc, actual, &entry.inverse).map_err(HistoryError::Rejected)
}

/// Builds the history entry that undoes `commit`.
#[must_use]
pub fn inverse_entry(commit: &Commit) -> HistoryEntry {
    HistoryEntry {
        revision: commit.revision(),
        inverse: inverse_commands(commit.effects()),
        coalesced: false,
    }
}

/// Combines an older entry with a newer one into a single undoable step.
///
/// Undoing applies the newer inverse first, so the older entry's commands come
/// second. The newer revision wins because it describes the later state.
fn merge_entries(older: HistoryEntry, newer: HistoryEntry) -> HistoryEntry {
    let mut inverse = newer.inverse;
    inverse.extend(older.inverse);
    HistoryEntry {
        revision: newer.revision,
        inverse,
        coalesced: true,
    }
}

/// Turns recorded effects into the commands that undo them.
///
/// Effects are inverted in reverse application order, which is what makes a
/// nested batch undo as a unit: a child's inverse must run before its parent's,
/// while the child is still reachable.
fn inverse_commands(effects: &[Effect]) -> Vec<Command> {
    let commands: Vec<Command> = effects.iter().rev().filter_map(effect_inverse).collect();
    drop_redundant_deletes(commands)
}

/// Drops deletes whose node an earlier delete already re-removes.
///
/// A `DeleteNode` inverse removes the whole subtree, so an inverse built from
/// records that mention both a frame and a child would try to delete the child
/// after the frame had already taken it. Keeping only the outermost delete per
/// node preserves the order the effects recorded.
fn drop_redundant_deletes(commands: Vec<Command>) -> Vec<Command> {
    let mut seen: Vec<NodeId> = Vec::new();
    let mut kept: Vec<Command> = Vec::new();
    for command in commands {
        if let Command::DeleteNode { id } = &command {
            if seen.contains(id) {
                continue;
            }
            seen.push(*id);
        }
        kept.push(command);
    }
    kept
}

/// Inverts one effect. Returns `None` for effects that need the document to
/// invert, which [`inverse_with_subtree`] supplies.
fn effect_inverse(effect: &Effect) -> Option<Command> {
    match effect {
        Effect::PageCreated { page } => Some(Command::RemovePage { id: *page }),
        Effect::PageRemoved { page } => Some(Command::CreatePage {
            id: *page,
            name: String::new(),
        }),
        Effect::NodeCreated { node, .. } => Some(Command::DeleteNode { id: *node }),
        Effect::NodeMoved {
            node,
            old_parent,
            old_index,
            old_page,
        } => Some(Command::MoveNode {
            id: *node,
            parent: placement(*old_parent, *old_index, *old_page),
        }),
        Effect::NodeRenamed { node, old_name } => Some(Command::RenameNode {
            id: *node,
            name: old_name.clone(),
        }),
        Effect::NodePropsChanged { node, old_props } => Some(Command::SetNodeProps {
            id: *node,
            props: old_props.clone(),
        }),
        Effect::NodeReordered {
            parent,
            page,
            old_order,
        } => Some(Command::ReorderTo {
            parent: *parent,
            page: *page,
            order: old_order.clone(),
        }),
        // A removed node's kind and subtree are not in the effect. A plain
        // inverse cannot restore it, and guessing (say, re-deleting it) would be
        // silently wrong. `inverse_with_subtree` is the variant that can, and
        // it handles this effect before reaching here.
        Effect::NodeRemoved { .. } => None,
        // Assets are import-time, document-scoped metadata with no removal
        // command in M0. There is nothing to undo, and inventing a remove
        // command purely to satisfy the match would widen the write surface for
        // no user-visible behaviour.
        Effect::AssetCreated { .. } => None,
    }
}

/// Builds a placement pointing at a recorded index.
fn placement(parent: Option<NodeId>, index: usize, page: PageId) -> NodePlacement {
    match parent {
        Some(parent) => NodePlacement::Child {
            parent,
            position: Position::At(index),
        },
        None => NodePlacement::PageRoot {
            page,
            position: Position::At(index),
        },
    }
}

/// An entry that restores deleted subtrees, not just their roots.
///
/// [`inverse_entry`] can rebuild a page or a node identity from the effect
/// alone, but a `NodeRemoved` effect does not carry the removed node's kind or
/// its descendants. When the caller still holds the document as it was before
/// the batch, this produces an inverse that restores every descendant with its
/// kind and name intact.
///
/// # Examples
///
/// ```
/// use swotvibe_core::history::inverse_with_subtree;
/// use swotvibe_core::{
///     Command, Document, NodeId, NodeKind, NodePlacement, PageId, Position,
///     apply_batch, apply_batch_at_current,
/// };
///
/// let mut doc = Document::new();
/// let page = PageId::new();
/// let frame = NodeId::new();
/// let child = NodeId::new();
/// apply_batch_at_current(
///     &mut doc,
///     &[
///         Command::CreatePage { id: page, name: "Page".into() },
///         Command::CreateNode {
///             id: frame,
///             kind: NodeKind::Frame,
///             name: None,
///             parent: NodePlacement::PageRoot { page, position: Position::Last },
///         },
///         Command::CreateNode {
///             id: child,
///             kind: NodeKind::Text,
///             name: Some("label".into()),
///             parent: NodePlacement::Child { parent: frame, position: Position::Last },
///         },
///     ],
/// )
/// .expect("batch should apply");
///
/// let before = doc.clone();
/// let commit = apply_batch_at_current(&mut doc, &[Command::DeleteNode { id: frame }])
///     .expect("delete should apply");
///
/// let entry = inverse_with_subtree(&before, &commit);
/// let revision = doc.revision();
/// apply_batch(&mut doc, revision, entry.inverse()).expect("undo should apply");
/// assert_eq!(doc.node(child).unwrap().kind, NodeKind::Text);
/// ```
#[must_use]
pub fn inverse_with_subtree(before: &Document, commit: &Commit) -> HistoryEntry {
    let mut inverse: Vec<Command> = Vec::new();
    for effect in commit.effects().iter().rev() {
        match effect {
            Effect::NodeRemoved {
                node,
                old_parent,
                old_index,
                old_page,
            } => match before.node(*node) {
                Some(removed) => {
                    inverse.push(Command::CreateNode {
                        id: *node,
                        kind: removed.kind,
                        name: removed.name.clone(),
                        parent: placement(*old_parent, *old_index, *old_page),
                    });
                    push_props(removed, &mut inverse);
                    collect_subtree(before, *node, &mut inverse);
                }
                None => inverse.extend(effect_inverse(effect)),
            },
            other => inverse.extend(effect_inverse(other)),
        }
    }
    HistoryEntry {
        revision: commit.revision(),
        inverse,
        coalesced: false,
    }
}

/// Appends one create per descendant of `node`, in pre-order.
fn collect_subtree(doc: &Document, node: NodeId, out: &mut Vec<Command>) {
    let Some(parent) = doc.node(node) else {
        return;
    };
    for (index, child) in parent.children().iter().enumerate() {
        let Some(child_node) = doc.node(*child) else {
            continue;
        };
        out.push(Command::CreateNode {
            id: *child,
            kind: child_node.kind,
            name: child_node.name.clone(),
            parent: placement(Some(node), index, uuid_placeholder_page()),
        });
        push_props(child_node, out);
        collect_subtree(doc, *child, out);
    }
}

/// Restores a recreated node's properties when they differ from the defaults.
fn push_props(node: &Node, out: &mut Vec<Command>) {
    if node.props != NodeProps::default_for(node.kind) {
        out.push(Command::SetNodeProps {
            id: node.id,
            props: Box::new(node.props.clone()),
        });
    }
}

/// A placeholder page for a child placement.
///
/// A child placement names only its parent; the page argument exists because
/// the placement type is shared with root placements. Any value works, and
/// every caller passes the same one so the command compares equal across runs.
fn uuid_placeholder_page() -> PageId {
    PageId::from_uuid(uuid::Uuid::nil())
}

/// Checks that a rebuilt tree preserves the identities a later effect depends
/// on.
///
/// # Errors
///
/// Returns a [`CommandError`] when a node in `tree` already exists in `doc`.
pub fn check_ids_free(doc: &Document, tree: &[Node]) -> Result<(), CommandError> {
    for node in tree {
        if doc.contains_node(node.id) {
            return Err(CommandError::on_node(
                CommandErrorCode::IdCollision,
                node.id,
                "the node identity is already present in the document",
            ));
        }
    }
    Ok(())
}

/// The page a node lived on, for callers checking a restore.
#[must_use]
pub fn page_of_removed(before: &Document, node: NodeId) -> Option<PageId> {
    before.page_of(node)
}

/// A page value carrying the given identity and name, for symmetry with the
/// node restoration helpers.
#[must_use]
pub fn restored_page(id: PageId, name: impl Into<String>) -> Page {
    Page::new(id, name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{NodeKind, Page};
    use crate::validation::{ResourceLimits, validate};

    fn commit(doc: &mut Document, commands: &[Command]) {
        apply_batch_at_current(doc, commands).expect("batch should apply");
    }

    /// Creates a root node through the history, so it becomes one undoable step.
    fn node(
        doc: &mut Document,
        history: &mut History,
        kind: NodeKind,
        name: &str,
        page: PageId,
    ) -> NodeId {
        let id = NodeId::new();
        history
            .commit_at_current(
                doc,
                &[Command::CreateNode {
                    id,
                    kind,
                    name: if name.is_empty() {
                        None
                    } else {
                        Some(name.into())
                    },
                    parent: NodePlacement::PageRoot {
                        page,
                        position: Position::Last,
                    },
                }],
            )
            .expect("node creation should apply");
        id
    }

    fn history_doc() -> (Document, History, PageId) {
        let mut doc = Document::new();
        let mut history = History::new();
        let page = PageId::new();
        history
            .commit_at_current(
                &mut doc,
                &[Command::CreatePage {
                    id: page,
                    name: "Page".into(),
                }],
            )
            .expect("page creation should apply");
        (doc, history, page)
    }

    #[test]
    fn a_fresh_history_can_do_neither() {
        let history = History::new();
        assert!(!history.can_undo());
        assert!(!history.can_redo());
        assert_eq!(history.undo_depth(), 0);
        assert_eq!(history.redo_depth(), 0);
    }

    #[test]
    fn undo_on_an_empty_history_is_refused() {
        let (mut doc, mut history, _) = history_doc();
        history.clear();
        let error = history.undo(&mut doc).unwrap_err();
        assert_eq!(error, HistoryError::NothingToUndo);
        assert_eq!(doc.revision(), Revision::INITIAL.next());
    }

    #[test]
    fn redo_on_an_empty_history_is_refused() {
        let (mut doc, mut history, _) = history_doc();
        let error = history.redo(&mut doc).unwrap_err();
        assert_eq!(error, HistoryError::NothingToRedo);
    }

    #[test]
    fn committing_records_one_undoable_step_per_batch() {
        let (mut doc, mut history, page) = history_doc();
        assert_eq!(history.undo_depth(), 1);
        node(&mut doc, &mut history, NodeKind::Shape, "shape", page);
        assert_eq!(history.undo_depth(), 2);
        assert!(history.can_undo());
        assert!(!history.can_redo());
    }

    #[test]
    fn a_batch_of_many_commands_is_one_step() {
        let (mut doc, mut history, page) = history_doc();
        let a = NodeId::new();
        let b = NodeId::new();
        let c = NodeId::new();
        let root = |id| Command::CreateNode {
            id,
            kind: NodeKind::Shape,
            name: None,
            parent: NodePlacement::PageRoot {
                page,
                position: Position::Last,
            },
        };
        history
            .commit_at_current(&mut doc, &[root(a), root(b), root(c)])
            .expect("batch should apply");

        assert_eq!(history.undo_depth(), 2);
        history.undo(&mut doc).expect("undo should apply");
        assert!(!doc.contains_node(a));
        assert!(!doc.contains_node(b));
        assert!(!doc.contains_node(c));
        assert_eq!(history.undo_depth(), 1);
    }

    #[test]
    fn a_rejected_batch_leaves_the_timeline_untouched() {
        let (mut doc, mut history, _) = history_doc();
        let depth = history.undo_depth();
        let revision = doc.revision();

        let result = history.commit(
            &mut doc,
            revision,
            &[Command::CreateNode {
                id: NodeId::new(),
                kind: NodeKind::Shape,
                name: None,
                parent: NodePlacement::PageRoot {
                    page: PageId::new(),
                    position: Position::Last,
                },
            }],
        );

        assert!(result.is_err());
        assert_eq!(history.undo_depth(), depth);
        assert_eq!(doc.revision(), revision);
    }

    #[test]
    fn undoing_a_rename_restores_the_previous_name() {
        let (mut doc, mut history, page) = history_doc();
        let shape = node(&mut doc, &mut history, NodeKind::Shape, "before", page);

        history
            .commit_at_current(
                &mut doc,
                &[Command::RenameNode {
                    id: shape,
                    name: Some("after".into()),
                }],
            )
            .expect("rename should apply");
        assert_eq!(doc.node(shape).unwrap().name(), "after");

        history.undo(&mut doc).expect("undo should apply");
        assert_eq!(doc.node(shape).unwrap().name(), "before");
    }

    #[test]
    fn undoing_a_first_rename_removes_the_name_entirely() {
        let (mut doc, mut history, page) = history_doc();
        let shape = node(&mut doc, &mut history, NodeKind::Shape, "", page);

        history
            .commit_at_current(
                &mut doc,
                &[Command::RenameNode {
                    id: shape,
                    name: Some("named".into()),
                }],
            )
            .expect("rename should apply");

        history.undo(&mut doc).expect("undo should apply");
        assert_eq!(doc.node(shape).unwrap().name, None);
    }

    #[test]
    fn redo_reapplies_the_undone_change() {
        let (mut doc, mut history, page) = history_doc();
        let shape = node(&mut doc, &mut history, NodeKind::Shape, "before", page);
        history
            .commit_at_current(
                &mut doc,
                &[Command::RenameNode {
                    id: shape,
                    name: Some("after".into()),
                }],
            )
            .expect("rename should apply");

        history.undo(&mut doc).expect("undo should apply");
        assert_eq!(doc.node(shape).unwrap().name(), "before");
        assert!(history.can_redo());

        history.redo(&mut doc).expect("redo should apply");
        assert_eq!(doc.node(shape).unwrap().name(), "after");
        assert!(!history.can_redo());
        assert!(history.can_undo());
    }

    #[test]
    fn undo_restores_sibling_order_exactly() {
        let (mut doc, mut history, page) = history_doc();
        let a = node(&mut doc, &mut history, NodeKind::Shape, "a", page);
        let b = node(&mut doc, &mut history, NodeKind::Shape, "b", page);
        let c = node(&mut doc, &mut history, NodeKind::Shape, "c", page);

        history
            .commit_at_current(
                &mut doc,
                &[Command::ReorderNode {
                    id: a,
                    position: Position::Last,
                }],
            )
            .expect("reorder should apply");
        let reordered: Vec<NodeId> = doc.page(page).unwrap().roots().to_vec();
        assert_eq!(reordered, vec![b, c, a]);

        history.undo(&mut doc).expect("undo should apply");
        let restored: Vec<NodeId> = doc.page(page).unwrap().roots().to_vec();
        assert_eq!(restored, vec![a, b, c]);
        assert_eq!(history.undo_depth(), 4);
    }

    #[test]
    fn undo_restores_a_move_between_pages() {
        let mut doc = Document::new();
        let mut history = History::new();
        let one = PageId::new();
        let two = PageId::new();
        history
            .commit_at_current(
                &mut doc,
                &[
                    Command::CreatePage {
                        id: one,
                        name: "One".into(),
                    },
                    Command::CreatePage {
                        id: two,
                        name: "Two".into(),
                    },
                ],
            )
            .expect("pages should be created");

        let shape = NodeId::new();
        history
            .commit_at_current(
                &mut doc,
                &[Command::CreateNode {
                    id: shape,
                    kind: NodeKind::Shape,
                    name: None,
                    parent: NodePlacement::PageRoot {
                        page: one,
                        position: Position::Last,
                    },
                }],
            )
            .expect("node should be created");
        assert_eq!(doc.page_of(shape), Some(one));

        history
            .commit_at_current(
                &mut doc,
                &[Command::MoveNode {
                    id: shape,
                    parent: NodePlacement::PageRoot {
                        page: two,
                        position: Position::Last,
                    },
                }],
            )
            .expect("move should apply");
        assert_eq!(doc.page_of(shape), Some(two));

        history.undo(&mut doc).expect("undo should apply");
        assert_eq!(doc.page_of(shape), Some(one));
        assert_eq!(doc.page(one).unwrap().roots(), &[shape]);
        assert!(doc.page(two).unwrap().roots().is_empty());
    }

    #[test]
    fn undo_restores_a_move_into_a_frame() {
        let (mut doc, mut history, page) = history_doc();
        let frame = node(&mut doc, &mut history, NodeKind::Frame, "frame", page);
        let shape = node(&mut doc, &mut history, NodeKind::Shape, "shape", page);

        history
            .commit_at_current(
                &mut doc,
                &[Command::MoveNode {
                    id: shape,
                    parent: NodePlacement::Child {
                        parent: frame,
                        position: Position::Last,
                    },
                }],
            )
            .expect("move should apply");
        assert_eq!(doc.parent_of(shape), Some(frame));
        assert_eq!(doc.page_of(shape), Some(page));

        history.undo(&mut doc).expect("undo should apply");
        assert_eq!(doc.parent_of(shape), None);
        assert_eq!(doc.page_of(shape), Some(page));
        assert!(doc.node(frame).unwrap().children().is_empty());
    }

    #[test]
    fn undo_restores_a_deleted_leaf_to_its_parent_and_index() {
        let (mut doc, mut history, page) = history_doc();
        let frame = node(&mut doc, &mut history, NodeKind::Frame, "frame", page);
        let first = NodeId::new();
        let second = NodeId::new();
        commit(
            &mut doc,
            &[
                Command::CreateNode {
                    id: first,
                    kind: NodeKind::Shape,
                    name: None,
                    parent: NodePlacement::Child {
                        parent: frame,
                        position: Position::Last,
                    },
                },
                Command::CreateNode {
                    id: second,
                    kind: NodeKind::Shape,
                    name: None,
                    parent: NodePlacement::Child {
                        parent: frame,
                        position: Position::Last,
                    },
                },
            ],
        );

        let before = doc.clone();
        let batch = history
            .commit_at_current(&mut doc, &[Command::DeleteNode { id: first }])
            .expect("delete should apply");
        assert!(!doc.contains_node(first));
        assert_eq!(doc.node(frame).unwrap().children(), &[second]);

        let entry = inverse_with_subtree(&before, &batch);
        apply_batch_at_current(&mut doc, entry.inverse()).expect("undo should apply");
        assert_eq!(doc.node(frame).unwrap().children(), &[first, second]);
        assert_eq!(doc.page_of(first), Some(page));
    }

    #[test]
    fn deleting_a_frame_needs_the_subtree_aware_inverse() {
        let (mut doc, mut history, page) = history_doc();
        let frame = node(&mut doc, &mut history, NodeKind::Frame, "frame", page);
        let child = NodeId::new();
        commit(
            &mut doc,
            &[Command::CreateNode {
                id: child,
                kind: NodeKind::Text,
                name: Some("label".into()),
                parent: NodePlacement::Child {
                    parent: frame,
                    position: Position::Last,
                },
            }],
        );
        let before = doc.clone();

        let mut local = History::new();
        let batch = local
            .commit_at_current(&mut doc, &[Command::DeleteNode { id: frame }])
            .expect("delete should apply");
        assert!(!doc.contains_node(frame));
        assert!(!doc.contains_node(child));

        let entry = inverse_with_subtree(&before, &batch);
        apply_batch_at_current(&mut doc, entry.inverse())
            .expect("subtree-aware inverse should apply");

        assert_eq!(
            doc.node(frame).expect("frame is restored").kind,
            NodeKind::Frame
        );
        let restored_child = doc.node(child).expect("child is restored");
        assert_eq!(restored_child.kind, NodeKind::Text);
        assert_eq!(restored_child.name(), "label");
        assert_eq!(doc.parent_of(child), Some(frame));
        assert_eq!(doc.page_of(child), Some(page));
        assert_eq!(doc.page(page).unwrap().roots(), &[frame]);
    }

    #[test]
    fn a_subtree_restore_is_rejected_when_the_parent_is_gone() {
        let (mut doc, mut history, page) = history_doc();
        let frame = node(&mut doc, &mut history, NodeKind::Frame, "frame", page);
        let child = NodeId::new();
        commit(
            &mut doc,
            &[Command::CreateNode {
                id: child,
                kind: NodeKind::Shape,
                name: None,
                parent: NodePlacement::Child {
                    parent: frame,
                    position: Position::Last,
                },
            }],
        );
        let before = doc.clone();
        let batch = history
            .commit_at_current(&mut doc, &[Command::DeleteNode { id: child }])
            .expect("delete should apply");

        // Removing the frame invalidates the inverse, which needed it as the
        // child's parent.
        commit(&mut doc, &[Command::DeleteNode { id: frame }]);

        let entry = inverse_with_subtree(&before, &batch);
        let error = apply_batch_at_current(&mut doc, entry.inverse()).unwrap_err();
        assert!(matches!(error, BatchError::Command(_)));
    }

    #[test]
    fn undo_removes_a_created_page() {
        let (mut doc, mut history, page) = history_doc();
        assert!(doc.page(page).is_some());
        history.undo(&mut doc).expect("undo should apply");
        assert!(doc.page(page).is_none());
    }

    #[test]
    fn undo_of_nodes_then_page_runs_the_page_removal_last() {
        let mut doc = Document::new();
        let mut history = History::new();
        let page = PageId::new();
        let shape = NodeId::new();
        history
            .commit_at_current(
                &mut doc,
                &[
                    Command::CreatePage {
                        id: page,
                        name: "Page".into(),
                    },
                    Command::CreateNode {
                        id: shape,
                        kind: NodeKind::Shape,
                        name: None,
                        parent: NodePlacement::PageRoot {
                            page,
                            position: Position::Last,
                        },
                    },
                ],
            )
            .expect("batch should apply");

        let inverse = history.done.last().unwrap().inverse();
        assert!(matches!(inverse[0], Command::DeleteNode { id } if id == shape));
        assert!(matches!(inverse[1], Command::RemovePage { id } if id == page));

        history.undo(&mut doc).expect("undo should apply");
        assert!(doc.page(page).is_none());
        assert!(!doc.contains_node(shape));
    }

    #[test]
    fn removing_a_page_that_still_holds_nodes_is_refused() {
        let (mut doc, mut history, page) = history_doc();
        node(&mut doc, &mut history, NodeKind::Shape, "shape", page);
        let error =
            apply_batch_at_current(&mut doc, &[Command::RemovePage { id: page }]).unwrap_err();
        assert!(matches!(error, BatchError::Command(_)));
        assert!(doc.page(page).is_some());
    }

    #[test]
    fn a_new_commit_discards_the_redo_branch() {
        let (mut doc, mut history, page) = history_doc();
        let shape = node(&mut doc, &mut history, NodeKind::Shape, "shape", page);
        history
            .commit_with_at_current(
                &mut doc,
                &[Command::DeleteNode { id: shape }],
                inverse_with_subtree,
            )
            .expect("delete should apply");
        history.undo(&mut doc).expect("undo should apply");
        assert!(history.can_redo());

        node(&mut doc, &mut history, NodeKind::Text, "other", page);
        assert!(!history.can_redo());
        assert_eq!(history.redo_depth(), 0);
    }

    #[test]
    fn undo_after_an_intervening_write_is_refused_and_stays_available() {
        let (mut doc, mut history, page) = history_doc();
        let shape = node(&mut doc, &mut history, NodeKind::Shape, "shape", page);

        // A write that bypasses the history leaves it out of sync.
        commit(
            &mut doc,
            &[Command::RenameNode {
                id: shape,
                name: Some("outside".into()),
            }],
        );

        let error = history.undo(&mut doc).unwrap_err();
        assert!(matches!(error, HistoryError::OutOfSync { .. }));
        assert!(history.can_undo());
        assert_eq!(history.undo_depth(), 2);
    }

    #[test]
    fn an_undo_from_a_moved_document_is_refused_and_stays_undoable() {
        let (mut doc, mut history, page) = history_doc();
        let frame = node(&mut doc, &mut history, NodeKind::Frame, "frame", page);
        let child = NodeId::new();
        commit(
            &mut doc,
            &[Command::CreateNode {
                id: child,
                kind: NodeKind::Shape,
                name: None,
                parent: NodePlacement::Child {
                    parent: frame,
                    position: Position::Last,
                },
            }],
        );

        history
            .commit_with_at_current(
                &mut doc,
                &[Command::DeleteNode { id: child }],
                inverse_with_subtree,
            )
            .expect("delete should apply");
        history
            .commit_with_at_current(
                &mut doc,
                &[Command::DeleteNode { id: frame }],
                inverse_with_subtree,
            )
            .expect("frame delete should apply");

        // Another writer moves the document past the recorded step.
        commit(
            &mut doc,
            &[Command::CreateNode {
                id: NodeId::new(),
                kind: NodeKind::Shape,
                name: None,
                parent: NodePlacement::PageRoot {
                    page,
                    position: Position::Last,
                },
            }],
        );

        let depth = history.undo_depth();
        let error = history.undo(&mut doc).unwrap_err();
        assert!(
            matches!(error, HistoryError::OutOfSync { .. }),
            "got {error:?}"
        );
        assert_eq!(history.undo_depth(), depth);
    }

    #[test]
    fn a_rejected_undo_stays_undoable() {
        let (mut doc, mut history, page) = history_doc();
        let shape = node(&mut doc, &mut history, NodeKind::Shape, "shape", page);

        // A second step that renames the shape. Its inverse rewrites the name
        // of a node that a later out-of-band edit removes, so the inverse is
        // valid to *record* but impossible to *apply*.
        history
            .commit_at_current(
                &mut doc,
                &[Command::RenameNode {
                    id: shape,
                    name: Some("renamed".into()),
                }],
            )
            .expect("rename should apply");

        let following = history.undo_depth();
        history
            .commit_at_current(
                &mut doc,
                &[Command::CreateNode {
                    id: NodeId::new(),
                    kind: NodeKind::Shape,
                    name: None,
                    parent: NodePlacement::PageRoot {
                        page,
                        position: Position::Last,
                    },
                }],
            )
            .expect("create should apply");
        let depth = history.undo_depth();

        // Undo the create, then break the rename's target out of band.
        history.undo(&mut doc).expect("create undo should apply");
        assert_eq!(history.undo_depth(), following);
        apply_batch_at_current(&mut doc, &[Command::DeleteNode { id: shape }])
            .expect("out-of-band delete should apply");

        let error = history.undo(&mut doc).unwrap_err();
        assert!(
            matches!(
                error,
                HistoryError::Rejected(_) | HistoryError::OutOfSync { .. }
            ),
            "got {error:?}"
        );
        assert_eq!(history.undo_depth(), depth - 1);
    }

    #[test]
    fn an_inverse_the_write_path_refuses_surfaces_as_rejected() {
        let (mut doc, _, page) = history_doc();
        let shape = node(
            &mut doc,
            &mut History::new(),
            NodeKind::Shape,
            "shape",
            page,
        );

        // An inverse naming a node that is not in the document is refused by
        // `apply_batch`; `replay` must report that as `Rejected`, not as a
        // timeline desync.
        let entry = HistoryEntry {
            revision: doc.revision(),
            inverse: vec![Command::RenameNode {
                id: NodeId::new(),
                name: Some("ghost".into()),
            }],
            coalesced: false,
        };
        let error = replay(&mut doc, &entry).unwrap_err();
        assert!(matches!(error, HistoryError::Rejected(_)), "got {error:?}");
        // A refused replay leaves the document untouched.
        assert_eq!(doc.revision(), Revision::INITIAL.next().next());
        assert_eq!(doc.node(shape).unwrap().name(), "shape");
    }

    #[test]
    fn undo_is_an_edit_forward_not_a_rewind_of_the_revision() {
        let (mut doc, mut history, page) = history_doc();
        node(&mut doc, &mut history, NodeKind::Shape, "shape", page);
        let revision_before_undo = doc.revision();

        history.undo(&mut doc).expect("undo should apply");
        assert!(doc.revision() > revision_before_undo);
    }

    #[test]
    fn a_merged_sequence_undoes_in_one_step() {
        let (mut doc, mut history, page) = history_doc();
        let shape = node(&mut doc, &mut history, NodeKind::Shape, "shape", page);
        let depth = history.undo_depth();

        for name in ["one", "two", "three"] {
            history
                .commit_merged_at_current(
                    &mut doc,
                    &[Command::RenameNode {
                        id: shape,
                        name: Some(name.into()),
                    }],
                    |_before, commit| inverse_entry(commit),
                )
                .expect("merged rename should apply");
        }

        assert_eq!(history.undo_depth(), depth + 1);
        assert_eq!(doc.node(shape).unwrap().name(), "three");
        history.undo(&mut doc).expect("undo should apply");
        assert_eq!(doc.node(shape).unwrap().name(), "shape");
    }

    #[test]
    fn a_merged_sequence_without_a_previous_step_still_records_one() {
        let mut doc = Document::new();
        let mut history = History::new();
        history
            .commit_merged_at_current(
                &mut doc,
                &[Command::CreatePage {
                    id: PageId::new(),
                    name: "Page".into(),
                }],
                |_before, commit| inverse_entry(commit),
            )
            .expect("batch should apply");
        assert_eq!(history.undo_depth(), 1);
    }

    #[test]
    fn the_document_validates_after_a_sequence_of_undo_and_redo() {
        let (mut doc, mut history, page) = history_doc();
        let frame = node(&mut doc, &mut history, NodeKind::Frame, "frame", page);
        let shape = node(&mut doc, &mut history, NodeKind::Shape, "shape", page);
        history
            .commit_at_current(
                &mut doc,
                &[Command::MoveNode {
                    id: shape,
                    parent: NodePlacement::Child {
                        parent: frame,
                        position: Position::Last,
                    },
                }],
            )
            .expect("move should apply");

        while history.can_undo() {
            history.undo(&mut doc).expect("undo should apply");
            validate(&doc, ResourceLimits::UNLIMITED).expect("document stays valid");
        }
        while history.can_redo() {
            history.redo(&mut doc).expect("redo should apply");
            validate(&doc, ResourceLimits::UNLIMITED).expect("document stays valid");
        }
        assert_eq!(doc.parent_of(shape), Some(frame));
    }

    #[test]
    fn an_empty_batch_records_no_inverse() {
        let (mut doc, mut history, _) = history_doc();
        let depth = history.undo_depth();
        history
            .commit_at_current(&mut doc, &[])
            .expect("an empty batch applies");
        assert_eq!(history.undo_depth(), depth + 1);
        assert!(history.done.last().unwrap().is_empty());
    }

    #[test]
    fn undo_does_not_change_the_document_identity() {
        let (mut doc, mut history, _) = history_doc();
        let id = doc.id();
        history.undo(&mut doc).expect("undo should apply");
        assert_eq!(doc.id(), id);
    }

    #[test]
    fn an_inverse_entry_remembers_the_revision_it_produced() {
        let (doc, history, _) = history_doc();
        let expected = doc.revision();
        let entry = history.done.last().unwrap();
        assert_eq!(entry.revision(), expected);
        assert!(!entry.is_empty());
    }

    #[test]
    fn clearing_forgets_both_stacks() {
        let (mut doc, mut history, page) = history_doc();
        node(&mut doc, &mut history, NodeKind::Shape, "shape", page);
        history.undo(&mut doc).expect("undo should apply");
        assert!(history.can_undo() && history.can_redo());

        history.clear();
        assert!(!history.can_undo());
        assert!(!history.can_redo());
    }

    #[test]
    fn check_ids_free_rejects_a_collision() {
        let (mut doc, mut history, page) = history_doc();
        let existing = node(&mut doc, &mut history, NodeKind::Shape, "shape", page);
        let tree = vec![Node::new(existing, NodeKind::Shape, None)];
        let error = check_ids_free(&doc, &tree).unwrap_err();
        assert_eq!(error.code, CommandErrorCode::IdCollision);
    }

    #[test]
    fn check_ids_free_accepts_a_disjoint_set() {
        let (doc, _, _) = history_doc();
        let tree = vec![Node::new(NodeId::new(), NodeKind::Shape, None)];
        check_ids_free(&doc, &tree).expect("the identity is new");
    }

    #[test]
    fn page_of_removed_reads_the_page_a_node_lived_on() {
        let (mut doc, mut history, page) = history_doc();
        let shape = node(&mut doc, &mut history, NodeKind::Shape, "shape", page);
        assert_eq!(page_of_removed(&doc, shape), Some(page));
    }

    #[test]
    fn restored_page_carries_its_identity_and_name() {
        let id = PageId::new();
        let page = restored_page(id, "Recovered");
        assert_eq!(page.id, id);
        assert_eq!(page.name, "Recovered");
        assert_eq!(page, Page::new(id, "Recovered"));
    }

    #[test]
    fn history_error_display_distinguishes_the_cases() {
        assert_eq!(
            HistoryError::NothingToUndo.to_string(),
            "there is nothing to undo"
        );
        let out_of_sync = HistoryError::OutOfSync {
            expected: Revision::from_raw(3),
            actual: Revision::from_raw(5),
        };
        let text = out_of_sync.to_string();
        assert!(text.contains('3'), "{text}");
        assert!(text.contains('5'), "{text}");
    }

    #[test]
    fn redundant_deletes_for_one_node_are_dropped() {
        let id = NodeId::new();
        let commands = vec![Command::DeleteNode { id }, Command::DeleteNode { id }];
        assert_eq!(drop_redundant_deletes(commands).len(), 1);
    }

    #[test]
    fn a_placeholder_page_is_stable_across_calls() {
        assert_eq!(uuid_placeholder_page(), uuid_placeholder_page());
    }
}
