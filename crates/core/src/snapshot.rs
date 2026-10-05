//! Immutable read views of the document.
//!
//! Read access is provided through a fixed snapshot; writes are not. Layout
//! results, render output, and helper indexes are derived and rebuildable, so
//! they never live inside the snapshot of the document itself (§4, §5.4).
//!
//! ## Why a snapshot, not just `&Document`
//!
//! A layout or render adapter must not keep a live reference into the
//! document, because the next batch would invalidate it mid-pass. A
//! [`Snapshot`] pairs an owned, immutable copy of the document with the
//! revision it was taken at, so an adapter can read for as long as it needs and
//! a caller can compare the snapshot's revision against the engine's to decide
//! whether a cached result is still valid (§6.6). Because the copy is owned,
//! the snapshot stays `Send` if the document does, which is what lets a host
//! move a render pass to a worker thread without holding the engine lock.

use crate::ids::{AssetId, DocumentId, NodeId, PageId};
use crate::model::{Asset, Document, Node, Page, Revision};

/// An immutable, revision-tagged copy of a document.
///
/// The snapshot owns its data, so it does not borrow the engine and cannot be
/// invalidated by a later edit. It is deliberately read-only: the only way to
/// change a document remains the engine's command surface, so a snapshot can
/// never become a second writer.
#[derive(Debug, Clone)]
pub struct Snapshot {
    document: Document,
    revision: Revision,
}

impl Snapshot {
    /// Takes a snapshot of `document` at its current revision.
    ///
    /// The copy is cheap relative to a layout or render pass it guards, and it
    /// is the intended way to hand the scene to an adapter without lending it a
    /// mutable reference.
    #[must_use]
    pub fn of(document: &Document) -> Self {
        Self {
            document: document.clone(),
            revision: document.revision(),
        }
    }

    /// The revision this snapshot was taken at.
    ///
    /// Compare against [`DocumentEngine::revision`](crate::engine::DocumentEngine::revision)
    /// to decide whether a derived result made from this snapshot is still
    /// valid; a mismatch means the result must be recomputed (§4).
    #[must_use]
    pub const fn revision(&self) -> Revision {
        self.revision
    }

    /// The document identity.
    #[must_use]
    pub const fn id(&self) -> DocumentId {
        self.document.id()
    }

    /// The ordered pages.
    #[must_use]
    pub fn pages(&self) -> &[Page] {
        self.document.pages()
    }

    /// The ordered assets.
    #[must_use]
    pub fn assets(&self) -> &[Asset] {
        self.document.assets()
    }

    /// Looks up an asset by identity.
    #[must_use]
    pub fn asset(&self, id: AssetId) -> Option<&Asset> {
        self.document.asset(id)
    }

    /// Looks up a page by identity.
    #[must_use]
    pub fn page(&self, id: PageId) -> Option<&Page> {
        self.document.page(id)
    }

    /// Looks up a node by identity.
    #[must_use]
    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.document.node(id)
    }

    /// Returns the derived parent of a node, if any.
    #[must_use]
    pub fn parent_of(&self, id: NodeId) -> Option<NodeId> {
        self.document.parent_of(id)
    }

    /// Returns the page a node belongs to, if any.
    #[must_use]
    pub fn page_of(&self, id: NodeId) -> Option<PageId> {
        self.document.page_of(id)
    }

    /// The number of nodes in the snapshot.
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.document.node_count()
    }

    /// Iterates over all nodes in unspecified order.
    pub fn nodes(&self) -> impl Iterator<Item = &Node> {
        self.document.nodes()
    }

    /// Whether `candidate` lies inside the subtree rooted at `ancestor`.
    #[must_use]
    pub fn is_descendant_of(&self, candidate: NodeId, ancestor: NodeId) -> bool {
        self.document.is_descendant_of(candidate, ancestor)
    }

    /// Borrows the underlying document.
    ///
    /// Provided for code that already takes a `&Document`, such as the format
    /// exporter, without forcing a second clone.
    #[must_use]
    pub const fn document(&self) -> &Document {
        &self.document
    }
}

impl From<&Document> for Snapshot {
    fn from(document: &Document) -> Self {
        Self::of(document)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{Command, NodePlacement, Position};
    use crate::model::NodeKind;
    use crate::transaction::apply_batch_at_current;

    #[test]
    fn a_snapshot_captures_the_revision_at_take_time() {
        let mut doc = Document::new();
        apply_batch_at_current(
            &mut doc,
            &[Command::CreatePage {
                id: PageId::new(),
                name: "P".into(),
            }],
        )
        .unwrap();

        let snapshot = Snapshot::of(&doc);
        assert_eq!(snapshot.revision(), doc.revision());

        let page = doc.pages()[0].id;
        apply_batch_at_current(
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
        .unwrap();

        assert_ne!(snapshot.revision(), doc.revision());
        assert_eq!(snapshot.node_count(), 0, "the snapshot predates the node");
        assert_eq!(doc.node_count(), 1);
    }

    #[test]
    fn a_snapshot_mirrors_the_structure_it_was_taken_from() {
        let mut doc = Document::new();
        let page = PageId::new();
        let frame = NodeId::new();
        apply_batch_at_current(
            &mut doc,
            &[
                Command::CreatePage {
                    id: page,
                    name: "P".into(),
                },
                Command::CreateNode {
                    id: frame,
                    kind: NodeKind::Frame,
                    name: Some("F".into()),
                    parent: NodePlacement::PageRoot {
                        page,
                        position: Position::Last,
                    },
                },
            ],
        )
        .unwrap();

        let snapshot = Snapshot::from(&doc);
        assert_eq!(
            snapshot.page(page).map(|p| p.roots()),
            Some([frame].as_slice())
        );
        assert_eq!(snapshot.page_of(frame), Some(page));
        assert_eq!(snapshot.node(frame).map(|n| n.name()), Some("F"));
        assert_eq!(snapshot.document().id(), doc.id());
    }
}
