//! Document integrity rules and error reports.
//!
//! Ownership: the rules that keep the document sound and the error reports.
//! Not owned: silent correction of invalid data.
//!
//! Errors are typed, carry a stable code and the element path/identity, and
//! must never be hidden behind a silent fix-up. Per the technical
//! specification (§7) a returned error must:
//!
//! - be a typed value, not a formatted string;
//! - carry the identity (or path) of the offending element;
//! - carry a **stable code** that the UI and the command layer branch on. The
//!   human-readable message is diagnostics only and is not a programmatic key.
//!
//! ## Rule set
//!
//! The validator enforces, at minimum:
//!
//! - identity uniqueness within the document;
//! - parent/child and page/root references that resolve to existing nodes, and
//!   no reference to a removed element;
//! - a tree with no cycles and no multiple parents;
//! - consistency between the parent relation and the sibling order, with a
//!   single source of truth (the canonical children/roots lists);
//! - asset references that resolve;
//! - node properties that obey [`crate::props::NodeProps::check`];
//! - resource limits (tree depth, node count) on untrusted input.
//!
//! Numeric values are finite by construction (see [`crate::geometry`]).

use std::collections::HashSet;
use std::fmt;

use crate::ids::{AssetId, NodeId, PageId};
use crate::model::{Document, Revision};

/// Resource limits applied when reading untrusted input.
///
/// A limit of `None` means "no limit enforced by the validator". The defaults
/// exist so an import cannot turn a malformed file into unbounded work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceLimits {
    /// Maximum permitted tree depth (a page root has depth `0`).
    pub max_depth: Option<usize>,
    /// Maximum permitted number of nodes in the document.
    pub max_nodes: Option<usize>,
    /// Maximum permitted number of pages in the document.
    pub max_pages: Option<usize>,
}

impl ResourceLimits {
    /// Limits that reject nothing. Useful only for fully trusted, in-memory
    /// documents that the kernel created itself.
    pub const UNLIMITED: Self = Self {
        max_depth: None,
        max_nodes: None,
        max_pages: None,
    };

    /// The conservative defaults for untrusted input.
    pub const UNTRUSTED: Self = Self {
        max_depth: Some(512),
        max_nodes: Some(1_000_000),
        max_pages: Some(10_000),
    };
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self::UNTRUSTED
    }
}

/// A stable, machine-readable validation error code.
///
/// The UI and the command layer branch on these codes. The codes are part of
/// the module's public contract and must not be renamed without a migration
/// note: they are what a caller matches on, not the message text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ErrorCode {
    /// The same identity is used by two different elements.
    DuplicateId,
    /// A reference points at a node that is not in the document.
    MissingNode,
    /// A reference points at a page that is not in the document.
    MissingPage,
    /// A reference points at an asset that is not in the document.
    MissingAsset,
    /// Two or more parents claim the same child.
    MultipleParents,
    /// A node is its own ancestor.
    Cycle,
    /// The same child appears twice in one parent's children list.
    DuplicateChild,
    /// The same root appears twice in one page's roots list.
    DuplicateRoot,
    /// A leaf kind owns children.
    InvalidChildKind,
    /// A node's properties break a rule (wrong content for the kind, a negative
    /// length, an unusable font setting).
    InvalidProps,
    /// The derived parent/page index disagrees with the canonical lists.
    StaleIndex,
    /// A node in the scene is reachable from no page.
    OrphanNode,
    /// The tree exceeds the configured depth limit.
    DepthLimit,
    /// The document exceeds the configured node-count limit.
    NodeCountLimit,
    /// The document exceeds the configured page-count limit.
    PageCountLimit,
}

impl ErrorCode {
    /// The stable text form of the code.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DuplicateId => "duplicate-id",
            Self::MissingNode => "missing-node",
            Self::MissingPage => "missing-page",
            Self::MissingAsset => "missing-asset",
            Self::MultipleParents => "multiple-parents",
            Self::Cycle => "cycle",
            Self::DuplicateChild => "duplicate-child",
            Self::DuplicateRoot => "duplicate-root",
            Self::InvalidChildKind => "invalid-child-kind",
            Self::InvalidProps => "invalid-props",
            Self::StaleIndex => "stale-index",
            Self::OrphanNode => "orphan-node",
            Self::DepthLimit => "depth-limit",
            Self::NodeCountLimit => "node-count-limit",
            Self::PageCountLimit => "page-count-limit",
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The identity of the element an error is about.
///
/// The variants are separate so a caller cannot accidentally treat a `NodeId`
/// as a `PageId`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ElementRef {
    /// The document itself.
    Document,
    /// A page.
    Page(PageId),
    /// A scene node.
    Node(NodeId),
    /// An asset.
    Asset(AssetId),
}

impl fmt::Display for ElementRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Document => f.write_str("document"),
            Self::Page(id) => write!(f, "page {id}"),
            Self::Node(id) => write!(f, "node {id}"),
            Self::Asset(id) => write!(f, "asset {id}"),
        }
    }
}

/// A single integrity violation.
///
/// The `code` and `subject` are the programmatic contract; `message` is
/// diagnostics for a human and may change freely.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    /// The stable code the caller branches on.
    pub code: ErrorCode,
    /// The element the error is about.
    pub subject: ElementRef,
    /// A human-readable explanation. Not a programmatic key.
    pub message: String,
}

impl ValidationError {
    fn new(code: ErrorCode, subject: ElementRef, message: impl Into<String>) -> Self {
        Self {
            code,
            subject,
            message: message.into(),
        }
    }
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}: {}", self.code, self.subject, self.message)
    }
}

impl std::error::Error for ValidationError {}

/// A typed validation failure: one error or a batch of them.
///
/// A batch is returned when more than one rule fails, so a caller can present
/// every problem in an import at once instead of fixing them one at a time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationErrors(Vec<ValidationError>);

impl ValidationErrors {
    /// The contained errors, in discovery order.
    #[must_use]
    pub fn as_slice(&self) -> &[ValidationError] {
        &self.0
    }

    /// Returns the number of contained errors.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns whether there are no errors.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Iterates over the contained errors.
    pub fn iter(&self) -> std::slice::Iter<'_, ValidationError> {
        self.0.iter()
    }
}

impl IntoIterator for ValidationErrors {
    type Item = ValidationError;
    type IntoIter = std::vec::IntoIter<ValidationError>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl fmt::Display for ValidationErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0.as_slice() {
            [] => f.write_str("no validation errors"),
            [only] => write!(f, "{only}"),
            many => {
                write!(f, "{} validation errors: ", many.len())?;
                for (i, e) in many.iter().enumerate() {
                    if i > 0 {
                        f.write_str("; ")?;
                    }
                    write!(f, "{e}")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for ValidationErrors {}

/// Collects violations without short-circuiting.
///
/// The validator records every violation it can find in one pass so an import
/// reports the whole problem set, then decides whether to return them.
#[derive(Debug, Default)]
struct Report {
    errors: Vec<ValidationError>,
}

impl Report {
    fn push(&mut self, code: ErrorCode, subject: ElementRef, message: impl Into<String>) {
        self.errors
            .push(ValidationError::new(code, subject, message));
    }

    fn finish(self) -> Result<(), ValidationErrors> {
        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(ValidationErrors(self.errors))
        }
    }
}

/// Validates a document against every rule in the module contract.
///
/// Returns `Ok(())` when the document is sound, or every violation found.
///
/// # Errors
///
/// Returns [`ValidationErrors`] containing one or more [`ValidationError`]
/// values when any integrity rule fails.
pub fn validate(document: &Document, limits: ResourceLimits) -> Result<(), ValidationErrors> {
    let mut report = Report::default();

    check_resource_limits(document, limits, &mut report);
    check_page_identity(document, &mut report);
    check_node_identity(document, &mut report);
    check_asset_identity(document, &mut report);
    check_references(document, &mut report);
    check_parent_and_order(document, &mut report);
    check_child_kinds(document, &mut report);
    check_props(document, &mut report);
    check_depth(document, limits, &mut report);
    check_derived_index(document, &mut report);

    report.finish()
}

/// Validates a document using the untrusted-input limits.
///
/// # Errors
///
/// See [`validate`].
pub fn validate_untrusted(document: &Document) -> Result<(), ValidationErrors> {
    validate(document, ResourceLimits::UNTRUSTED)
}

/// Validates a document with no resource limits applied.
///
/// Intended for documents the kernel built itself and that are already known
/// to be within limits.
///
/// # Errors
///
/// See [`validate`].
pub fn validate_shape(document: &Document) -> Result<(), ValidationErrors> {
    validate(document, ResourceLimits::UNLIMITED)
}

fn check_resource_limits(document: &Document, limits: ResourceLimits, report: &mut Report) {
    if let Some(max) = limits.max_nodes {
        let count = document.node_count();
        if count > max {
            report.push(
                ErrorCode::NodeCountLimit,
                ElementRef::Document,
                format!("document has {count} nodes, limit is {max}"),
            );
        }
    }

    if let Some(max) = limits.max_pages {
        let count = document.pages().len();
        if count > max {
            report.push(
                ErrorCode::PageCountLimit,
                ElementRef::Document,
                format!("document has {count} pages, limit is {max}"),
            );
        }
    }
}

fn check_page_identity(document: &Document, report: &mut Report) {
    let mut seen = HashSet::new();
    for page in document.pages() {
        if !seen.insert(page.id) {
            report.push(
                ErrorCode::DuplicateId,
                ElementRef::Page(page.id),
                "page identity is used more than once",
            );
        }
    }
}

fn check_node_identity(document: &Document, report: &mut Report) {
    let mut seen = HashSet::new();
    for node in document.nodes() {
        if !seen.insert(node.id) {
            report.push(
                ErrorCode::DuplicateId,
                ElementRef::Node(node.id),
                "node identity is used more than once",
            );
        }
    }
}

fn check_asset_identity(document: &Document, report: &mut Report) {
    let mut seen = HashSet::new();
    for asset in document.assets() {
        if !seen.insert(asset.id) {
            report.push(
                ErrorCode::DuplicateId,
                ElementRef::Asset(asset.id),
                "asset identity is used more than once",
            );
        }
    }
}

fn check_references(document: &Document, report: &mut Report) {
    for page in document.pages() {
        let mut seen_roots = HashSet::new();
        for &root in page.roots() {
            if !seen_roots.insert(root) {
                report.push(
                    ErrorCode::DuplicateRoot,
                    ElementRef::Page(page.id),
                    format!("node {root} appears more than once in the page roots"),
                );
            }
            if !document.contains_node(root) {
                report.push(
                    ErrorCode::MissingNode,
                    ElementRef::Page(page.id),
                    format!("page root {root} does not exist"),
                );
            }
        }
    }

    for node in document.nodes() {
        let mut seen_children = HashSet::new();
        for &child in node.children() {
            if !seen_children.insert(child) {
                report.push(
                    ErrorCode::DuplicateChild,
                    ElementRef::Node(node.id),
                    format!("node {child} appears more than once in the children"),
                );
            }
            if !document.contains_node(child) {
                report.push(
                    ErrorCode::MissingNode,
                    ElementRef::Node(node.id),
                    format!("child {child} does not exist"),
                );
            }
        }
    }
}

/// Verifies that every non-root node has exactly one parent, and that no node
/// is owned as both a page root and a child.
fn check_parent_and_order(document: &Document, report: &mut Report) {
    let mut parent_count: std::collections::HashMap<NodeId, usize> =
        std::collections::HashMap::new();
    for node in document.nodes() {
        for &child in node.children() {
            *parent_count.entry(child).or_insert(0) += 1;
        }
    }

    let mut root_set = HashSet::new();
    for page in document.pages() {
        for &root in page.roots() {
            root_set.insert(root);
        }
    }

    for (&child, &count) in &parent_count {
        if count > 1 {
            report.push(
                ErrorCode::MultipleParents,
                ElementRef::Node(child),
                format!("node is claimed by {count} parents; a node has at most one"),
            );
        }
        if root_set.contains(&child) {
            report.push(
                ErrorCode::MultipleParents,
                ElementRef::Node(child),
                "node is both a page root and a child of another node",
            );
        }
    }

    check_orphans_and_cycles(document, &root_set, report);
}

/// Walks each page's tree to prove that every node is reachable from a root
/// and that no cycle exists.
fn check_orphans_and_cycles(document: &Document, root_set: &HashSet<NodeId>, report: &mut Report) {
    let mut reachable: HashSet<NodeId> = HashSet::new();

    for page in document.pages() {
        let mut on_path: Vec<NodeId> = Vec::new();
        let mut on_path_set: HashSet<NodeId> = HashSet::new();

        for &root in page.roots() {
            if !document.contains_node(root) {
                continue; // already reported as a missing node
            }
            walk(
                document,
                root,
                &mut on_path,
                &mut on_path_set,
                &mut reachable,
                report,
            );
        }
    }

    for node in document.nodes() {
        let id = node.id;
        if !reachable.contains(&id) {
            report.push(
                ErrorCode::OrphanNode,
                ElementRef::Node(id),
                "node is not reachable from any page root",
            );
        }
    }

    let _ = root_set;
}

fn walk(
    document: &Document,
    current: NodeId,
    on_path: &mut Vec<NodeId>,
    on_path_set: &mut HashSet<NodeId>,
    reachable: &mut HashSet<NodeId>,
    report: &mut Report,
) {
    if on_path_set.contains(&current) {
        report.push(
            ErrorCode::Cycle,
            ElementRef::Node(current),
            "node is its own ancestor through the children relation",
        );
        return;
    }

    if !reachable.insert(current) {
        report.push(
            ErrorCode::MultipleParents,
            ElementRef::Node(current),
            "node is reachable from more than one page root",
        );
        return;
    }

    on_path.push(current);
    on_path_set.insert(current);

    if let Some(node) = document.node(current) {
        let children: Vec<NodeId> = node.children().to_vec();
        for child in children {
            if document.contains_node(child) {
                walk(document, child, on_path, on_path_set, reachable, report);
            }
        }
    }

    on_path.pop();
    on_path_set.remove(&current);
}

fn check_child_kinds(document: &Document, report: &mut Report) {
    for node in document.nodes() {
        if !node.children().is_empty() && !node.kind.can_have_children() {
            report.push(
                ErrorCode::InvalidChildKind,
                ElementRef::Node(node.id),
                format!("a {} node cannot own children", node.kind.as_str()),
            );
        }
    }
}

fn check_props(document: &Document, report: &mut Report) {
    for node in document.nodes() {
        if let Err(error) = node.props.check(node.kind) {
            report.push(
                ErrorCode::InvalidProps,
                ElementRef::Node(node.id),
                error.to_string(),
            );
        }
        if let Some(asset) = node.props.asset()
            && document.asset(asset).is_none()
        {
            report.push(
                ErrorCode::MissingAsset,
                ElementRef::Node(node.id),
                format!("the image refers to missing asset {asset}"),
            );
        }
    }
}

fn check_depth(document: &Document, limits: ResourceLimits, report: &mut Report) {
    let Some(max_depth) = limits.max_depth else {
        return;
    };

    for page in document.pages() {
        for &root in page.roots() {
            if !document.contains_node(root) {
                continue;
            }
            let mut guard = HashSet::new();
            measure_depth(document, root, 0, max_depth, &mut guard, report);
        }
    }
}

fn measure_depth(
    document: &Document,
    current: NodeId,
    depth: usize,
    max_depth: usize,
    guard: &mut HashSet<NodeId>,
    report: &mut Report,
) {
    if depth > max_depth {
        report.push(
            ErrorCode::DepthLimit,
            ElementRef::Node(current),
            format!("tree depth exceeds the limit of {max_depth}"),
        );
        return;
    }

    // Guard re-entry so an existing cycle does not turn depth measurement into
    // infinite recursion; the cycle itself is reported by the tree walk.
    if !guard.insert(current) {
        return;
    }

    if let Some(node) = document.node(current) {
        let children: Vec<NodeId> = node.children().to_vec();
        for child in children {
            if document.contains_node(child) {
                measure_depth(document, child, depth + 1, max_depth, guard, report);
            }
        }
    }

    guard.remove(&current);
}

/// Verifies the derived index agrees with the canonical children and roots
/// lists. A disagreement means some code path bypassed the mutation surface.
fn check_derived_index(document: &Document, report: &mut Report) {
    for node in document.nodes() {
        for &child in node.children() {
            match document.parent_of(child) {
                Some(actual) if actual == node.id => {}
                Some(actual) => report.push(
                    ErrorCode::StaleIndex,
                    ElementRef::Node(child),
                    format!(
                        "derived parent is {actual}, canonical parent is {}",
                        node.id
                    ),
                ),
                None if document.contains_node(child) => report.push(
                    ErrorCode::StaleIndex,
                    ElementRef::Node(child),
                    format!("derived parent is missing; canonical parent is {}", node.id),
                ),
                None => {} // child does not exist; reported as a missing node
            }
        }
    }

    for page in document.pages() {
        for &root in page.roots() {
            match document.page_of(root) {
                Some(actual) if actual == page.id => {}
                Some(actual) => report.push(
                    ErrorCode::StaleIndex,
                    ElementRef::Node(root),
                    format!("derived page is {actual}, canonical page is {}", page.id),
                ),
                None => report.push(
                    ErrorCode::StaleIndex,
                    ElementRef::Node(root),
                    format!("derived page is missing; canonical page is {}", page.id),
                ),
            }
        }
    }

    for node in document.nodes() {
        if let Some(page) = document.page_of(node.id)
            && document.page(page).is_none()
        {
            report.push(
                ErrorCode::MissingPage,
                ElementRef::Node(node.id),
                format!("derived page {page} does not exist"),
            );
        }
    }
}

/// The schema revision this validator was written against.
///
/// Compared with a file's declared revision to reject a file that is newer
/// than the running kernel (see §8).
pub const SCHEMA_REVISION: u32 = 1;

/// Returns whether `file_schema` is a version this kernel can read.
///
/// A file declared newer than [`SCHEMA_REVISION`] must be rejected rather than
/// guessed at; a caller reports the dedicated "newer file version" error.
#[must_use]
pub const fn is_readable_schema(file_schema: u32) -> bool {
    file_schema <= SCHEMA_REVISION
}

/// A revision conflict between an expected and an actual document revision.
///
/// Returned when a caller applies a batch against a revision the document has
/// already moved past (see §6.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RevisionConflict {
    /// The revision the caller expected.
    pub expected: Revision,
    /// The revision the document actually holds.
    pub actual: Revision,
}

impl fmt::Display for RevisionConflict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "revision conflict: expected {}, document is at {}",
            self.expected, self.actual
        )
    }
}

impl std::error::Error for RevisionConflict {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Asset, Node, NodeKind, Page};

    fn leaf(kind: NodeKind, name: &str) -> Node {
        Node::new(NodeId::new(), kind, Some(name.to_owned()))
    }

    /// A sound document: one page holding one frame with two shapes.
    fn sound() -> (Document, PageId, NodeId) {
        let page_id = PageId::new();
        let frame = leaf(NodeKind::Frame, "frame");
        let a = leaf(NodeKind::Shape, "a");
        let b = leaf(NodeKind::Shape, "b");
        let frame_id = frame.id;

        let mut doc = Document::new();
        doc.insert_node(frame);
        doc.insert_node(a.clone());
        doc.insert_node(b.clone());
        doc.add_page(Page::new(page_id, "Page 1"));
        assert!(doc.append_child(frame_id, a.id));
        assert!(doc.append_child(frame_id, b.id));
        assert!(doc.append_root(page_id, frame_id));
        (doc, page_id, frame_id)
    }

    fn codes(errors: &ValidationErrors) -> Vec<ErrorCode> {
        errors.iter().map(|e| e.code).collect()
    }

    #[test]
    fn a_sound_document_validates() {
        let (doc, _, _) = sound();
        assert!(validate_untrusted(&doc).is_ok());
    }

    #[test]
    fn an_empty_document_validates() {
        assert!(validate_untrusted(&Document::new()).is_ok());
    }

    #[test]
    fn duplicate_page_identity_is_rejected() {
        let (mut doc, page_id, _) = sound();
        doc.add_page(Page::new(page_id, "Duplicate"));
        let errors = validate_shape(&doc).expect_err("must fail");
        assert!(codes(&errors).contains(&ErrorCode::DuplicateId));
    }

    #[test]
    fn duplicate_asset_identity_is_rejected() {
        let mut doc = Document::new();
        let id = AssetId::new();
        doc.add_asset(Asset {
            id,
            name: "a".to_owned(),
        });
        doc.add_asset(Asset {
            id,
            name: "b".to_owned(),
        });
        let errors = validate_shape(&doc).expect_err("must fail");
        assert_eq!(codes(&errors), vec![ErrorCode::DuplicateId]);
    }

    #[test]
    fn a_dangling_child_reference_is_rejected() {
        let (mut doc, _, frame) = sound();
        doc.insert_child_at(frame, 0, NodeId::new());
        let errors = validate_shape(&doc).expect_err("must fail");
        assert!(codes(&errors).contains(&ErrorCode::MissingNode));
    }

    #[test]
    fn a_dangling_root_reference_is_rejected() {
        let (mut doc, page_id, _) = sound();
        doc.append_root(page_id, NodeId::new());
        let errors = validate_shape(&doc).expect_err("must fail");
        assert!(codes(&errors).contains(&ErrorCode::MissingNode));
    }

    #[test]
    fn an_orphan_node_is_rejected() {
        let (mut doc, _, _) = sound();
        doc.insert_node(leaf(NodeKind::Shape, "orphan"));
        let errors = validate_shape(&doc).expect_err("must fail");
        assert!(codes(&errors).contains(&ErrorCode::OrphanNode));
    }

    #[test]
    fn a_leaf_owning_children_is_rejected() {
        let (mut doc, _, _) = sound();
        let shape = leaf(NodeKind::Shape, "shape");
        let child = leaf(NodeKind::Shape, "child");
        let shape_id = shape.id;
        doc.insert_node(shape);
        doc.insert_node(child.clone());
        assert!(doc.append_child(shape_id, child.id));
        assert!(doc.append_root(doc.pages()[0].id, shape_id));
        let errors = validate_shape(&doc).expect_err("must fail");
        assert!(codes(&errors).contains(&ErrorCode::InvalidChildKind));
    }

    #[test]
    fn a_stale_parent_index_is_rejected() {
        let (mut doc, _, frame) = sound();
        let named: Vec<NodeId> = doc.nodes().map(|n| n.id).collect();
        let child = named
            .into_iter()
            .find(|id| doc.parent_of(*id) == Some(frame))
            .expect("a child of the frame");
        doc.set_parent(child, NodeId::new());
        let errors = validate_shape(&doc).expect_err("must fail");
        assert!(codes(&errors).contains(&ErrorCode::StaleIndex));
    }

    #[test]
    fn a_missing_derived_parent_entry_is_rejected() {
        let (mut doc, _, frame) = sound();
        let child = doc
            .node(frame)
            .expect("frame")
            .children()
            .first()
            .copied()
            .expect("a child");
        doc.clear_parent(child);
        let errors = validate_shape(&doc).expect_err("must fail");
        assert!(codes(&errors).contains(&ErrorCode::StaleIndex));
    }

    #[test]
    fn a_stale_page_index_is_rejected() {
        let (mut doc, _, frame) = sound();
        let wrong = PageId::new();
        doc.set_page(frame, wrong);
        let errors = validate_shape(&doc).expect_err("must fail");
        assert!(
            codes(&errors).contains(&ErrorCode::StaleIndex),
            "got: {errors}"
        );
        assert!(
            codes(&errors).contains(&ErrorCode::MissingPage),
            "a derived page that does not exist must be reported: {errors}"
        );
    }

    #[test]
    fn a_missing_derived_page_entry_is_rejected() {
        let (mut doc, page_id, frame) = sound();
        assert!(doc.detach_root(page_id, frame));
        assert!(doc.append_root(page_id, frame));
        doc.clear_page(frame);
        let errors = validate_shape(&doc).expect_err("must fail");
        assert!(codes(&errors).contains(&ErrorCode::StaleIndex));
    }

    #[test]
    fn a_cycle_is_rejected() {
        // frame -> inner -> frame closes a loop while every node stays
        // reachable from the page root.
        let page_id = PageId::new();
        let mut doc = Document::new();
        doc.add_page(Page::new(page_id, "Page 1"));

        let frame = leaf(NodeKind::Frame, "frame");
        let inner = leaf(NodeKind::Frame, "inner");
        let (frame_id, inner_id) = (frame.id, inner.id);
        doc.insert_node(frame);
        doc.insert_node(inner);

        assert!(doc.append_root(page_id, frame_id));
        assert!(doc.append_child(frame_id, inner_id));
        assert!(doc.append_child(inner_id, frame_id));

        let errors = validate_shape(&doc).expect_err("must fail");
        assert!(codes(&errors).contains(&ErrorCode::Cycle), "got: {errors}");
    }

    #[test]
    fn a_node_owned_by_two_parents_is_rejected() {
        let (mut doc, _, frame) = sound();
        let extra = leaf(NodeKind::Frame, "extra");
        let extra_id = extra.id;
        let shared = doc
            .node(frame)
            .expect("frame")
            .children()
            .first()
            .copied()
            .expect("a child");

        doc.insert_node(extra);
        assert!(doc.append_root(doc.pages()[0].id, extra_id));
        assert!(doc.append_child(extra_id, shared));

        let errors = validate_shape(&doc).expect_err("must fail");
        assert!(
            codes(&errors).contains(&ErrorCode::MultipleParents),
            "got: {errors}"
        );
    }

    #[test]
    fn exceeding_the_node_limit_is_rejected() {
        let (doc, _, _) = sound();
        let limits = ResourceLimits {
            max_nodes: Some(1),
            ..ResourceLimits::UNLIMITED
        };
        let errors = validate(&doc, limits).expect_err("must fail");
        assert_eq!(codes(&errors), vec![ErrorCode::NodeCountLimit]);
    }

    #[test]
    fn exceeding_the_page_limit_is_rejected() {
        let (doc, _, _) = sound();
        let limits = ResourceLimits {
            max_pages: Some(0),
            ..ResourceLimits::UNLIMITED
        };
        let errors = validate(&doc, limits).expect_err("must fail");
        assert_eq!(codes(&errors), vec![ErrorCode::PageCountLimit]);
    }

    #[test]
    fn exceeding_the_depth_limit_is_rejected() {
        let mut doc = Document::new();
        let page_id = PageId::new();
        doc.add_page(Page::new(page_id, "Page 1"));

        // A chain of nested frames: depth 0, 1, 2, ...
        let mut parent: Option<NodeId> = None;
        for _ in 0..5 {
            let f = leaf(NodeKind::Frame, "frame");
            let id = f.id;
            doc.insert_node(f);
            match parent {
                Some(p) => assert!(doc.append_child(p, id)),
                None => assert!(doc.append_root(page_id, id)),
            }
            parent = Some(id);
        }

        let limits = ResourceLimits {
            max_depth: Some(2),
            ..ResourceLimits::UNLIMITED
        };
        let errors = validate(&doc, limits).expect_err("must fail");
        assert!(codes(&errors).contains(&ErrorCode::DepthLimit));
    }

    #[test]
    fn limits_of_none_reject_nothing() {
        let (doc, _, _) = sound();
        assert!(validate(&doc, ResourceLimits::UNLIMITED).is_ok());
    }

    #[test]
    fn a_document_within_the_limits_validates() {
        let (doc, _, _) = sound();
        assert!(validate_untrusted(&doc).is_ok());
    }

    #[test]
    fn error_code_strings_are_stable() {
        assert_eq!(ErrorCode::DuplicateId.as_str(), "duplicate-id");
        assert_eq!(ErrorCode::MissingNode.as_str(), "missing-node");
        assert_eq!(ErrorCode::Cycle.as_str(), "cycle");
        assert_eq!(ErrorCode::StaleIndex.as_str(), "stale-index");
        assert_eq!(ErrorCode::DepthLimit.as_str(), "depth-limit");
        assert_eq!(ErrorCode::DuplicateId.to_string(), "duplicate-id");
    }

    #[test]
    fn an_error_carries_its_code_and_subject() {
        let (mut doc, _, _) = sound();
        doc.insert_node(leaf(NodeKind::Shape, "orphan"));
        let errors = validate_shape(&doc).expect_err("must fail");
        let first = &errors.as_slice()[0];
        assert_eq!(first.code, ErrorCode::OrphanNode);
        assert!(matches!(first.subject, ElementRef::Node(_)));
        assert!(!first.message.is_empty());
    }

    #[test]
    fn every_violation_is_reported_not_just_the_first() {
        let (mut doc, _, _) = sound();
        doc.insert_node(leaf(NodeKind::Shape, "orphan-1"));
        doc.insert_node(leaf(NodeKind::Shape, "orphan-2"));
        let errors = validate_shape(&doc).expect_err("must fail");
        let orphans = errors
            .iter()
            .filter(|e| e.code == ErrorCode::OrphanNode)
            .count();
        assert_eq!(orphans, 2, "both orphans must be reported");
    }

    #[test]
    fn validation_errors_display_with_code_and_subject() {
        let (mut doc, _, _) = sound();
        doc.insert_node(leaf(NodeKind::Shape, "orphan"));
        let errors = validate_shape(&doc).expect_err("must fail");
        let text = errors.to_string();
        assert!(text.contains("orphan-node"), "got: {text}");
    }

    #[test]
    fn schema_revision_gate_rejects_a_newer_file() {
        assert!(is_readable_schema(1));
        assert!(is_readable_schema(0));
        assert!(!is_readable_schema(SCHEMA_REVISION + 1));
    }

    #[test]
    fn revision_conflict_reports_both_revisions() {
        let conflict = RevisionConflict {
            expected: Revision::from_raw(3),
            actual: Revision::from_raw(5),
        };
        let text = conflict.to_string();
        assert!(text.contains('3') && text.contains('5'), "got: {text}");
    }

    #[test]
    fn a_single_error_displays_without_a_count_prefix() {
        let error = ValidationError::new(ErrorCode::Cycle, ElementRef::Document, "self reference");
        let errors = ValidationErrors(vec![error]);
        assert_eq!(errors.len(), 1);
        assert!(!errors.is_empty());
        assert!(errors.to_string().starts_with('['), "got: {errors}");
    }
}
