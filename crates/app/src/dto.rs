//! The application's interface contract: requests in, views and results out.
//!
//! ## Why these are not the kernel's types
//!
//! The kernel's runtime types are not a wire contract, and the file schema is a
//! third thing again. A host that hands the kernel's structures to a frontend
//! makes every refactor a breaking interface change. These DTOs are the
//! deliberate, small surface an interface is allowed to depend on.
//!
//! ## Rules
//!
//! - Identities cross as strings, because JSON has no UUID type.
//! - Numbers are design units and must be finite; a value the kernel would
//!   reject is refused here rather than coerced.
//! - Field names serialize as `camelCase`, the convention a JavaScript host
//!   expects.
//! - A kind or enum crosses as its stable string name, not as a bare index, so
//!   adding a variant cannot silently change a meaning.

use serde::{Deserialize, Serialize};

/// An RGBA colour, eight bits per channel.
///
/// The document stores colour this way, so the contract does not invent a
/// second representation such as a CSS string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rgba {
    /// Red.
    pub r: u8,
    /// Green.
    pub g: u8,
    /// Blue.
    pub b: u8,
    /// Alpha; `255` is opaque.
    pub a: u8,
}

impl Rgba {
    /// An opaque colour.
    #[must_use]
    pub const fn opaque(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }
}

/// One page in the document tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageView {
    /// The page identity.
    pub id: String,
    /// The page name.
    pub name: String,
    /// The page's root nodes, in paint order.
    pub roots: Vec<String>,
}

/// A stroke, as the interface sees it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StrokeView {
    /// The stroke colour.
    pub color: Rgba,
    /// The stroke width in design units.
    pub width: f64,
}

/// The editable properties of one node.
///
/// A property the node's kind does not carry is `None` rather than a default
/// value, so an inspector can hide a field instead of offering a control that
/// would write a meaningless number.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PropsView {
    /// The stored size in design units.
    pub size: [f64; 2],
    /// The stored transform, `[a, b, c, d, e, f]`.
    pub transform: [f64; 6],
    /// How the width is decided: `fixed`, `fill`, or `hug`.
    pub width_sizing: String,
    /// How the height is decided: `fixed`, `fill`, or `hug`.
    pub height_sizing: String,
    /// The fill colour, when the node has one.
    pub fill: Option<Rgba>,
    /// The stroke, when the node has one.
    pub stroke: Option<StrokeView>,
    /// A shape's geometry name, for a shape node.
    pub shape_geometry: Option<String>,
    /// A shape's corner radius, for a shape node.
    pub corner_radius: Option<f64>,
    /// A text node's content.
    pub text_content: Option<String>,
    /// A text node's font family.
    pub font_family: Option<String>,
    /// A text node's font size in design units.
    pub font_size: Option<f64>,
    /// A text node's paragraph direction: `auto`, `ltr`, or `rtl`.
    pub text_direction: Option<String>,
    /// An image node's asset identity.
    pub image_asset: Option<String>,
    /// A frame node's layout name: `none` or `flex`.
    pub frame_layout: Option<String>,
}

/// One node in the document tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeView {
    /// The node identity.
    pub id: String,
    /// The node kind's stable name.
    pub kind: String,
    /// The node name.
    pub name: String,
    /// The page the node belongs to.
    pub page: String,
    /// The parent node, or `None` for a page root.
    pub parent: Option<String>,
    /// The node's children, in paint order.
    pub children: Vec<String>,
    /// The node's properties.
    pub props: PropsView,
}

/// A complete read view of the open document.
///
/// The nodes are ordered by page structure, which is also paint order, so an
/// interface can rely on the same order the renderer uses.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentView {
    /// The revision this view was taken at.
    pub revision: u64,
    /// Whether the document has unsaved changes.
    pub dirty: bool,
    /// The file name to show, or `None` for a document with no source yet.
    ///
    /// A display label only. The full path stays in the host: an interface does
    /// not need to know where a document lives, and handing it over would widen
    /// what a compromised page can learn.
    pub display_name: Option<String>,
    /// The pages, in order.
    pub pages: Vec<PageView>,
    /// Every node in the document.
    pub nodes: Vec<NodeView>,
}

/// A request to change the document.
///
/// The variant set is deliberately small: it is the interface's vocabulary, not
/// the kernel's. A host that needs a richer edit adds a variant here and maps it
/// to kernel commands, which keeps the mapping in one tested place.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
#[non_exhaustive]
pub enum EditCommand {
    /// Sets or clears a node's name.
    RenameNode {
        /// The node to rename.
        node: String,
        /// The new name, or `None` to clear it.
        name: Option<String>,
    },
    /// Sets or clears a node's fill colour.
    SetNodeFill {
        /// The node to change.
        node: String,
        /// The new fill, or `None` to remove it.
        fill: Option<Rgba>,
    },
}

/// One atomic batch of edits at an expected revision.
///
/// The revision is the caller's assertion about the state it was looking at. If
/// it does not match, nothing is attempted: a stale interface must reload rather
/// than overwrite a change it never saw.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditRequest {
    /// The revision the caller believes the document is at.
    pub expected_revision: u64,
    /// The commands to apply in one transaction.
    pub commands: Vec<EditCommand>,
}

/// What a committed batch changed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitSummary {
    /// The revision before the batch.
    pub previous_revision: u64,
    /// The revision after the batch.
    pub revision: u64,
    /// Whether the document now differs from its last saved state.
    pub dirty: bool,
    /// Whether there is a step to undo.
    pub can_undo: bool,
    /// Whether there is a step to redo.
    pub can_redo: bool,
    /// Nodes that did not exist before the batch.
    pub added_nodes: Vec<String>,
    /// Nodes that no longer exist.
    pub removed_nodes: Vec<String>,
    /// Nodes whose stored value changed.
    pub changed_nodes: Vec<String>,
    /// Pages a consumer must repaint or recompute.
    pub affected_pages: Vec<String>,
}

/// Where a page is laid out and how it is rasterized.
///
/// These are session choices, not document state: the page size is the artboard
/// the host is showing, and the scale is a zoom level. Neither is persisted.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewOptions {
    /// The artboard size in design units, or `None` to fit the content.
    pub page_size: Option<[f64; 2]>,
    /// Pixels per design unit.
    pub scale: f64,
    /// The opaque backdrop, `[r, g, b, a]`.
    pub background: [u8; 4],
}

impl Default for PreviewOptions {
    fn default() -> Self {
        Self {
            page_size: None,
            scale: 1.0,
            background: [255, 255, 255, 255],
        }
    }
}

/// A rendered page, encoded as a PNG.
///
/// The image crosses as bytes rather than as pixels in JSON: an RGBA array for
/// one page is several megabytes of numbers, which is the wrong thing to put on
/// a message bus. A host is expected to hand the bytes to the interface as a
/// binary payload or a local resource, not as a data URL.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    /// The revision this preview was produced from.
    ///
    /// A preview whose revision is behind the document must be discarded, not
    /// shown as if it were current.
    pub revision: u64,
    /// The page that was rendered.
    pub page: String,
    /// The artboard size in design units.
    pub page_size: [f64; 2],
    /// The output width in pixels.
    pub width: u32,
    /// The output height in pixels.
    pub height: u32,
    /// Pixels per design unit.
    pub scale: f64,
    /// The encoded PNG.
    #[serde(skip)]
    pub png: Vec<u8>,
    /// Cases the layout engine interpreted by a documented fallback.
    pub layout_diagnostics: Vec<String>,
    /// Content the renderer left out or substituted.
    pub render_diagnostics: Vec<String>,
}

/// One node's resolved geometry, in page units.
///
/// Geometry is derived, never stored in the document: a caller must treat it as
/// valid only for the revision it came with.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutNodeView {
    /// The node.
    pub id: String,
    /// The node's box in page units, as an axis-aligned rectangle:
    /// `[x, y, width, height]`.
    ///
    /// A rotated node's rectangle is the bounding box of its transformed box,
    /// which is what a selection overlay needs to draw.
    pub rect: [f64; 4],
    /// The node's resolved size in design units.
    pub size: [f64; 2],
    /// The composed transform from page space into the node's space,
    /// `[a, b, c, d, e, f]`.
    pub world: [f64; 6],
}

/// The geometry of one laid-out page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutView {
    /// The revision the geometry was computed at.
    pub revision: u64,
    /// The page that was laid out.
    pub page: String,
    /// The artboard size in design units, which may exceed the requested size
    /// when content overflowed it.
    pub page_size: [f64; 2],
    /// The nodes in paint order, so a caller can draw them back to front.
    pub nodes: Vec<LayoutNodeView>,
    /// Cases the layout engine interpreted by a documented fallback.
    pub diagnostics: Vec<String>,
}

/// The result of testing a point against a page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HitTestResult {
    /// The topmost node under the point.
    pub node: Option<NodeIdString>,
    /// The revision the hit test was computed against.
    pub revision: u64,
}

/// A node identity on the wire.
///
/// A newtype rather than a bare `String` only for documentation; it serializes
/// as the plain string.
pub type NodeIdString = String;
