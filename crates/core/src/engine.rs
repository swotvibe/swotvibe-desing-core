//! The public `DocumentEngine` entry point.
//!
//! Ownership: the single writer that owns a document at any moment. Writes are
//! ordered through an actor/queue or a host lock around `&mut DocumentEngine`.
//! Not owned: filesystem, network, or rendering.
//!
//! The engine is UI-agnostic. UI, agent, and importer layers all send commands
//! to the same owner with an expected revision, so the engine never has to
//! arbitrate between concurrent writers itself (§6.6). A host that needs
//! concurrency wraps the engine in its own lock or actor and serializes calls.

use crate::commands::Command;
use crate::history::{
    DEFAULT_HISTORY_CAPACITY, History, HistoryEntry, HistoryError, inverse_with_subtree,
};
use crate::model::{Document, Revision};
use crate::transaction::{BatchError, Commit};
use crate::validation::{ResourceLimits, ValidationErrors, validate};

/// The core kernel entry point: a document, its undo history, and the single
/// write path that keeps them consistent.
///
/// Every mutation goes through [`DocumentEngine::apply`],
/// [`DocumentEngine::apply_at`], [`DocumentEngine::undo`], or
/// [`DocumentEngine::redo`]. A rejected batch leaves the document, its
/// revision, and the timeline untouched (§6.2).
///
/// # Examples
///
/// ```
/// use swotvibe_core::{Command, DocumentEngine, PageId};
///
/// let mut engine = DocumentEngine::new();
/// let page = PageId::new();
/// engine
///     .apply(&[Command::CreatePage {
///         id: page,
///         name: "Page 1".into(),
///     }])
///     .expect("a fresh document accepts a page creation");
///
/// assert!(engine.document().page(page).is_some());
/// assert!(engine.can_undo());
/// ```
#[derive(Debug, Clone)]
pub struct DocumentEngine {
    document: Document,
    history: History,
}

impl DocumentEngine {
    /// Creates an engine over a fresh, empty document.
    #[must_use]
    pub fn new() -> Self {
        Self {
            document: Document::new(),
            history: History::new(),
        }
    }

    /// Creates an engine over an existing document with an empty timeline.
    ///
    /// Use this when opening a file: the loaded document's revision is kept,
    /// and undo history starts fresh because session history is not part of the
    /// persisted payload in v1 (§6.4).
    #[must_use]
    pub fn with_document(document: Document) -> Self {
        Self {
            document,
            history: History::new(),
        }
    }

    /// Creates an engine over a fresh document with a caller-selected undo
    /// depth. A capacity of zero disables undo recording.
    #[must_use]
    pub fn with_history_capacity(capacity: usize) -> Self {
        Self {
            document: Document::new(),
            history: History::with_capacity(capacity),
        }
    }

    /// Creates an engine over an existing document with a caller-selected
    /// undo depth and an empty timeline.
    #[must_use]
    pub fn with_document_and_history_capacity(document: Document, capacity: usize) -> Self {
        Self {
            document,
            history: History::with_capacity(capacity),
        }
    }

    /// A shared read view of the current document.
    ///
    /// The view borrows the live document, so it cannot outlive a call to any
    /// `&mut self` method. This is deliberate: reads are cheap and always see
    /// the latest committed revision, never an abandoned working copy.
    #[must_use]
    pub const fn document(&self) -> &Document {
        &self.document
    }

    /// The configured maximum undo depth, or `None` for unbounded history.
    #[must_use]
    pub const fn history_capacity(&self) -> Option<usize> {
        self.history.capacity()
    }

    /// The default maximum number of undoable batches retained by a new
    /// engine.
    pub const fn default_history_capacity() -> usize {
        DEFAULT_HISTORY_CAPACITY
    }

    /// Returns the session history for inspection or host configuration.
    #[must_use]
    pub const fn history(&self) -> &History {
        &self.history
    }

    /// Returns mutable access to the session history configuration.
    ///
    /// The timeline is separate from the document and cannot change document
    /// content through this handle.
    pub fn history_mut(&mut self) -> &mut History {
        &mut self.history
    }

    /// The document's current revision, which a caller passes back as the
    /// `expected_revision` of the next batch.
    #[must_use]
    pub const fn revision(&self) -> Revision {
        self.document.revision()
    }

    /// Applies one batch atomically at the current revision.
    ///
    /// The batch is applied against a detached working copy that becomes live
    /// only if every command succeeds, so a failure leaves no trace (§6.2). On
    /// success the whole batch becomes one undoable step (§6.4).
    ///
    /// # Errors
    ///
    /// Returns [`BatchError`] if any command is invalid in the current state.
    /// The document, revision, and history are untouched.
    ///
    /// # Examples
    ///
    /// ```
    /// use swotvibe_core::{Command, DocumentEngine, PageId};
    ///
    /// let mut engine = DocumentEngine::new();
    /// let page = PageId::new();
    /// engine
    ///     .apply(&[Command::CreatePage { id: page, name: "P".into() }])
    ///     .expect("page creation succeeds");
    /// assert!(engine.document().page(page).is_some());
    /// ```
    pub fn apply(&mut self, commands: &[Command]) -> Result<Commit, BatchError> {
        let revision = self.document.revision();
        self.apply_at(revision, commands)
    }

    /// Applies a batch the caller has already checked the revision for.
    ///
    /// This is the raw path used by a host that owns the revision itself, for
    /// example a preview session that captured a revision at drag start (§6.3).
    ///
    /// # Errors
    ///
    /// As [`DocumentEngine::apply`], plus [`BatchError::RevisionConflict`] when
    /// `expected_revision` is not the document's current revision.
    pub fn apply_at(
        &mut self,
        expected_revision: Revision,
        commands: &[Command],
    ) -> Result<Commit, BatchError> {
        self.history.commit_with(
            &mut self.document,
            expected_revision,
            commands,
            inverse_with_subtree,
        )
    }

    /// Whether there is a step to undo.
    #[must_use]
    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    /// Whether there is a step to redo.
    #[must_use]
    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    /// The number of undoable steps currently recorded.
    #[must_use]
    pub fn undo_depth(&self) -> usize {
        self.history.undo_depth()
    }

    /// The number of redoable steps currently recorded.
    #[must_use]
    pub fn redo_depth(&self) -> usize {
        self.history.redo_depth()
    }

    /// The most recent undoable entry, or `None` when the timeline is empty.
    #[must_use]
    pub fn last_entry(&self) -> Option<&HistoryEntry> {
        self.history.last_entry()
    }

    /// Undoes the most recent step.
    ///
    /// The inverse passes through the same validation as any other batch, so a
    /// later change that invalidates the inverse is reported rather than
    /// silently skipped (§6.4).
    ///
    /// # Errors
    ///
    /// Returns [`HistoryError::NothingToUndo`] when the timeline is empty, or
    /// [`HistoryError::Rejected`] if the inverse is no longer applicable.
    pub fn undo(&mut self) -> Result<Commit, HistoryError> {
        self.history.undo(&mut self.document)
    }

    /// Redoes the most recently undone step.
    ///
    /// # Errors
    ///
    /// Returns [`HistoryError::NothingToRedo`] when nothing was undone, or
    /// [`HistoryError::Rejected`] if the redo is no longer applicable.
    pub fn redo(&mut self) -> Result<Commit, HistoryError> {
        self.history.redo(&mut self.document)
    }

    /// Checks the whole document against the integrity rules.
    ///
    /// Every committed batch is validated incrementally as it applies; this is
    /// the full reference check used at load, migration, import, and in tests
    /// (§6.2, §7).
    ///
    /// # Errors
    ///
    /// Returns every rule violation found.
    pub fn validate(&self) -> Result<(), ValidationErrors> {
        validate(&self.document, ResourceLimits::UNLIMITED)
    }

    /// Replaces the document, dropping all history.
    ///
    /// This is what a host calls after opening a file or loading a migrated
    /// document. The new document's revision is preserved.
    pub fn replace_document(&mut self, document: Document) {
        self.document = document;
        self.history.clear();
    }

    /// Consumes the engine and returns the document, discarding the timeline.
    #[must_use]
    pub fn into_document(self) -> Document {
        self.document
    }
}

impl Default for DocumentEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{NodePlacement, Position};
    use crate::ids::{NodeId, PageId};
    use crate::model::NodeKind;

    /// A page with one frame root, committed as a single batch.
    fn seeded() -> (DocumentEngine, PageId, NodeId) {
        let mut engine = DocumentEngine::new();
        let page = PageId::new();
        let frame = NodeId::new();
        engine
            .apply(&[
                Command::CreatePage {
                    id: page,
                    name: "Page 1".into(),
                },
                Command::CreateNode {
                    id: frame,
                    kind: NodeKind::Frame,
                    name: Some("Frame".into()),
                    parent: NodePlacement::PageRoot {
                        page,
                        position: Position::Last,
                    },
                },
            ])
            .expect("seeding succeeds");
        (engine, page, frame)
    }

    #[test]
    fn a_new_engine_is_empty_at_the_initial_revision() {
        let engine = DocumentEngine::new();
        assert_eq!(engine.revision(), Revision::INITIAL);
        assert!(!engine.can_undo());
        assert!(!engine.can_redo());
        assert!(engine.validate().is_ok());
    }

    #[test]
    fn apply_advances_the_revision_and_enables_undo() {
        let (engine, _page, _frame) = seeded();
        assert_eq!(engine.revision(), Revision::INITIAL.next());
        assert!(engine.can_undo());
        assert_eq!(engine.undo_depth(), 1);
        assert!(engine.last_entry().is_some());
    }

    #[test]
    fn a_failed_batch_leaves_revision_and_history_untouched() {
        let (mut engine, page, frame) = seeded();
        let before = engine.revision();
        let depth = engine.undo_depth();

        let err = engine
            .apply(&[
                Command::RenameNode {
                    id: frame,
                    name: Some("Renamed".into()),
                },
                Command::CreateNode {
                    id: NodeId::new(),
                    kind: NodeKind::Shape,
                    name: None,
                    parent: NodePlacement::PageRoot {
                        page,
                        position: Position::Before(NodeId::new()),
                    },
                },
            ])
            .expect_err("a sibling reference to a missing node is rejected");

        assert!(matches!(err, BatchError::Command(_)));
        assert_eq!(engine.revision(), before);
        assert_eq!(engine.undo_depth(), depth);
        assert_eq!(
            engine.document().node(frame).map(|n| n.name()),
            Some("Frame"),
            "the first command's rename must be rolled back with the batch"
        );
    }

    #[test]
    fn undo_and_redo_walk_the_timeline() {
        let (mut engine, _page, frame) = seeded();
        engine
            .apply(&[Command::RenameNode {
                id: frame,
                name: Some("Renamed".into()),
            }])
            .expect("rename succeeds");
        assert_eq!(
            engine.document().node(frame).map(|n| n.name()),
            Some("Renamed")
        );

        engine.undo().expect("undo succeeds");
        assert_eq!(
            engine.document().node(frame).map(|n| n.name()),
            Some("Frame")
        );
        assert!(engine.can_redo());

        engine.redo().expect("redo succeeds");
        assert_eq!(
            engine.document().node(frame).map(|n| n.name()),
            Some("Renamed")
        );
    }

    #[test]
    fn a_stale_expected_revision_is_a_conflict_not_a_change() {
        let (mut engine, page, _frame) = seeded();
        let err = engine
            .apply_at(
                Revision::INITIAL,
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
            .expect_err("a stale revision is refused");
        assert!(matches!(err, BatchError::RevisionConflict { .. }));
    }

    #[test]
    fn replace_document_drops_history_and_resets_revision() {
        let (mut engine, _page, _frame) = seeded();
        engine.replace_document(Document::new());
        assert_eq!(engine.revision(), Revision::INITIAL);
        assert!(!engine.can_undo());
        assert!(!engine.can_redo());
    }

    #[test]
    fn undoing_a_delete_removes_nothing_and_restores_the_subtree() {
        let (mut engine, page, frame) = seeded();
        let child = NodeId::new();
        engine
            .apply(&[Command::CreateNode {
                id: child,
                kind: NodeKind::Shape,
                name: Some("Child".into()),
                parent: NodePlacement::Child {
                    parent: frame,
                    position: Position::Last,
                },
            }])
            .expect("child creation succeeds");

        engine
            .apply(&[Command::DeleteNode { id: frame }])
            .expect("deleting the frame succeeds");
        assert!(engine.document().node(frame).is_none());
        assert!(engine.document().node(child).is_none());

        engine.undo().expect("undo restores the subtree");
        assert!(engine.document().node(frame).is_some());
        assert_eq!(
            engine.document().node(child).map(|n| n.name()),
            Some("Child"),
            "the whole subtree comes back, not just its root"
        );
        assert_eq!(engine.document().page_of(frame), Some(page));
    }
}
