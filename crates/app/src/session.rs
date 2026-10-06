//! The editor session: one open document, owned by the application layer.
//!
//! ## What this owns
//!
//! The session is the only place that holds a [`DocumentEngine`] for the
//! interface. It applies edits, runs undo and redo, produces read views, and
//! drives layout and render for a preview. Everything else — where a file lives,
//! which window shows the preview, what a dialog looks like — belongs to a host.
//!
//! ## Why one session and not a session manager
//!
//! Nothing in the current scope opens two documents at once. A map from identity
//! to session would be a guess about a future feature, and the migration from
//! this shape to that one is small. The session is therefore a single value.
//!
//! ## Dirty tracking
//!
//! "Dirty" means the document's canonical persisted bytes differ from the bytes
//! recorded at the last successful load or save. Counting edits would be wrong:
//! an undo that returns the document to its saved state leaves it clean, and a
//! redo that moves away from it makes it dirty again. The comparison is
//! therefore a byte comparison of the canonical form, cached per revision so a
//! view request does not re-serialize the document.
//!
//! ## What a preview is
//!
//! Layout and render are derived, revision-tagged, and never stored in the
//! document. A preview carries the revision it was produced from so a caller can
//! discard a stale image instead of showing it as current.

use std::sync::Arc;

use swotvibe_core::{
    AssetId, BatchError, Color, Command, Document, DocumentEngine, Node, NodeId, NodeKind, PageId,
    Revision, Scalar, Snapshot, validate_untrusted,
};
use swotvibe_format::{export, from_json, import, to_json};
use swotvibe_layout::{LayoutEngine, LayoutOptions, LayoutResult, TaffyLayoutEngine};
use swotvibe_render::{RenderAssets, RenderConfig, Renderer, VELLO_ENGINE_ID, VelloCpuRenderer};
use swotvibe_text::TextLayoutEngine;

use crate::dto::{
    CommitSummary, DocumentView, EditCommand, EditRequest, HitTestResult, LayoutNodeView,
    LayoutView, NodeView, PageView, Preview, PreviewOptions, PropsView, Rgba, StrokeView,
};
use crate::error::{AppError, AppErrorCode};

/// What a new session renders with before a caller states otherwise.
const DEFAULT_PREVIEW: PreviewOptions = PreviewOptions {
    page_size: Some([480.0, 320.0]),
    scale: 1.0,
    background: [255, 255, 255, 255],
};

/// The largest preview side this layer will ask for.
///
/// A mistaken scale must not allocate an unbounded buffer. The bound is far
/// above any interface preview and matches the renderer's own limit.
const MAX_PREVIEW_PIXELS_PER_SIDE: f64 = 8192.0;

/// The shared text engine a session measures with.
///
/// `Send + Sync` is part of the type because a session is held by a host: a
/// desktop shell keeps it in application state, which is shared across threads.
/// Requiring it here means an engine that cannot cross a thread is caught when a
/// host is written, not when it is first run.
pub type SharedTextEngine = Arc<dyn TextLayoutEngine + Send + Sync>;

/// One open document and everything the interface needs to edit it.
pub struct EditorSession {
    engine: DocumentEngine,
    text: SharedTextEngine,
    assets: Option<RenderAssets>,
    display_name: Option<String>,
    /// The canonical bytes last loaded or successfully saved.
    baseline: Option<Vec<u8>>,
    /// The canonical bytes for `cached_revision`, when they have been computed.
    cached: Option<(Revision, Vec<u8>)>,
    preview_defaults: PreviewOptions,
}

impl std::fmt::Debug for EditorSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EditorSession")
            .field("revision", &self.engine.revision())
            .field("display_name", &self.display_name)
            .field("has_baseline", &self.baseline.is_some())
            .finish_non_exhaustive()
    }
}

impl EditorSession {
    /// Opens a session over a new, empty document.
    ///
    /// A fresh document is not dirty: `saved` means "matches the bytes this
    /// session would write", and an empty document it has just created does.
    #[must_use]
    pub fn new(text: SharedTextEngine) -> Self {
        let mut session = Self {
            engine: DocumentEngine::new(),
            text,
            assets: None,
            display_name: None,
            baseline: None,
            cached: None,
            preview_defaults: DEFAULT_PREVIEW,
        };
        session.baseline = session.canonical().ok();
        session
    }

    /// Supplies encoded image resources for a document that references assets.
    #[must_use]
    pub fn with_assets(mut self, assets: RenderAssets) -> Self {
        self.assets = Some(assets);
        self
    }

    /// Replaces the preview defaults a caller does not override per request.
    pub fn set_preview_defaults(&mut self, options: PreviewOptions) {
        self.preview_defaults = options;
    }

    /// The document's current revision.
    #[must_use]
    pub const fn revision(&self) -> Revision {
        self.engine.revision()
    }

    /// Whether there is a step to undo.
    #[must_use]
    pub fn can_undo(&self) -> bool {
        self.engine.can_undo()
    }

    /// Whether there is a step to redo.
    #[must_use]
    pub fn can_redo(&self) -> bool {
        self.engine.can_redo()
    }

    /// The document's display name, when it has a source.
    #[must_use]
    pub fn display_name(&self) -> Option<&str> {
        self.display_name.as_deref()
    }

    /// Whether the document differs from its last loaded or saved state.
    #[must_use]
    pub fn is_dirty(&mut self) -> bool {
        match (self.canonical(), self.baseline.as_ref()) {
            (Ok(current), Some(baseline)) => current != *baseline,
            // Without a baseline there is nothing to compare against, so the
            // honest answer is that the state is unsaved.
            _ => true,
        }
    }

    /// Reads the open document.
    ///
    /// # Errors
    ///
    /// Returns [`AppErrorCode::Schema`] when the document cannot be projected
    /// into its persisted form, which would be a bug rather than bad input.
    pub fn view(&mut self) -> Result<DocumentView, AppError> {
        let dirty = self.is_dirty();
        let document = self.engine.document();
        let revision = document.revision();

        let pages: Vec<PageView> = document
            .pages()
            .iter()
            .map(|page| PageView {
                id: page.id.to_string(),
                name: page.name.clone(),
                roots: page.roots().iter().map(ToString::to_string).collect(),
            })
            .collect();

        let mut nodes = Vec::new();
        for page in document.pages() {
            for &root in page.roots() {
                self.collect_nodes(document, root, &mut nodes);
            }
        }

        Ok(DocumentView {
            revision: revision.as_u64(),
            dirty,
            display_name: self.display_name.clone(),
            pages,
            nodes,
        })
    }

    /// Walks one subtree in paint order, appending a view per node.
    ///
    /// Pre-order is the order the renderer paints, so an interface that uses
    /// this list shows layers in the same stacking the image has.
    fn collect_nodes(&self, document: &Document, id: NodeId, out: &mut Vec<NodeView>) {
        let Some(node) = document.node(id) else {
            return;
        };
        let Some(page) = document.page_of(id) else {
            return;
        };
        out.push(NodeView {
            id: node.id.to_string(),
            kind: node.kind.as_str().to_owned(),
            name: node.name().to_owned(),
            page: page.to_string(),
            parent: document.parent_of(id).map(|parent| parent.to_string()),
            children: node.children().iter().map(ToString::to_string).collect(),
            props: props_view(node),
        });
        for &child in node.children() {
            self.collect_nodes(document, child, out);
        }
    }

    /// Replaces the open document with one loaded from persisted JSON bytes.
    ///
    /// The load is all-or-nothing: a document that fails to parse, migrate,
    /// import, or validate leaves the session exactly as it was, and the undo
    /// timeline starts empty because session history is not part of the payload.
    ///
    /// The revision after a load is **not necessarily zero**. Revision is a
    /// session counter and is not persisted, so an imported document carries
    /// whatever revision the import itself produced. A caller must read the
    /// revision from the returned view and send that value back with its first
    /// edit, rather than assuming a freshly opened document starts at zero.
    ///
    /// # Errors
    ///
    /// Returns the mapped [`AppError`] for malformed JSON, an unsupported schema
    /// version, an exceeded read limit, or a document that breaks an integrity
    /// rule.
    pub fn open_json(
        &mut self,
        bytes: &[u8],
        display_name: Option<String>,
    ) -> Result<DocumentView, AppError> {
        let dto = from_json(bytes)?;
        let document = import(&dto)?;
        validate_untrusted(&document)?;

        let mut engine = DocumentEngine::with_document(document);
        let baseline = canonical_of(&mut engine)?;
        self.engine = engine;
        self.baseline = Some(baseline);
        self.cached = None;
        self.display_name = display_name;
        self.view()
    }

    /// Applies one atomic batch of edits.
    ///
    /// # Errors
    ///
    /// - [`AppErrorCode::RevisionConflict`] when `expected_revision` is not the
    ///   current revision; nothing is attempted.
    /// - [`AppErrorCode::InvalidRequest`] for an unparsable identity or a
    ///   non-finite number.
    /// - [`AppErrorCode::UnknownNode`] when a command names a node the document
    ///   does not contain.
    /// - [`AppErrorCode::CommandRejected`] for any other refusal by the kernel.
    pub fn apply(&mut self, request: &EditRequest) -> Result<CommitSummary, AppError> {
        let expected = Revision::from_raw(request.expected_revision);
        if expected != self.engine.revision() {
            return Err(AppError::from(BatchError::RevisionConflict {
                expected,
                actual: self.engine.revision(),
            }));
        }
        let commands = self.to_kernel_commands(&request.commands)?;
        self.commit(|engine| engine.apply(&commands).map_err(AppError::from))
    }

    /// Undoes the most recent step.
    ///
    /// # Errors
    ///
    /// As [`EditorSession::apply`], plus [`AppErrorCode::History`] when there is
    /// nothing to undo or the timeline is out of sync with the document.
    pub fn undo(&mut self, expected_revision: u64) -> Result<CommitSummary, AppError> {
        self.check_revision(expected_revision)?;
        self.commit(|engine| engine.undo().map_err(AppError::from))
    }

    /// Redoes the most recently undone step.
    ///
    /// # Errors
    ///
    /// As [`EditorSession::undo`], when there is nothing to redo.
    pub fn redo(&mut self, expected_revision: u64) -> Result<CommitSummary, AppError> {
        self.check_revision(expected_revision)?;
        self.commit(|engine| engine.redo().map_err(AppError::from))
    }

    /// The canonical bytes this session would write.
    ///
    /// A host writes these to a destination it owns. This does not mark the
    /// document as saved: that only happens in [`EditorSession::note_saved`],
    /// after the write actually succeeded.
    ///
    /// # Errors
    ///
    /// Returns [`AppErrorCode::Schema`] when the document cannot be projected
    /// into its persisted form.
    pub fn export_bytes(&mut self) -> Result<Vec<u8>, AppError> {
        self.canonical()
    }

    /// Records that the current bytes were written successfully.
    ///
    /// # Errors
    ///
    /// As [`EditorSession::export_bytes`].
    pub fn note_saved(&mut self) -> Result<(), AppError> {
        let bytes = self.canonical()?;
        self.baseline = Some(bytes);
        Ok(())
    }

    /// Records the file name to show for this document.
    pub fn set_display_name(&mut self, name: Option<String>) {
        self.display_name = name;
    }

    /// Lays out and rasterizes one page.
    ///
    /// # Errors
    ///
    /// [`AppErrorCode::UnknownPage`] for a page the document does not contain,
    /// [`AppErrorCode::InvalidRequest`] for an unusable scale or artboard size,
    /// and the mapped layout or render failure otherwise.
    pub fn preview(&self, page: PageId, options: PreviewOptions) -> Result<Preview, AppError> {
        let snapshot = self.snapshot();
        if snapshot.page(page).is_none() {
            return Err(AppError::on_page(
                AppErrorCode::UnknownPage,
                page,
                "the page is not in the document",
            ));
        }
        let scale = check_scale(options.scale)?;
        let layout = self.layout_page(&snapshot, page, options)?;
        let page_size = layout.page_size();
        let background = Color {
            r: options.background[0],
            g: options.background[1],
            b: options.background[2],
            a: options.background[3],
        };

        let renderer = match self.assets.clone() {
            Some(assets) => VelloCpuRenderer::new(self.adapter_text()).with_assets(assets),
            None => VelloCpuRenderer::new(self.adapter_text()),
        };
        let config = RenderConfig::for_page(page_size, scale, background);
        let rendered = renderer.render(&snapshot, page, &layout, &config)?;
        let png = rendered.image.to_png()?;

        Ok(Preview {
            revision: snapshot.revision().as_u64(),
            page: page.to_string(),
            page_size: [page_size.0, page_size.1],
            width: rendered.image.width(),
            height: rendered.image.height(),
            scale,
            png,
            layout_diagnostics: layout
                .diagnostics()
                .iter()
                .map(ToString::to_string)
                .collect(),
            render_diagnostics: rendered
                .diagnostics
                .iter()
                .map(ToString::to_string)
                .collect(),
        })
    }

    /// The resolved geometry of one page.
    ///
    /// This exists so a selection overlay can draw a node's box without
    /// reimplementing layout. The result is derived and revision-tagged, and it
    /// is never written back to the document.
    ///
    /// # Errors
    ///
    /// As [`EditorSession::hit_test`] without the point check.
    pub fn layout(&self, page: PageId, options: PreviewOptions) -> Result<LayoutView, AppError> {
        let snapshot = self.snapshot();
        let layout = self.layout_for(&snapshot, page, options)?;
        let page_size = layout.page_size();
        Ok(LayoutView {
            revision: snapshot.revision().as_u64(),
            page: page.to_string(),
            page_size: [page_size.0, page_size.1],
            nodes: layout
                .nodes()
                .map(|node| LayoutNodeView {
                    id: node.id.to_string(),
                    rect: [node.rect.x, node.rect.y, node.rect.width, node.rect.height],
                    size: [node.size.0, node.size.1],
                    world: node.world.coefficients(),
                })
                .collect(),
            diagnostics: layout
                .diagnostics()
                .iter()
                .map(ToString::to_string)
                .collect(),
        })
    }

    /// Returns the topmost node under a point in page units.
    ///
    /// The point is tested against each node's own box after undoing that node's
    /// world transform, which is exact for a rotated or scaled box rather than an
    /// approximation by its bounding rectangle. Nodes are tested back to front,
    /// so the node painted last wins — the one a user sees on top.
    ///
    /// # Errors
    ///
    /// [`AppErrorCode::UnknownPage`] for a page the document does not contain,
    /// [`AppErrorCode::InvalidRequest`] for a non-finite point or an unusable
    /// artboard size, and the mapped layout failure otherwise.
    pub fn hit_test(
        &self,
        page: PageId,
        point: (f64, f64),
        options: PreviewOptions,
    ) -> Result<HitTestResult, AppError> {
        if !point.0.is_finite() || !point.1.is_finite() {
            return Err(AppError::new(
                AppErrorCode::InvalidRequest,
                "the hit-test point must be a finite pair of design units",
            ));
        }
        let snapshot = self.snapshot();
        let layout = self.layout_for(&snapshot, page, options)?;
        let node = layout
            .order()
            .iter()
            .rev()
            .filter_map(|&id| layout.node(id).map(|node_layout| (id, node_layout)))
            .find(|(_, node_layout)| {
                let Ok(inverse) = node_layout.world.invert() else {
                    return false;
                };
                let local = inverse.apply(point.0, point.1);
                local.0 >= 0.0
                    && local.1 >= 0.0
                    && local.0 <= node_layout.size.0
                    && local.1 <= node_layout.size.1
            })
            .map(|(id, _)| id);

        Ok(HitTestResult {
            node: node.map(|id| id.to_string()),
            revision: snapshot.revision().as_u64(),
        })
    }

    /// The stored properties of one node, as the interface sees them.
    ///
    /// # Errors
    ///
    /// [`AppErrorCode::InvalidRequest`] when the identity does not parse, and
    /// [`AppErrorCode::UnknownNode`] when no such node exists.
    pub fn node_props(&self, node: &str) -> Result<PropsView, AppError> {
        let id = parse_node(node)?;
        self.engine
            .document()
            .node(id)
            .map(props_view)
            .ok_or_else(|| {
                AppError::on_node(
                    AppErrorCode::UnknownNode,
                    id,
                    "the node is not in the document",
                )
            })
    }

    /// The identity of the first page, or `None` for a document with no pages.
    #[must_use]
    pub fn first_page(&self) -> Option<PageId> {
        self.engine.document().pages().first().map(|page| page.id)
    }

    /// Takes an immutable, revision-tagged copy for a layout or render pass.
    #[must_use]
    pub fn snapshot(&self) -> Snapshot {
        Snapshot::of(self.engine.document())
    }

    /// The text engine as the adapter contracts name it.
    ///
    /// The session holds `Send + Sync` so a host can share it across threads;
    /// the adapters name the plain trait. The coercion here is what keeps the
    /// stronger requirement in the host layer instead of pushing it onto every
    /// adapter and every existing caller.
    fn adapter_text(&self) -> Arc<dyn TextLayoutEngine> {
        self.text.clone()
    }

    /// The stored document, for a host that needs to inspect it directly.
    #[must_use]
    pub const fn document(&self) -> &Document {
        self.engine.document()
    }

    /// The renderer and pipeline a preview is produced with.
    #[must_use]
    pub const fn renderer_id(&self) -> &'static str {
        VELLO_ENGINE_ID
    }

    fn check_revision(&self, expected: u64) -> Result<(), AppError> {
        let expected = Revision::from_raw(expected);
        if expected == self.engine.revision() {
            Ok(())
        } else {
            Err(AppError::from(BatchError::RevisionConflict {
                expected,
                actual: self.engine.revision(),
            }))
        }
    }

    /// Runs one committing operation and summarizes the result.
    ///
    /// A failure returns the kernel's error unchanged and leaves the cached
    /// bytes alone, so a rejected operation cannot make the session look saved.
    fn commit(
        &mut self,
        operation: impl FnOnce(&mut DocumentEngine) -> Result<swotvibe_core::Commit, AppError>,
    ) -> Result<CommitSummary, AppError> {
        let commit = operation(&mut self.engine)?;
        let change_set = commit.change_set().clone();
        self.cached = None;
        Ok(CommitSummary {
            previous_revision: change_set.previous_revision().as_u64(),
            revision: change_set.new_revision().as_u64(),
            dirty: self.is_dirty(),
            can_undo: self.engine.can_undo(),
            can_redo: self.engine.can_redo(),
            added_nodes: change_set.added_nodes().map(|id| id.to_string()).collect(),
            removed_nodes: change_set
                .removed_nodes()
                .map(|id| id.to_string())
                .collect(),
            changed_nodes: change_set
                .changed_nodes()
                .map(|id| id.to_string())
                .collect(),
            affected_pages: change_set
                .affected_pages()
                .iter()
                .map(ToString::to_string)
                .collect(),
        })
    }

    /// Translates interface commands into kernel commands.
    ///
    /// A property edit reads the node's current properties first, because the
    /// kernel replaces a node's properties as a whole. Doing that here keeps the
    /// read-modify-write in one place instead of asking every host to repeat it.
    fn to_kernel_commands(&self, commands: &[EditCommand]) -> Result<Vec<Command>, AppError> {
        let document = self.engine.document();
        let mut out = Vec::with_capacity(commands.len());
        for command in commands {
            match command {
                EditCommand::RenameNode { node, name } => {
                    let id = parse_node(node)?;
                    if document.node(id).is_none() {
                        return Err(AppError::on_node(
                            AppErrorCode::UnknownNode,
                            id,
                            "the node is not in the document",
                        ));
                    }
                    out.push(Command::RenameNode {
                        id,
                        name: name.clone(),
                    });
                }
                EditCommand::SetNodeFill { node, fill } => {
                    let id = parse_node(node)?;
                    let Some(existing) = document.node(id) else {
                        return Err(AppError::on_node(
                            AppErrorCode::UnknownNode,
                            id,
                            "the node is not in the document",
                        ));
                    };
                    let mut props = existing.props.clone();
                    props.fill = fill.map(|fill| Color {
                        r: fill.r,
                        g: fill.g,
                        b: fill.b,
                        a: fill.a,
                    });
                    out.push(Command::SetNodeProps {
                        id,
                        props: Box::new(props),
                    });
                }
            }
        }
        Ok(out)
    }

    /// The canonical persisted bytes, computed at most once per revision.
    fn canonical(&mut self) -> Result<Vec<u8>, AppError> {
        let revision = self.engine.revision();
        if let Some((cached_revision, bytes)) = &self.cached
            && *cached_revision == revision
        {
            return Ok(bytes.clone());
        }
        let bytes = canonical_of(&mut self.engine)?;
        self.cached = Some((revision, bytes.clone()));
        Ok(bytes)
    }

    /// Lays out a page, failing with a typed error when the page is unknown.
    ///
    /// Every caller needs the same page check, so it lives here once rather than
    /// being repeated — and repeated slightly differently — at each call site.
    fn layout_for(
        &self,
        snapshot: &Snapshot,
        page: PageId,
        options: PreviewOptions,
    ) -> Result<LayoutResult, AppError> {
        if snapshot.page(page).is_none() {
            return Err(AppError::on_page(
                AppErrorCode::UnknownPage,
                page,
                "the page is not in the document",
            ));
        }
        self.layout_page(snapshot, page, options)
    }

    fn layout_page(
        &self,
        snapshot: &Snapshot,
        page: PageId,
        options: PreviewOptions,
    ) -> Result<LayoutResult, AppError> {
        let page_size = match options.page_size {
            Some(size) => Some((
                check_side(size[0], "page width")?,
                check_side(size[1], "page height")?,
            )),
            None => self
                .preview_defaults
                .page_size
                .map(|size| (size[0], size[1])),
        };
        let options = LayoutOptions { page_size };
        let engine = TaffyLayoutEngine::new(self.adapter_text());
        Ok(engine.layout(snapshot, page, &options)?)
    }
}

/// Projects a document into its canonical persisted bytes.
fn canonical_of(engine: &mut DocumentEngine) -> Result<Vec<u8>, AppError> {
    let dto = export(engine.document());
    Ok(to_json(&dto)?)
}

/// Parses a node identity from the wire.
fn parse_node(value: &str) -> Result<NodeId, AppError> {
    value.parse().map_err(|error| {
        AppError::new(
            AppErrorCode::InvalidRequest,
            format!("`{value}` is not a node identity: {error}"),
        )
    })
}

/// Checks a preview scale and returns it.
fn check_scale(scale: f64) -> Result<f64, AppError> {
    if !scale.is_finite() || scale <= 0.0 {
        return Err(AppError::new(
            AppErrorCode::InvalidRequest,
            format!("the preview scale {scale} must be finite and positive"),
        ));
    }
    if scale > 64.0 {
        return Err(AppError::new(
            AppErrorCode::ResourceLimit,
            format!("the preview scale {scale} is above the supported maximum of 64"),
        ));
    }
    Ok(scale)
}

/// Checks one artboard side and returns it.
fn check_side(value: f64, what: &str) -> Result<f64, AppError> {
    if !value.is_finite() || value <= 0.0 {
        return Err(AppError::new(
            AppErrorCode::InvalidRequest,
            format!("the {what} {value} must be finite and positive"),
        ));
    }
    if value > MAX_PREVIEW_PIXELS_PER_SIDE {
        return Err(AppError::new(
            AppErrorCode::ResourceLimit,
            format!(
                "the {what} {value} is above the supported maximum of {MAX_PREVIEW_PIXELS_PER_SIDE}"
            ),
        ));
    }
    Ok(value)
}

/// Builds the interface's view of one node's properties.
fn props_view(node: &Node) -> PropsView {
    let props = &node.props;
    let (shape_geometry, corner_radius) = match &props.content {
        swotvibe_core::Content::Shape(shape) => (
            Some(shape_geometry_name(shape.geometry).to_owned()),
            Some(shape.corner_radius.get()),
        ),
        _ => (None, None),
    };
    let (text_content, font_family, font_size, text_direction) = match &props.content {
        swotvibe_core::Content::Text(text) => (
            Some(text.content.clone()),
            Some(text.font_family.clone()),
            Some(text.font_size.get()),
            Some(direction_name(text.direction).to_owned()),
        ),
        _ => (None, None, None, None),
    };
    let image_asset = match &props.content {
        swotvibe_core::Content::Image(image) => image.asset.map(|asset| asset.to_string()),
        _ => None,
    };
    let frame_layout = match &props.content {
        swotvibe_core::Content::Frame(frame) => Some(frame_layout_name(frame.layout).to_owned()),
        _ => None,
    };

    PropsView {
        size: [props.size.width(), props.size.height()],
        transform: props.transform.coefficients(),
        width_sizing: sizing_name(props.width_sizing).to_owned(),
        height_sizing: sizing_name(props.height_sizing).to_owned(),
        fill: props.fill.map(rgba_of),
        stroke: props.stroke.map(|stroke| StrokeView {
            color: rgba_of(stroke.color),
            width: stroke.width.get(),
        }),
        shape_geometry,
        corner_radius,
        text_content,
        font_family,
        font_size,
        text_direction,
        image_asset,
        frame_layout,
    }
}

fn rgba_of(color: Color) -> Rgba {
    Rgba {
        r: color.r,
        g: color.g,
        b: color.b,
        a: color.a,
    }
}

/// The wire name of a sizing rule. Stable, like a kind name.
const fn sizing_name(sizing: swotvibe_core::Sizing) -> &'static str {
    match sizing {
        swotvibe_core::Sizing::Fixed => "fixed",
        swotvibe_core::Sizing::Fill => "fill",
        swotvibe_core::Sizing::Hug => "hug",
    }
}

const fn shape_geometry_name(geometry: swotvibe_core::ShapeGeometry) -> &'static str {
    match geometry {
        swotvibe_core::ShapeGeometry::Rect => "rect",
        swotvibe_core::ShapeGeometry::Ellipse => "ellipse",
    }
}

const fn direction_name(direction: swotvibe_core::TextDirection) -> &'static str {
    match direction {
        swotvibe_core::TextDirection::Auto => "auto",
        swotvibe_core::TextDirection::Ltr => "ltr",
        swotvibe_core::TextDirection::Rtl => "rtl",
    }
}

const fn frame_layout_name(layout: swotvibe_core::FrameLayout) -> &'static str {
    match layout {
        swotvibe_core::FrameLayout::None => "none",
        swotvibe_core::FrameLayout::Flex(_) => "flex",
    }
}

/// Whether a kind can be selected in the interface.
///
/// Every kind in the current schema can, so this exists to state the intent
/// rather than to filter: a future kind that is not selectable is added here.
#[must_use]
pub const fn is_selectable(kind: NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::Group | NodeKind::Frame | NodeKind::Shape | NodeKind::Text | NodeKind::Image
    )
}

/// A scalar the kernel will accept, or an error naming what was wrong.
///
/// Kept for hosts that build a property from user input and want the kernel's
/// own rule enforced before they commit to a command.
///
/// # Errors
///
/// [`AppErrorCode::InvalidRequest`] for a value the kernel would refuse.
pub fn checked_scalar(value: f64, what: &str) -> Result<Scalar, AppError> {
    Scalar::new(value).map_err(|error| {
        AppError::new(
            AppErrorCode::InvalidRequest,
            format!("the {what} {value} is not a usable design-unit value: {error}"),
        )
    })
}

/// Checks that an asset identity is one this session can resolve.
///
/// A host uses this before it reports an image as available, so "the asset is
/// registered" is answered by one place rather than by each caller.
#[must_use]
pub fn asset_is_registered(document: &Document, asset: AssetId) -> bool {
    document.asset(asset).is_some()
}
