//! The Taffy-backed layout engine: the temporary layout backend of the M0 slice.
//!
//! Taffy owns flexbox and box geometry; this module owns the translation of the
//! document's semantics into a Taffy tree and back, and the rules that make the
//! translation predictable:
//!
//! - **Design units, no rounding.** Taffy's pixel rounding is left off, so no
//!   value is silently snapped to a whole unit. A caller that wants pixels
//!   rounds at the renderer's boundary.
//! - **A node's position comes from layout, its shape from its transform.** The
//!   linear part of a node's stored transform (rotation, scale, skew) is applied
//!   as a transform; the translation is only used where layout does not decide
//!   the position — that is, inside a container with no layout rule. That is
//!   exactly the rule `NodeProps` documents for a flex child.
//! - **Hug measures text through the text contract.** A text leaf's size comes
//!   from [`TextLayoutEngine`], never from a character-count estimate, so the
//!   measurement and the render agree by construction.
//! - **A fallback is reported.** A sizing rule with no meaning for its node
//!   kind becomes a diagnostic instead of being applied in silence.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::Arc;

use swotvibe_core::{
    Axis, Content, CrossAlign, FrameLayout, MainAlign, NodeId, NodeKind, NodeProps, PageId, Sizing,
    Snapshot, TextAlign, TextDirection, Transform,
};
use swotvibe_text::{TextLayoutEngine, TextRequest};
use taffy::prelude::{
    Dimension, JustifyContent, LengthPercentage, LengthPercentageAuto, Rect as TaffyRect, Size,
};
use taffy::{
    AlignItems, AlignSelf, AvailableSpace, Display, FlexDirection, Position, Style, TaffyTree,
};

use crate::TAFFY_ENGINE_ID;
use crate::contract::{
    LayoutDiagnostic, LayoutEngine, LayoutError, LayoutFingerprint, LayoutOptions, LayoutResult,
    NodeLayout, Rect,
};

/// A layout engine built on Taffy 0.14.
///
/// See the module documentation for the rules this adapter follows.
pub struct TaffyLayoutEngine {
    text: Arc<dyn TextLayoutEngine>,
}

impl TaffyLayoutEngine {
    /// Builds an engine that measures text through `text`.
    ///
    /// The text engine is shared rather than owned, because the renderer has to
    /// shape and draw the same runs the layout measured; one engine behind one
    /// font set is what keeps the two from disagreeing.
    #[must_use]
    pub fn new(text: Arc<dyn TextLayoutEngine>) -> Self {
        Self { text }
    }

    /// The text engine this layout measures with.
    #[must_use]
    pub fn text_engine(&self) -> &Arc<dyn TextLayoutEngine> {
        &self.text
    }
}

/// The per-node data Taffy hands back to the measure function.
struct LeafContext<'a> {
    node: NodeId,
    /// The text to measure, borrowed from the snapshot the pass borrows.
    text: Option<TextSpec<'a>>,
}

/// The parts of a text node's style that measurement needs.
#[derive(Clone, Copy)]
struct TextSpec<'a> {
    text: &'a str,
    font_family: &'a str,
    font_size: f64,
    font_weight: u16,
    direction: TextDirection,
    align: TextAlign,
}

/// How a container positions its children, which decides how a child is styled.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Flow {
    /// No layout rule: a child's own transform says where it goes.
    Absolute,
    /// Flexbox along `Axis`.
    Flex(Axis),
}

fn flow_of(props: &NodeProps) -> Flow {
    match &props.content {
        Content::Frame(frame) => match frame.layout {
            FrameLayout::None => Flow::Absolute,
            FrameLayout::Flex(flex) => Flow::Flex(flex.direction),
        },
        // A group and every leaf kind position their children by transform; a
        // leaf cannot own children at all.
        _ => Flow::Absolute,
    }
}

impl LayoutEngine for TaffyLayoutEngine {
    fn engine_id(&self) -> &'static str {
        TAFFY_ENGINE_ID
    }

    fn layout(
        &self,
        snapshot: &Snapshot,
        page: PageId,
        options: &LayoutOptions,
    ) -> Result<LayoutResult, LayoutError> {
        options.check()?;
        let Some(page_node) = snapshot.page(page) else {
            return Err(LayoutError::PageNotFound { page });
        };

        // Structure order first, so both the build and the read-back pass walk
        // the document once in the same order the report is written in.
        let (pre_order, post_order) = walk(snapshot, page_node.roots().to_vec())?;

        let mut tree: TaffyTree<LeafContext<'_>> = TaffyTree::new();
        tree.disable_rounding();

        let mut diagnostics: Vec<LayoutDiagnostic> = Vec::new();
        let mut taffy_ids: BTreeMap<NodeId, taffy::NodeId> = BTreeMap::new();

        for id in &post_order {
            let node = snapshot
                .node(*id)
                .ok_or(LayoutError::NodeNotFound { node: *id })?;
            let flow = snapshot
                .parent_of(*id)
                .and_then(|parent| snapshot.node(parent))
                .map_or(Flow::Absolute, |parent| flow_of(&parent.props));
            let style = node_style(*id, snapshot, flow, &*self.text, &mut diagnostics)?;
            let text = text_spec(node);
            let children: Vec<taffy::NodeId> = node
                .children()
                .iter()
                .filter_map(|child| taffy_ids.get(child).copied())
                .collect();

            let taffy_id = tree
                .new_with_children(style, &children)
                .map_err(backend_error)?;
            tree.set_node_context(taffy_id, Some(LeafContext { node: *id, text }))
                .map_err(backend_error)?;
            taffy_ids.insert(*id, taffy_id);
        }

        let page_size = options.effective_page_size();
        let root_ids: Vec<taffy::NodeId> = page_node
            .roots()
            .iter()
            .filter_map(|root| taffy_ids.get(root).copied())
            .collect();
        let page_style = Style {
            display: Display::Block,
            size: Size {
                width: Dimension::length(page_size.0 as f32),
                height: Dimension::length(page_size.1 as f32),
            },
            ..Style::DEFAULT
        };
        let page_root = tree
            .new_with_children(page_style, &root_ids)
            .map_err(backend_error)?;

        let failure: RefCell<Option<LayoutError>> = RefCell::new(None);
        tree.compute_layout_with_measure(
            page_root,
            Size {
                width: AvailableSpace::Definite(page_size.0 as f32),
                height: AvailableSpace::Definite(page_size.1 as f32),
            },
            |inputs, _node, context, style| {
                let Some(context) = context else {
                    return taffy::compute_leaf_layout(
                        inputs,
                        style,
                        |_, _| 0.0,
                        |_, _| Size::ZERO,
                    );
                };
                taffy::compute_leaf_layout(
                    inputs,
                    style,
                    |_, _| 0.0,
                    |known, available| {
                        let Some(spec) = context.text else {
                            // A shape or an image has no content to measure: its size
                            // comes from its style.
                            return Size::ZERO;
                        };
                        let max_width = known
                            .width
                            .or_else(|| definite(available.width))
                            .map(f64::from);
                        let request = TextRequest {
                            max_width,
                            ..spec.request()
                        };
                        match self.text.shape(&request) {
                            Ok(shaped) => Size {
                                width: shaped.width as f32,
                                height: shaped.height as f32,
                            },
                            Err(error) => {
                                let mut failure = failure.borrow_mut();
                                if failure.is_none() {
                                    *failure = Some(LayoutError::Text {
                                        node: context.node,
                                        message: error.to_string(),
                                    });
                                }
                                // The pass fails below; a zero size here only keeps
                                // Taffy from panicking mid-pass.
                                Size::ZERO
                            }
                        }
                    },
                )
            },
        )
        .map_err(backend_error)?;

        if let Some(error) = failure.into_inner() {
            return Err(error);
        }

        let mut nodes: BTreeMap<NodeId, NodeLayout> = BTreeMap::new();
        let mut world_of_parent: BTreeMap<NodeId, Transform> = BTreeMap::new();
        let mut content: Option<Rect> = None;
        let mut content_size = (0.0f64, 0.0f64);

        for id in &pre_order {
            let taffy_id = taffy_ids[id];
            let layout = tree.layout(taffy_id).map_err(backend_error)?;
            let offset = (f64::from(layout.location.x), f64::from(layout.location.y));
            let size = (f64::from(layout.size.width), f64::from(layout.size.height));

            let node = snapshot
                .node(*id)
                .ok_or(LayoutError::NodeNotFound { node: *id })?;
            let transform = effective_transform(&node.props.transform, offset)?;
            let parent_world = snapshot
                .parent_of(*id)
                .and_then(|parent| world_of_parent.get(&parent).copied())
                .unwrap_or(Transform::IDENTITY);
            let world = parent_world
                .compose(&transform)
                .map_err(|error| LayoutError::Backend {
                    message: format!("the transform of {id} could not be composed: {error}"),
                })?;

            let rect = bounding_box(&world, size);
            content = Some(match content {
                Some(accumulated) => accumulated.union(&rect),
                None => rect,
            });
            content_size = (
                content_size.0.max(rect.right()),
                content_size.1.max(rect.bottom()),
            );

            world_of_parent.insert(*id, world);
            nodes.insert(
                *id,
                NodeLayout {
                    id: *id,
                    offset,
                    size,
                    transform,
                    world,
                    rect,
                },
            );
        }

        // The reported page size grows to hold the content, so a caller drawing
        // the result never has to guess whether anything fell outside.
        let page_size = (
            page_size.0.max(content_size.0.max(0.0)),
            page_size.1.max(content_size.1.max(0.0)),
        );

        let fingerprint = LayoutFingerprint {
            engine: TAFFY_ENGINE_ID.to_owned(),
            units: UNITS,
            page_size,
            rounded: false,
            fonts: self.text.fonts().fingerprint(),
        };

        Ok(LayoutResult::new(
            page,
            page_size,
            pre_order,
            nodes,
            diagnostics,
            fingerprint,
        ))
    }
}

/// The unit every number in a layout result is expressed in.
pub const UNITS: &str = "design units";

/// Walks a page's subtrees once, returning a pre-order list and a post-order
/// list.
///
/// Iterative on purpose: a document is only bounded by the validator's depth
/// limit, and a walk that recurses on input turns that limit into a stack
/// requirement.
fn walk(
    snapshot: &Snapshot,
    roots: Vec<NodeId>,
) -> Result<(Vec<NodeId>, Vec<NodeId>), LayoutError> {
    let mut pre_order: Vec<NodeId> = Vec::new();
    let mut post_order: Vec<NodeId> = Vec::new();
    let mut stack: Vec<(NodeId, bool)> = roots.into_iter().rev().map(|id| (id, false)).collect();

    while let Some((id, expanded)) = stack.pop() {
        if expanded {
            post_order.push(id);
            continue;
        }
        let node = snapshot
            .node(id)
            .ok_or(LayoutError::NodeNotFound { node: id })?;
        pre_order.push(id);
        stack.push((id, true));
        for child in node.children().iter().rev() {
            stack.push((*child, false));
        }
    }
    Ok((pre_order, post_order))
}

/// The measure specification of a text node, or `None` for every other kind.
fn text_spec(node: &swotvibe_core::Node) -> Option<TextSpec<'_>> {
    match &node.props.content {
        Content::Text(text) if node.kind == NodeKind::Text => Some(TextSpec {
            text: &text.content,
            font_family: &text.font_family,
            font_size: text.font_size.get(),
            font_weight: text.font_weight,
            direction: text.direction,
            align: text.align,
        }),
        _ => None,
    }
}

impl<'a> TextSpec<'a> {
    /// The specification as a request, with the wrap width left to the caller.
    fn request(&self) -> TextRequest<'a> {
        TextRequest {
            text: self.text,
            font_family: self.font_family,
            font_size: self.font_size,
            font_weight: self.font_weight,
            direction: self.direction,
            align: self.align,
            max_width: None,
        }
    }
}

/// Builds the Taffy style of one node.
fn node_style(
    id: NodeId,
    snapshot: &Snapshot,
    flow: Flow,
    text_engine: &dyn TextLayoutEngine,
    diagnostics: &mut Vec<LayoutDiagnostic>,
) -> Result<Style, LayoutError> {
    let Some(node) = snapshot.node(id) else {
        return Err(LayoutError::NodeNotFound { node: id });
    };
    let props = &node.props;
    let is_flex_container = matches!(
        &props.content,
        Content::Frame(frame) if matches!(frame.layout, FrameLayout::Flex(_))
    );
    let mut style = Style {
        display: Display::Block,
        ..Style::DEFAULT
    };

    match flow {
        Flow::Absolute => {
            // No layout rule in the parent: the node's own transform translation
            // is its position, which Taffy expresses as an absolute inset.
            let (x, y) = translation(&props.transform);
            style.position = Position::Absolute;
            style.inset = TaffyRect {
                left: LengthPercentageAuto::length(x as f32),
                right: LengthPercentageAuto::auto(),
                top: LengthPercentageAuto::length(y as f32),
                bottom: LengthPercentageAuto::auto(),
            };
        }
        Flow::Flex(axis) => {
            style.position = Position::Relative;
            apply_flex_child_sizing(&mut style, props, axis);
        }
    }

    apply_size(
        &mut style,
        props,
        flow,
        node.kind,
        is_flex_container,
        id,
        diagnostics,
    );

    // An absolutely positioned box has no container edge to shrink against, so
    // Taffy fills the available width for an automatic one. A hug text leaf is
    // therefore measured here and given an explicit box, which is also what
    // makes its width the intrinsic one rather than the page's.
    if flow == Flow::Absolute
        && let Some(spec) = text_spec(node)
    {
        let request = TextRequest {
            max_width: None,
            ..spec.request()
        };
        let shaped = text_engine
            .shape(&request)
            .map_err(|error| LayoutError::Text {
                node: id,
                message: error.to_string(),
            })?;
        if props.width_sizing == Sizing::Hug {
            style.size.width = Dimension::length(shaped.width as f32);
        }
        if props.height_sizing == Sizing::Hug {
            style.size.height = Dimension::length(shaped.height as f32);
        }
    }

    if let Content::Frame(frame) = &props.content
        && let FrameLayout::Flex(flex) = &frame.layout
    {
        style.display = Display::Flex;
        style.flex_direction = match flex.direction {
            Axis::Row => FlexDirection::Row,
            Axis::Column => FlexDirection::Column,
        };
        style.gap = Size {
            width: LengthPercentage::length(flex.gap.get() as f32),
            height: LengthPercentage::length(flex.gap.get() as f32),
        };
        style.padding = TaffyRect {
            left: LengthPercentage::length(flex.padding.left.get() as f32),
            right: LengthPercentage::length(flex.padding.right.get() as f32),
            top: LengthPercentage::length(flex.padding.top.get() as f32),
            bottom: LengthPercentage::length(flex.padding.bottom.get() as f32),
        };
        style.justify_content = Some(match flex.main_align {
            MainAlign::Start => JustifyContent::START,
            MainAlign::Center => JustifyContent::CENTER,
            MainAlign::End => JustifyContent::END,
            MainAlign::SpaceBetween => JustifyContent::SPACE_BETWEEN,
        });
        style.align_items = Some(match flex.cross_align {
            CrossAlign::Start => AlignItems::START,
            CrossAlign::Center => AlignItems::CENTER,
            CrossAlign::End => AlignItems::END,
            CrossAlign::Stretch => AlignItems::STRETCH,
        });
    }

    Ok(style)
}

/// A fill child takes the free space along the parent's main axis and stretches
/// across it.
fn apply_flex_child_sizing(style: &mut Style, props: &NodeProps, axis: Axis) {
    let (main, cross) = match axis {
        Axis::Row => (props.width_sizing, props.height_sizing),
        Axis::Column => (props.height_sizing, props.width_sizing),
    };
    if main == Sizing::Fill {
        style.flex_grow = 1.0;
        style.flex_basis = Dimension::length(0.0);
    }
    if cross == Sizing::Fill {
        style.align_self = Some(AlignSelf::STRETCH);
    }
}

/// Applies the stored size, the sizing rules, and the fallbacks they need.
fn apply_size(
    style: &mut Style,
    props: &NodeProps,
    flow: Flow,
    kind: NodeKind,
    is_flex_container: bool,
    id: NodeId,
    diagnostics: &mut Vec<LayoutDiagnostic>,
) {
    let size = [props.size.width(), props.size.height()];
    let sizing = [props.width_sizing, props.height_sizing];
    // A fill is decided by the flex rules, and only along the axis the flex
    // rules act on: main-axis fill is handled by `flex_grow`, cross-axis fill by
    // alignment, and either way the size itself stays automatic.
    let filled_by_flex = match flow {
        Flow::Absolute => [false, false],
        Flow::Flex(axis) => match axis {
            Axis::Row => [sizing[0] == Sizing::Fill, false],
            Axis::Column => [false, sizing[1] == Sizing::Fill],
        },
    };

    // Hug means "fit the content". Only two things have content a box can fit: a
    // text leaf, whose size is measured, and a flex container, whose size comes
    // from its children. Anywhere else the rule is replaced by the stored size
    // and the substitution is reported, because a container whose children are
    // positioned by their own transforms has no content flow to fit.
    let hugging_is_meaningful = kind == NodeKind::Text || is_flex_container;

    let mut dimension = |index: usize| -> Dimension {
        match sizing[index] {
            Sizing::Fixed => Dimension::length(size[index] as f32),
            Sizing::Hug if hugging_is_meaningful => Dimension::auto(),
            Sizing::Hug => {
                diagnostics.push(LayoutDiagnostic::SizingFallback {
                    node: id,
                    kind,
                    sizing: Sizing::Hug,
                });
                Dimension::length(size[index] as f32)
            }
            // Fill is decided by the flex rules; where there is no flex context
            // it behaves as a fixed size, which `NodeProps` already documents.
            Sizing::Fill => {
                if filled_by_flex[index] {
                    Dimension::auto()
                } else {
                    Dimension::length(size[index] as f32)
                }
            }
        }
    };

    style.size = Size {
        width: dimension(0),
        height: dimension(1),
    };
}

/// The node's effective local transform: the layout's offset with the stored
/// transform's linear part.
fn effective_transform(stored: &Transform, offset: (f64, f64)) -> Result<Transform, LayoutError> {
    let [a, b, c, d, _, _] = stored.coefficients();
    Transform::new([a, b, c, d, offset.0, offset.1]).map_err(|error| LayoutError::Backend {
        message: format!("a layout offset produced an invalid transform: {error}"),
    })
}

fn translation(transform: &Transform) -> (f64, f64) {
    let [_, _, _, _, e, f] = transform.coefficients();
    (e, f)
}

/// The axis-aligned bounding box of a box transformed into page space.
fn bounding_box(world: &Transform, size: (f64, f64)) -> Rect {
    let corners = [
        world.apply(0.0, 0.0),
        world.apply(size.0, 0.0),
        world.apply(0.0, size.1),
        world.apply(size.0, size.1),
    ];
    let min_x = corners
        .iter()
        .map(|point| point.0)
        .fold(f64::INFINITY, f64::min);
    let max_x = corners
        .iter()
        .map(|point| point.0)
        .fold(f64::NEG_INFINITY, f64::max);
    let min_y = corners
        .iter()
        .map(|point| point.1)
        .fold(f64::INFINITY, f64::min);
    let max_y = corners
        .iter()
        .map(|point| point.1)
        .fold(f64::NEG_INFINITY, f64::max);
    Rect {
        x: min_x,
        y: min_y,
        width: max_x - min_x,
        height: max_y - min_y,
    }
}

fn definite(space: AvailableSpace) -> Option<f32> {
    match space {
        AvailableSpace::Definite(value) => Some(value),
        AvailableSpace::MinContent | AvailableSpace::MaxContent => None,
    }
}

fn backend_error(error: impl std::fmt::Display) -> LayoutError {
    LayoutError::Backend {
        message: error.to_string(),
    }
}
