//! Node properties: geometry, paint, text, image, and frame layout.
//!
//! Ownership: the *meaning* of the visual properties a node carries. Not
//! owned: how they are shaped, laid out, or drawn; adapters read these values
//! through [`crate::Snapshot`] and translate them into their own types.
//!
//! All numbers are finite design units (see [`crate::geometry`]). Properties
//! are replaced as a whole by [`crate::Command::SetNodeProps`], which makes
//! one edit a single, trivially invertible step: the inverse is the previous
//! value.
//!
//! The kind-specific part lives in [`Content`], whose variant must match the
//! node's [`NodeKind`]. [`NodeProps::check`] states that rule once; both the
//! command layer and the validator call it.

use std::fmt;

use crate::geometry::{Scalar, Size, Transform};
use crate::ids::AssetId;
use crate::model::NodeKind;

/// The longest text content a node may hold, in bytes.
pub const MAX_TEXT_BYTES: usize = 1 << 20;

/// The longest font family name, in bytes.
pub const MAX_FONT_FAMILY_BYTES: usize = 256;

/// An sRGB colour with straight (non-premultiplied) alpha.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Color {
    /// Red, `0..=255`.
    pub r: u8,
    /// Green, `0..=255`.
    pub g: u8,
    /// Blue, `0..=255`.
    pub b: u8,
    /// Alpha, `0` transparent to `255` opaque.
    pub a: u8,
}

impl Color {
    /// An opaque colour.
    #[must_use]
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    /// A colour with explicit alpha.
    #[must_use]
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// The colour as `[r, g, b, a]`.
    #[must_use]
    pub const fn to_array(self) -> [u8; 4] {
        [self.r, self.g, self.b, self.a]
    }
}

/// A stroke around a shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stroke {
    /// The stroke colour.
    pub color: Color,
    /// The stroke width in design units. Not negative.
    pub width: Scalar,
}

/// How a node's size along one axis is decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Sizing {
    /// The size stored in [`NodeProps::size`].
    #[default]
    Fixed,
    /// Take the share of the parent's free space (flex grow); in a frame
    /// without layout this behaves like [`Self::Fixed`].
    Fill,
    /// Fit the content: the measured text, or the children's extent.
    Hug,
}

/// The geometry of a shape node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ShapeGeometry {
    /// A rectangle, optionally with rounded corners.
    #[default]
    Rect,
    /// An ellipse inscribed in the node's size.
    Ellipse,
}

/// Properties of a shape node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ShapeProps {
    /// The outline kind.
    pub geometry: ShapeGeometry,
    /// The corner radius of a rectangle. Not negative.
    pub corner_radius: Scalar,
}

/// The base direction of a paragraph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TextDirection {
    /// Resolve the direction from the first strong character.
    #[default]
    Auto,
    /// Left-to-right.
    Ltr,
    /// Right-to-left.
    Rtl,
}

/// Paragraph alignment, relative to the resolved text direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TextAlign {
    /// Align to the start edge: left in LTR, right in RTL.
    #[default]
    Start,
    /// Centre between the edges.
    Center,
    /// Align to the end edge: right in LTR, left in RTL.
    End,
}

/// Properties of a text node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextProps {
    /// The text. Line breaks are significant.
    pub content: String,
    /// The font family name. Resolved against the registered fonts only.
    pub font_family: String,
    /// The font size in design units. Greater than zero.
    pub font_size: Scalar,
    /// The font weight, `1..=1000` (400 normal, 700 bold).
    pub font_weight: u16,
    /// The paragraph's base direction.
    pub direction: TextDirection,
    /// The paragraph alignment.
    pub align: TextAlign,
}

impl Default for TextProps {
    fn default() -> Self {
        Self {
            content: String::new(),
            font_family: "sans-serif".to_owned(),
            font_size: Scalar::new(16.0).unwrap_or(Scalar::ONE),
            font_weight: 400,
            direction: TextDirection::Auto,
            align: TextAlign::Start,
        }
    }
}

/// Properties of an image node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ImageProps {
    /// The asset holding the pixels, or none while the image is unset.
    pub asset: Option<AssetId>,
}

/// The main axis of a flex frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Axis {
    /// Children flow left to right.
    #[default]
    Row,
    /// Children flow top to bottom.
    Column,
}

/// Distribution of children along the main axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum MainAlign {
    /// Pack at the start.
    #[default]
    Start,
    /// Pack in the middle.
    Center,
    /// Pack at the end.
    End,
    /// Spread with equal space between children.
    SpaceBetween,
}

/// Alignment of children across the main axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum CrossAlign {
    /// Align to the start edge.
    #[default]
    Start,
    /// Centre.
    Center,
    /// Align to the end edge.
    End,
    /// Stretch children whose cross size is not fixed.
    Stretch,
}

/// Space reserved inside a frame, in design units. No side is negative.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Insets {
    /// Top inset.
    pub top: Scalar,
    /// Right inset.
    pub right: Scalar,
    /// Bottom inset.
    pub bottom: Scalar,
    /// Left inset.
    pub left: Scalar,
}

impl Insets {
    /// The same inset on every side.
    #[must_use]
    pub const fn uniform(value: Scalar) -> Self {
        Self {
            top: value,
            right: value,
            bottom: value,
            left: value,
        }
    }
}

/// A flexbox layout for a frame's children.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FlexLayout {
    /// The main axis.
    pub direction: Axis,
    /// Space between adjacent children. Not negative.
    pub gap: Scalar,
    /// Space between the frame's edge and its children.
    pub padding: Insets,
    /// Distribution along the main axis.
    pub main_align: MainAlign,
    /// Alignment across the main axis.
    pub cross_align: CrossAlign,
}

/// How a frame positions its children.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FrameLayout {
    /// Children keep the positions given by their own transforms.
    #[default]
    None,
    /// Children are placed by a flexbox algorithm and their translation is
    /// decided by layout.
    Flex(FlexLayout),
}

/// Properties of a frame node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FrameProps {
    /// The layout applied to the children.
    pub layout: FrameLayout,
}

/// The kind-specific part of a node's properties.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Content {
    /// A group has no kind-specific properties.
    None,
    /// Frame properties.
    Frame(FrameProps),
    /// Shape properties.
    Shape(ShapeProps),
    /// Text properties.
    Text(TextProps),
    /// Image properties.
    Image(ImageProps),
}

impl Content {
    /// The content a node of `kind` starts with.
    #[must_use]
    pub fn default_for(kind: NodeKind) -> Self {
        match kind {
            NodeKind::Group => Self::None,
            NodeKind::Frame => Self::Frame(FrameProps::default()),
            NodeKind::Shape => Self::Shape(ShapeProps::default()),
            NodeKind::Text => Self::Text(TextProps::default()),
            NodeKind::Image => Self::Image(ImageProps::default()),
        }
    }

    /// Whether this content belongs to a node of `kind`.
    #[must_use]
    pub const fn matches(&self, kind: NodeKind) -> bool {
        matches!(
            (self, kind),
            (Self::None, NodeKind::Group)
                | (Self::Frame(_), NodeKind::Frame)
                | (Self::Shape(_), NodeKind::Shape)
                | (Self::Text(_), NodeKind::Text)
                | (Self::Image(_), NodeKind::Image)
        )
    }
}

/// Why a property set was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PropsError {
    /// The content variant does not belong to the node's kind.
    ContentKindMismatch,
    /// A length that must not be negative was negative.
    NegativeLength(&'static str),
    /// The font size is not greater than zero.
    InvalidFontSize,
    /// The font weight is outside `1..=1000`.
    InvalidFontWeight,
    /// The text exceeds [`MAX_TEXT_BYTES`].
    TextTooLong,
    /// The font family name is empty or exceeds [`MAX_FONT_FAMILY_BYTES`].
    InvalidFontFamily,
}

impl fmt::Display for PropsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ContentKindMismatch => f.write_str("the properties do not match the node kind"),
            Self::NegativeLength(field) => write!(f, "`{field}` must not be negative"),
            Self::InvalidFontSize => f.write_str("the font size must be greater than zero"),
            Self::InvalidFontWeight => f.write_str("the font weight must be within 1..=1000"),
            Self::TextTooLong => f.write_str("the text content is too long"),
            Self::InvalidFontFamily => f.write_str("the font family name is empty or too long"),
        }
    }
}

impl std::error::Error for PropsError {}

/// Every visual property of a node, replaced as a whole by one command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeProps {
    /// The map from the node's local space into its parent's space.
    ///
    /// Under a flex-layout parent the translation is decided by layout and
    /// this value's translation is ignored; its linear part still applies.
    pub transform: Transform,
    /// The stored size, used where [`Self::width_sizing`] or
    /// [`Self::height_sizing`] is [`Sizing::Fixed`].
    pub size: Size,
    /// How the width is decided.
    pub width_sizing: Sizing,
    /// How the height is decided.
    pub height_sizing: Sizing,
    /// The fill colour, or none.
    pub fill: Option<Color>,
    /// The stroke, or none.
    pub stroke: Option<Stroke>,
    /// The kind-specific properties.
    pub content: Content,
}

impl NodeProps {
    /// The properties a new node of `kind` starts with.
    #[must_use]
    pub fn default_for(kind: NodeKind) -> Self {
        let hug = if kind == NodeKind::Text {
            Sizing::Hug
        } else {
            Sizing::Fixed
        };
        Self {
            transform: Transform::IDENTITY,
            size: Size::ZERO,
            width_sizing: hug,
            height_sizing: hug,
            fill: None,
            stroke: None,
            content: Content::default_for(kind),
        }
    }

    /// Checks the rules the types themselves cannot express.
    ///
    /// # Errors
    ///
    /// Returns the first [`PropsError`] found: a content variant that does not
    /// match `kind`, a negative length, an unusable font size, weight or
    /// family, or oversize text.
    pub fn check(&self, kind: NodeKind) -> Result<(), PropsError> {
        if !self.content.matches(kind) {
            return Err(PropsError::ContentKindMismatch);
        }
        if let Some(stroke) = &self.stroke
            && stroke.width < Scalar::ZERO
        {
            return Err(PropsError::NegativeLength("stroke.width"));
        }
        match &self.content {
            Content::None | Content::Image(_) => {}
            Content::Shape(shape) => {
                if shape.corner_radius < Scalar::ZERO {
                    return Err(PropsError::NegativeLength("corner_radius"));
                }
            }
            Content::Frame(frame) => {
                if let FrameLayout::Flex(flex) = &frame.layout {
                    let lengths = [
                        ("gap", flex.gap),
                        ("padding.top", flex.padding.top),
                        ("padding.right", flex.padding.right),
                        ("padding.bottom", flex.padding.bottom),
                        ("padding.left", flex.padding.left),
                    ];
                    for (field, value) in lengths {
                        if value < Scalar::ZERO {
                            return Err(PropsError::NegativeLength(field));
                        }
                    }
                }
            }
            Content::Text(text) => {
                if text.font_size <= Scalar::ZERO {
                    return Err(PropsError::InvalidFontSize);
                }
                if !(1..=1000).contains(&text.font_weight) {
                    return Err(PropsError::InvalidFontWeight);
                }
                if text.content.len() > MAX_TEXT_BYTES {
                    return Err(PropsError::TextTooLong);
                }
                if text.font_family.is_empty() || text.font_family.len() > MAX_FONT_FAMILY_BYTES {
                    return Err(PropsError::InvalidFontFamily);
                }
            }
        }
        Ok(())
    }

    /// The asset this node refers to, if any.
    #[must_use]
    pub const fn asset(&self) -> Option<AssetId> {
        match &self.content {
            Content::Image(image) => image.asset,
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_their_kind() {
        for kind in NodeKind::ALL {
            let props = NodeProps::default_for(*kind);
            assert!(props.content.matches(*kind), "{kind:?}");
            assert_eq!(props.check(*kind), Ok(()), "{kind:?}");
        }
    }

    #[test]
    fn text_defaults_to_hug() {
        let props = NodeProps::default_for(NodeKind::Text);
        assert_eq!(props.width_sizing, Sizing::Hug);
        assert_eq!(props.height_sizing, Sizing::Hug);
    }

    #[test]
    fn mismatched_content_is_rejected() {
        let props = NodeProps::default_for(NodeKind::Text);
        assert_eq!(
            props.check(NodeKind::Shape),
            Err(PropsError::ContentKindMismatch)
        );
    }

    #[test]
    fn text_rules_are_checked() {
        let mut props = NodeProps::default_for(NodeKind::Text);
        let Content::Text(text) = &mut props.content else {
            unreachable!()
        };
        text.font_size = Scalar::ZERO;
        assert_eq!(
            props.check(NodeKind::Text),
            Err(PropsError::InvalidFontSize)
        );

        let mut props = NodeProps::default_for(NodeKind::Text);
        let Content::Text(text) = &mut props.content else {
            unreachable!()
        };
        text.font_weight = 0;
        assert_eq!(
            props.check(NodeKind::Text),
            Err(PropsError::InvalidFontWeight)
        );

        let mut props = NodeProps::default_for(NodeKind::Text);
        let Content::Text(text) = &mut props.content else {
            unreachable!()
        };
        text.font_family.clear();
        assert_eq!(
            props.check(NodeKind::Text),
            Err(PropsError::InvalidFontFamily)
        );
    }

    #[test]
    fn negative_lengths_are_rejected() {
        let mut props = NodeProps::default_for(NodeKind::Shape);
        props.content = Content::Shape(ShapeProps {
            geometry: ShapeGeometry::Rect,
            corner_radius: Scalar::new(-1.0).unwrap(),
        });
        assert_eq!(
            props.check(NodeKind::Shape),
            Err(PropsError::NegativeLength("corner_radius"))
        );

        let mut props = NodeProps::default_for(NodeKind::Frame);
        props.content = Content::Frame(FrameProps {
            layout: FrameLayout::Flex(FlexLayout {
                gap: Scalar::new(-2.0).unwrap(),
                ..FlexLayout::default()
            }),
        });
        assert_eq!(
            props.check(NodeKind::Frame),
            Err(PropsError::NegativeLength("gap"))
        );
    }
}
