//! The layout adapter contract: the request, the result, and the trait.
//!
//! Ownership: translating product layout semantics (fixed/fill/hug, flex
//! direction, gaps, padding) to and from an external layout engine, and the
//! layout result fingerprint.
//! Not owned: treating the engine as the document's source of truth. Layout
//! never writes to the document; it is a derived, rebuildable result.

use std::collections::BTreeMap;
use std::fmt;

use swotvibe_core::{NodeId, NodeKind, PageId, Sizing, Snapshot, Transform};
use swotvibe_text::FontFingerprint;

use crate::TAFFY_ENGINE_ID;

/// An axis-aligned rectangle in page space, in design units.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    /// The left edge.
    pub x: f64,
    /// The top edge.
    pub y: f64,
    /// The width.
    pub width: f64,
    /// The height.
    pub height: f64,
}

impl Rect {
    /// The right edge.
    #[must_use]
    pub fn right(&self) -> f64 {
        self.x + self.width
    }

    /// The bottom edge.
    #[must_use]
    pub fn bottom(&self) -> f64 {
        self.y + self.height
    }

    /// The union of two rectangles, used to accumulate a container's extent.
    #[must_use]
    pub fn union(&self, other: &Self) -> Self {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        Self {
            x,
            y,
            width: self.right().max(other.right()) - x,
            height: self.bottom().max(other.bottom()) - y,
        }
    }
}

/// One node's resolved geometry.
///
/// The offset, size, and transform are all in **design units**. The transform is
/// the node's effective local transform: the linear part of its stored
/// transform combined with the translation the layout decided, which is what
/// makes a flex child's position and a rotated node's own transform both
/// expressible in one value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodeLayout {
    /// The node.
    pub id: NodeId,
    /// The node's position inside its parent's space, from layout.
    pub offset: (f64, f64),
    /// The node's resolved size.
    pub size: (f64, f64),
    /// The effective local transform, from the parent's space into the node's.
    pub transform: Transform,
    /// The composed transform from page space into the node's space.
    pub world: Transform,
    /// The node's box in page space, as an axis-aligned bounding box of the
    /// transformed box. A rotated node's rectangle is therefore larger than its
    /// own size, which is the honest reading of "where is this node".
    pub rect: Rect,
}

/// Something the engine could not express exactly.
///
/// A diagnostic is not a failure: the engine produced a result, and this says
/// which part of the document it interpreted by a documented fallback rather
/// than by its own rule. It rides beside the result so a caller, or a reference
/// comparison, can see it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum LayoutDiagnostic {
    /// A leaf kind had a sizing rule that means nothing without content.
    ///
    /// `Hug` on a shape or an image has no content to fit, so the stored size is
    /// used. The fallback is recorded rather than applied silently.
    SizingFallback {
        /// The node.
        node: NodeId,
        /// Its kind.
        kind: NodeKind,
        /// The sizing rule that was replaced.
        sizing: Sizing,
    },
}

impl fmt::Display for LayoutDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SizingFallback { node, kind, sizing } => write!(
                f,
                "`{sizing:?}` has no meaning on a `{}` node ({node}); the stored size was used",
                kind.as_str()
            ),
        }
    }
}

/// Why a layout pass failed.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum LayoutError {
    /// The page is not in the snapshot.
    PageNotFound {
        /// The page that was requested.
        page: PageId,
    },
    /// A root of the page is not in the snapshot.
    NodeNotFound {
        /// The node that was requested.
        node: NodeId,
    },
    /// The requested page size is not usable.
    InvalidPageSize {
        /// The size that was requested.
        size: (f64, f64),
    },
    /// Measuring a node's text failed.
    ///
    /// The whole pass fails rather than laying the node out at a guessed size,
    /// because a wrong measurement moves every later node.
    Text {
        /// The node whose text could not be measured.
        node: NodeId,
        /// A human-readable explanation from the text engine.
        message: String,
    },
    /// The layout engine itself refused the tree.
    Backend {
        /// A human-readable explanation from the engine.
        message: String,
    },
}

impl fmt::Display for LayoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PageNotFound { page } => write!(f, "page {page} is not in the document"),
            Self::NodeNotFound { node } => write!(f, "node {node} is not in the document"),
            Self::InvalidPageSize { size } => {
                write!(f, "the page size {size:?} is not usable")
            }
            Self::Text { node, message } => {
                write!(f, "text on {node} could not be measured: {message}")
            }
            Self::Backend { message } => write!(f, "the layout engine failed: {message}"),
        }
    }
}

impl std::error::Error for LayoutError {}

/// What a layout pass was asked for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutOptions {
    /// The page's size, or `None` to size the page from its content.
    ///
    /// A page has no stored size in M0: the artboard is a host concern, so the
    /// caller states it here and the fingerprint records it.
    pub page_size: Option<(f64, f64)>,
}

impl LayoutOptions {
    /// No explicit page size: the page fits its content.
    pub const CONTENT_SIZED: Self = Self { page_size: None };

    /// Checks the options.
    ///
    /// # Errors
    ///
    /// Returns [`LayoutError::InvalidPageSize`] for a size that is not finite
    /// and positive.
    pub fn check(&self) -> Result<(), LayoutError> {
        if let Some(size) = self.page_size
            && (!size.0.is_finite() || !size.1.is_finite() || size.0 <= 0.0 || size.1 <= 0.0)
        {
            return Err(LayoutError::InvalidPageSize { size });
        }
        Ok(())
    }

    /// The default page size used when the caller states none.
    pub const DEFAULT_PAGE_SIZE: (f64, f64) = (1024.0, 768.0);

    /// The page size to lay out into.
    #[must_use]
    pub fn effective_page_size(&self) -> (f64, f64) {
        self.page_size.unwrap_or(Self::DEFAULT_PAGE_SIZE)
    }
}

impl Default for LayoutOptions {
    fn default() -> Self {
        Self::CONTENT_SIZED
    }
}

/// What a result was produced from.
///
/// A layout report is only comparable against the engine, the fonts, and the
/// page size it was produced with. A change to any field is a change of
/// fingerprint, not a regression to accept silently.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutFingerprint {
    /// The layout engine and its version.
    pub engine: String,
    /// The unit every number in the result is expressed in.
    pub units: &'static str,
    /// The page size the pass laid out into.
    pub page_size: (f64, f64),
    /// Whether the engine rounded any value to a whole unit.
    pub rounded: bool,
    /// The fonts the text measurements used.
    pub fonts: Vec<FontFingerprint>,
}

/// The result of one layout pass.
///
/// The node entries are ordered by the page's own structure, so a report
/// written from this result is byte-stable for the same document.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutResult {
    page: PageId,
    page_size: (f64, f64),
    order: Vec<NodeId>,
    nodes: BTreeMap<NodeId, NodeLayout>,
    diagnostics: Vec<LayoutDiagnostic>,
    fingerprint: LayoutFingerprint,
}

impl LayoutResult {
    /// Builds a result from an already ordered set of entries.
    ///
    /// `order` must list every node in `nodes` exactly once; the adapter that
    /// produces it walks the page in structure order.
    #[must_use]
    pub fn new(
        page: PageId,
        page_size: (f64, f64),
        order: Vec<NodeId>,
        nodes: BTreeMap<NodeId, NodeLayout>,
        diagnostics: Vec<LayoutDiagnostic>,
        fingerprint: LayoutFingerprint,
    ) -> Self {
        Self {
            page,
            page_size,
            order,
            nodes,
            diagnostics,
            fingerprint,
        }
    }

    /// The page that was laid out.
    #[must_use]
    pub const fn page(&self) -> PageId {
        self.page
    }

    /// The page size this pass laid out into, which may be larger than the
    /// requested one when content overflows.
    #[must_use]
    pub const fn page_size(&self) -> (f64, f64) {
        self.page_size
    }

    /// One node's geometry.
    #[must_use]
    pub fn node(&self, id: NodeId) -> Option<&NodeLayout> {
        self.nodes.get(&id)
    }

    /// Every node's geometry, in page structure order.
    pub fn nodes(&self) -> impl Iterator<Item = &NodeLayout> {
        self.order.iter().filter_map(|id| self.nodes.get(id))
    }

    /// The nodes in page structure order.
    #[must_use]
    pub fn order(&self) -> &[NodeId] {
        &self.order
    }

    /// The nodes the engine interpreted by a documented fallback.
    #[must_use]
    pub fn diagnostics(&self) -> &[LayoutDiagnostic] {
        &self.diagnostics
    }

    /// What this result was produced from.
    #[must_use]
    pub const fn fingerprint(&self) -> &LayoutFingerprint {
        &self.fingerprint
    }

    /// The union of every node's rectangle, which is the page's content extent.
    #[must_use]
    pub fn content_bounds(&self) -> Option<Rect> {
        self.nodes()
            .map(|node| node.rect)
            .reduce(|accumulated, rect| accumulated.union(&rect))
    }
}

/// Turning a document page into geometry, behind one adapter-neutral contract.
///
/// Implementations own their backend. The trait speaks design units and
/// [`Snapshot`]s only, so nothing above it depends on which layout library is
/// behind it.
pub trait LayoutEngine {
    /// The identifier of this engine, including its version, for a fingerprint.
    fn engine_id(&self) -> &'static str;

    /// Lays out one page of `snapshot`.
    ///
    /// # Errors
    ///
    /// Returns [`LayoutError`] for a page or node that is not in the snapshot, an
    /// unusable page size, a text measurement failure, or an engine failure.
    fn layout(
        &self,
        snapshot: &Snapshot,
        page: PageId,
        options: &LayoutOptions,
    ) -> Result<LayoutResult, LayoutError>;
}

/// The engine identifier a fingerprint records for the temporary M0 backend.
#[must_use]
pub fn engine_id() -> &'static str {
    TAFFY_ENGINE_ID
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectangles_union_to_their_bounding_box() {
        let a = Rect {
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
        };
        let b = Rect {
            x: -5.0,
            y: 20.0,
            width: 5.0,
            height: 5.0,
        };
        let union = a.union(&b);
        assert_eq!(
            union,
            Rect {
                x: -5.0,
                y: 0.0,
                width: 15.0,
                height: 25.0
            }
        );
    }

    #[test]
    fn a_non_positive_page_size_is_refused() {
        assert_eq!(
            LayoutOptions {
                page_size: Some((0.0, 10.0))
            }
            .check(),
            Err(LayoutError::InvalidPageSize { size: (0.0, 10.0) })
        );
        assert!(matches!(
            LayoutOptions {
                page_size: Some((f64::NAN, 10.0))
            }
            .check(),
            Err(LayoutError::InvalidPageSize { .. })
        ));
        assert_eq!(LayoutOptions::default().check(), Ok(()));
        assert_eq!(
            LayoutOptions::default().effective_page_size(),
            LayoutOptions::DEFAULT_PAGE_SIZE
        );
    }
}
