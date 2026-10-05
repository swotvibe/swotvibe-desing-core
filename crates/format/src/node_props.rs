//! The persisted form of a node's visual properties.
//!
//! Property records are written in full by [`crate::export`] and read back by
//! [`crate::import`]; this module is the only place that knows both the DTO
//! names and the runtime [`NodeProps`], so the on-disk vocabulary is versioned
//! independently of the runtime enums.
//!
//! ## Absent fields
//!
//! Every optional member means "the value this build defaults to", which is the
//! rule that lets an older file open after an additive schema change. A
//! *present* member is always validated: an unknown vocabulary string, a
//! non-finite number, or a property set that breaks a runtime rule is an error,
//! never a silent fallback. That keeps an additive change additive without
//! making a malformed value invisible.
//!
//! ## Vocabulary
//!
//! Strings are stable names, matched case-sensitively, and are the on-disk
//! contract: renaming one is a schema change, not a refactor.

use serde::{Deserialize, Serialize};
use swotvibe_core::{
    Color, Content, CrossAlign, FlexLayout, FrameLayout, FrameProps, GeometryError, ImageProps,
    Insets, MainAlign, NodeKind, NodeProps, Scalar, ShapeGeometry, ShapeProps, Size, Sizing,
    Stroke, TextAlign, TextDirection, TextProps, Transform,
};

use crate::dto::Extensions;

/// The stable name for [`Sizing::Fixed`].
pub const SIZING_FIXED: &str = "fixed";
/// The stable name for [`Sizing::Fill`].
pub const SIZING_FILL: &str = "fill";
/// The stable name for [`Sizing::Hug`].
pub const SIZING_HUG: &str = "hug";

const SHAPE_RECT: &str = "rect";
const SHAPE_ELLIPSE: &str = "ellipse";

const DIRECTION_AUTO: &str = "auto";
const DIRECTION_LTR: &str = "ltr";
const DIRECTION_RTL: &str = "rtl";

const ALIGN_START: &str = "start";
const ALIGN_CENTER: &str = "center";
const ALIGN_END: &str = "end";
const ALIGN_SPACE_BETWEEN: &str = "space-between";

const CROSS_STRETCH: &str = "stretch";

const AXIS_ROW: &str = "row";
const AXIS_COLUMN: &str = "column";

const LAYOUT_NONE: &str = "none";
const LAYOUT_FLEX: &str = "flex";

/// Why a persisted property record could not become runtime properties.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum PropsDtoError {
    /// A vocabulary string is not one this schema knows.
    UnknownName {
        /// The member the name appeared in, for diagnostics.
        field: &'static str,
        /// The name found in the file.
        found: String,
    },
    /// A number was not finite, or was outside the supported range.
    Number {
        /// The member the number appeared in.
        field: &'static str,
        /// What was wrong with it.
        error: GeometryError,
    },
    /// The record carries the properties of a different node kind.
    KindMismatch {
        /// The kind the record describes.
        found: &'static str,
        /// The node's actual kind.
        expected: &'static str,
    },
    /// The record breaks a rule the runtime enforces.
    Rule(String),
}

impl std::fmt::Display for PropsDtoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownName { field, found } => {
                write!(f, "`{found}` is not a known value for `{field}`")
            }
            Self::Number { field, error } => write!(f, "`{field}` is invalid: {error}"),
            Self::KindMismatch { found, expected } => write!(
                f,
                "the record carries `{found}` properties but the node is a `{expected}`"
            ),
            Self::Rule(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for PropsDtoError {}

fn number(field: &'static str, value: f64) -> Result<Scalar, PropsDtoError> {
    Scalar::new(value).map_err(|error| PropsDtoError::Number { field, error })
}

/// The persisted form of a stroke.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DtoStroke {
    /// sRGB channels with straight alpha, as `[r, g, b, a]`.
    pub color: [u8; 4],
    /// The width in design units.
    pub width: f64,
    /// Unknown fields preserved verbatim from the file.
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// The persisted form of an inset box.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DtoInsets {
    /// Top inset in design units.
    pub top: f64,
    /// Right inset in design units.
    pub right: f64,
    /// Bottom inset in design units.
    pub bottom: f64,
    /// Left inset in design units.
    pub left: f64,
}

/// The persisted form of a frame's layout.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DtoFrameProps {
    /// `"none"` or `"flex"`.
    pub layout: String,
    /// The main axis, `"row"` or `"column"`. Absent means the default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<String>,
    /// Space between children, in design units.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gap: Option<f64>,
    /// Space between the frame's edge and its children.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub padding: Option<DtoInsets>,
    /// Distribution along the main axis.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub main_align: Option<String>,
    /// Alignment across the main axis.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cross_align: Option<String>,
    /// Unknown fields preserved verbatim from the file.
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// The persisted form of a shape.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DtoShapeProps {
    /// `"rect"` or `"ellipse"`.
    pub geometry: String,
    /// The corner radius of a rectangle, in design units.
    pub corner_radius: f64,
    /// Unknown fields preserved verbatim from the file.
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// The persisted form of a text node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DtoTextProps {
    /// The text. Line breaks are significant.
    pub content: String,
    /// The font family name.
    pub font_family: String,
    /// The font size in design units.
    pub font_size: f64,
    /// The font weight, `1..=1000`.
    pub font_weight: u16,
    /// `"auto"`, `"ltr"`, or `"rtl"`.
    pub direction: String,
    /// `"start"`, `"center"`, or `"end"`.
    pub align: String,
    /// Unknown fields preserved verbatim from the file.
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// The persisted form of an image node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DtoImageProps {
    /// The asset identity, or `null` while the image is unset.
    pub asset: Option<String>,
    /// Unknown fields preserved verbatim from the file.
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// The persisted form of every visual property of a node.
///
/// Exactly one of `frame`, `shape`, `text`, and `image` belongs to a given
/// node, chosen by the node's kind. A record carrying another kind's member is
/// rejected instead of ignored, so a file cannot describe a shape as text and
/// have the difference silently disappear.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DtoProps {
    /// The affine transform coefficients `[a, b, c, d, e, f]`.
    pub transform: [f64; 6],
    /// The stored size `[width, height]` in design units.
    pub size: [f64; 2],
    /// `"fixed"`, `"fill"`, or `"hug"`.
    #[serde(default = "default_sizing_name")]
    pub width: String,
    /// `"fixed"`, `"fill"`, or `"hug"`.
    #[serde(default = "default_sizing_name")]
    pub height: String,
    /// The fill colour as `[r, g, b, a]`, or absent for no fill.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<[u8; 4]>,
    /// The stroke, or absent for no stroke.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke: Option<DtoStroke>,
    /// Present only on a frame.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame: Option<DtoFrameProps>,
    /// Present only on a shape.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shape: Option<DtoShapeProps>,
    /// Present only on a text node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<DtoTextProps>,
    /// Present only on an image node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<DtoImageProps>,
    /// Unknown fields preserved verbatim from the file.
    #[serde(flatten)]
    pub extensions: Extensions,
}

fn default_sizing_name() -> String {
    SIZING_FIXED.to_owned()
}

fn sizing_name(sizing: Sizing) -> &'static str {
    match sizing {
        Sizing::Fixed => SIZING_FIXED,
        Sizing::Fill => SIZING_FILL,
        Sizing::Hug => SIZING_HUG,
    }
}

fn sizing_from(field: &'static str, value: &str) -> Result<Sizing, PropsDtoError> {
    match value {
        SIZING_FIXED => Ok(Sizing::Fixed),
        SIZING_FILL => Ok(Sizing::Fill),
        SIZING_HUG => Ok(Sizing::Hug),
        _ => Err(PropsDtoError::UnknownName {
            field,
            found: value.to_owned(),
        }),
    }
}

/// Projects runtime properties into their persisted form.
#[must_use]
pub fn props_to_dto(props: &NodeProps) -> DtoProps {
    DtoProps {
        transform: props.transform.coefficients(),
        size: [props.size.width(), props.size.height()],
        width: sizing_name(props.width_sizing).to_owned(),
        height: sizing_name(props.height_sizing).to_owned(),
        fill: props.fill.map(Color::to_array),
        stroke: props.stroke.map(|stroke| DtoStroke {
            color: stroke.color.to_array(),
            width: stroke.width.get(),
            extensions: Extensions::new(),
        }),
        frame: match &props.content {
            Content::Frame(frame) => Some(frame_to_dto(frame)),
            _ => None,
        },
        shape: match &props.content {
            Content::Shape(shape) => Some(DtoShapeProps {
                geometry: match shape.geometry {
                    ShapeGeometry::Rect => SHAPE_RECT,
                    ShapeGeometry::Ellipse => SHAPE_ELLIPSE,
                }
                .to_owned(),
                corner_radius: shape.corner_radius.get(),
                extensions: Extensions::new(),
            }),
            _ => None,
        },
        text: match &props.content {
            Content::Text(text) => Some(DtoTextProps {
                content: text.content.clone(),
                font_family: text.font_family.clone(),
                font_size: text.font_size.get(),
                font_weight: text.font_weight,
                direction: match text.direction {
                    TextDirection::Auto => DIRECTION_AUTO,
                    TextDirection::Ltr => DIRECTION_LTR,
                    TextDirection::Rtl => DIRECTION_RTL,
                }
                .to_owned(),
                align: match text.align {
                    TextAlign::Start => ALIGN_START,
                    TextAlign::Center => ALIGN_CENTER,
                    TextAlign::End => ALIGN_END,
                }
                .to_owned(),
                extensions: Extensions::new(),
            }),
            _ => None,
        },
        image: match &props.content {
            Content::Image(image) => Some(DtoImageProps {
                asset: image.asset.map(|asset| asset.to_string()),
                extensions: Extensions::new(),
            }),
            _ => None,
        },
        extensions: Extensions::new(),
    }
}

fn frame_to_dto(frame: &FrameProps) -> DtoFrameProps {
    match frame.layout {
        FrameLayout::None => DtoFrameProps {
            layout: LAYOUT_NONE.to_owned(),
            direction: None,
            gap: None,
            padding: None,
            main_align: None,
            cross_align: None,
            extensions: Extensions::new(),
        },
        FrameLayout::Flex(flex) => DtoFrameProps {
            layout: LAYOUT_FLEX.to_owned(),
            direction: Some(
                match flex.direction {
                    swotvibe_core::Axis::Row => AXIS_ROW,
                    swotvibe_core::Axis::Column => AXIS_COLUMN,
                }
                .to_owned(),
            ),
            gap: Some(flex.gap.get()),
            padding: Some(DtoInsets {
                top: flex.padding.top.get(),
                right: flex.padding.right.get(),
                bottom: flex.padding.bottom.get(),
                left: flex.padding.left.get(),
            }),
            main_align: Some(
                match flex.main_align {
                    MainAlign::Start => ALIGN_START,
                    MainAlign::Center => ALIGN_CENTER,
                    MainAlign::End => ALIGN_END,
                    MainAlign::SpaceBetween => ALIGN_SPACE_BETWEEN,
                }
                .to_owned(),
            ),
            cross_align: Some(
                match flex.cross_align {
                    CrossAlign::Start => ALIGN_START,
                    CrossAlign::Center => ALIGN_CENTER,
                    CrossAlign::End => ALIGN_END,
                    CrossAlign::Stretch => CROSS_STRETCH,
                }
                .to_owned(),
            ),
            extensions: Extensions::new(),
        },
    }
}

/// Reads a persisted property record into runtime properties.
///
/// `kind` is the node's kind, which selects the one content member that is
/// allowed to be present.
///
/// # Errors
///
/// Returns [`PropsDtoError`] for an unknown vocabulary name, a number that is
/// not finite or is out of range, a member that belongs to another kind, or a
/// record that breaks a rule [`NodeProps::check`] enforces.
pub fn props_from_dto(kind: NodeKind, dto: &DtoProps) -> Result<NodeProps, PropsDtoError> {
    let size = Size::new(dto.size[0], dto.size[1]).map_err(|error| PropsDtoError::Number {
        field: "size",
        error,
    })?;
    let transform = Transform::new(dto.transform).map_err(|error| PropsDtoError::Number {
        field: "transform",
        error,
    })?;

    require_absent(kind, dto)?;

    let content = match kind {
        NodeKind::Group => Content::None,
        NodeKind::Frame => Content::Frame(frame_from_dto(dto.frame.as_ref())?),
        NodeKind::Shape => Content::Shape(shape_from_dto(dto.shape.as_ref())?),
        NodeKind::Text => Content::Text(text_from_dto(dto.text.as_ref())?),
        NodeKind::Image => Content::Image(image_from_dto(dto.image.as_ref())?),
    };

    let props = NodeProps {
        transform,
        size,
        width_sizing: sizing_from("width", &dto.width)?,
        height_sizing: sizing_from("height", &dto.height)?,
        fill: dto.fill.map(|[r, g, b, a]| Color::rgba(r, g, b, a)),
        stroke: match &dto.stroke {
            Some(stroke) => Some(Stroke {
                color: Color::rgba(
                    stroke.color[0],
                    stroke.color[1],
                    stroke.color[2],
                    stroke.color[3],
                ),
                width: number("stroke.width", stroke.width)?,
            }),
            None => None,
        },
        content,
    };

    props
        .check(kind)
        .map_err(|error| PropsDtoError::Rule(error.to_string()))?;
    Ok(props)
}

/// A member name, the kind that owns it, and a test for whether it is present.
type ContentMember = (&'static str, NodeKind, fn(&DtoProps) -> bool);

/// The kind whose content member each optional member belongs to.
const MEMBERS: &[ContentMember] = &[
    ("frame", NodeKind::Frame, |dto| dto.frame.is_some()),
    ("shape", NodeKind::Shape, |dto| dto.shape.is_some()),
    ("text", NodeKind::Text, |dto| dto.text.is_some()),
    ("image", NodeKind::Image, |dto| dto.image.is_some()),
];

fn require_absent(kind: NodeKind, dto: &DtoProps) -> Result<(), PropsDtoError> {
    for (name, owner, present) in MEMBERS {
        if *owner != kind && present(dto) {
            return Err(PropsDtoError::KindMismatch {
                found: name,
                expected: kind.as_str(),
            });
        }
    }
    Ok(())
}

fn frame_from_dto(dto: Option<&DtoFrameProps>) -> Result<FrameProps, PropsDtoError> {
    let Some(dto) = dto else {
        return Ok(FrameProps::default());
    };
    match dto.layout.as_str() {
        LAYOUT_NONE => Ok(FrameProps {
            layout: FrameLayout::None,
        }),
        LAYOUT_FLEX => Ok(FrameProps {
            layout: FrameLayout::Flex(FlexLayout {
                direction: match dto.direction.as_deref() {
                    None | Some(AXIS_ROW) => swotvibe_core::Axis::Row,
                    Some(AXIS_COLUMN) => swotvibe_core::Axis::Column,
                    Some(other) => {
                        return Err(PropsDtoError::UnknownName {
                            field: "frame.direction",
                            found: other.to_owned(),
                        });
                    }
                },
                gap: match dto.gap {
                    Some(value) => number("frame.gap", value)?,
                    None => Scalar::ZERO,
                },
                padding: match dto.padding {
                    Some(padding) => Insets {
                        top: number("padding.top", padding.top)?,
                        right: number("padding.right", padding.right)?,
                        bottom: number("padding.bottom", padding.bottom)?,
                        left: number("padding.left", padding.left)?,
                    },
                    None => Insets::default(),
                },
                main_align: match dto.main_align.as_deref() {
                    None | Some(ALIGN_START) => MainAlign::Start,
                    Some(ALIGN_CENTER) => MainAlign::Center,
                    Some(ALIGN_END) => MainAlign::End,
                    Some(ALIGN_SPACE_BETWEEN) => MainAlign::SpaceBetween,
                    Some(other) => {
                        return Err(PropsDtoError::UnknownName {
                            field: "frame.main_align",
                            found: other.to_owned(),
                        });
                    }
                },
                cross_align: match dto.cross_align.as_deref() {
                    None | Some(ALIGN_START) => CrossAlign::Start,
                    Some(ALIGN_CENTER) => CrossAlign::Center,
                    Some(ALIGN_END) => CrossAlign::End,
                    Some(CROSS_STRETCH) => CrossAlign::Stretch,
                    Some(other) => {
                        return Err(PropsDtoError::UnknownName {
                            field: "frame.cross_align",
                            found: other.to_owned(),
                        });
                    }
                },
            }),
        }),
        other => Err(PropsDtoError::UnknownName {
            field: "frame.layout",
            found: other.to_owned(),
        }),
    }
}

fn shape_from_dto(dto: Option<&DtoShapeProps>) -> Result<ShapeProps, PropsDtoError> {
    let Some(dto) = dto else {
        return Ok(ShapeProps::default());
    };
    Ok(ShapeProps {
        geometry: match dto.geometry.as_str() {
            SHAPE_RECT => ShapeGeometry::Rect,
            SHAPE_ELLIPSE => ShapeGeometry::Ellipse,
            other => {
                return Err(PropsDtoError::UnknownName {
                    field: "shape.geometry",
                    found: other.to_owned(),
                });
            }
        },
        corner_radius: number("shape.corner_radius", dto.corner_radius)?,
    })
}

fn text_from_dto(dto: Option<&DtoTextProps>) -> Result<TextProps, PropsDtoError> {
    let Some(dto) = dto else {
        return Ok(TextProps::default());
    };
    let direction = match dto.direction.as_str() {
        DIRECTION_AUTO => TextDirection::Auto,
        DIRECTION_LTR => TextDirection::Ltr,
        DIRECTION_RTL => TextDirection::Rtl,
        other => {
            return Err(PropsDtoError::UnknownName {
                field: "text.direction",
                found: other.to_owned(),
            });
        }
    };
    let align = match dto.align.as_str() {
        ALIGN_START => TextAlign::Start,
        ALIGN_CENTER => TextAlign::Center,
        ALIGN_END => TextAlign::End,
        other => {
            return Err(PropsDtoError::UnknownName {
                field: "text.align",
                found: other.to_owned(),
            });
        }
    };
    Ok(TextProps {
        content: dto.content.clone(),
        font_family: dto.font_family.clone(),
        font_size: number("text.font_size", dto.font_size)?,
        font_weight: dto.font_weight,
        direction,
        align,
    })
}

fn image_from_dto(dto: Option<&DtoImageProps>) -> Result<ImageProps, PropsDtoError> {
    let Some(dto) = dto else {
        return Ok(ImageProps::default());
    };
    let asset =
        match &dto.asset {
            Some(text) => Some(text.parse().map_err(|_| {
                PropsDtoError::Rule(format!("`{text}` is not a valid asset identity"))
            })?),
            None => None,
        };
    Ok(ImageProps { asset })
}

#[cfg(test)]
mod tests {
    use super::*;
    use swotvibe_core::AssetId;

    fn round_trip(kind: NodeKind, props: &NodeProps) {
        let dto = props_to_dto(props);
        let back = props_from_dto(kind, &dto).expect("a written record should read back");
        assert_eq!(&back, props, "{kind:?}");
    }

    #[test]
    fn every_kind_round_trips_through_its_dto() {
        for kind in NodeKind::ALL {
            let mut props = NodeProps::default_for(*kind);
            props.transform = Transform::translate(1.5, -2.5)
                .expect("finite")
                .compose(&Transform::rotate(0.5).expect("finite"))
                .expect("finite");
            props.size = Size::new(120.0, 40.0).expect("non-negative");
            props.width_sizing = Sizing::Fill;
            props.height_sizing = Sizing::Hug;
            props.fill = Some(Color::rgba(1, 2, 3, 4));
            props.stroke = Some(Stroke {
                color: Color::rgb(255, 0, 0),
                width: Scalar::new(2.0).expect("finite"),
            });
            match &mut props.content {
                Content::Frame(frame) => {
                    frame.layout = FrameLayout::Flex(FlexLayout {
                        direction: swotvibe_core::Axis::Column,
                        gap: Scalar::new(8.0).expect("finite"),
                        padding: Insets::uniform(Scalar::new(16.0).expect("finite")),
                        main_align: MainAlign::SpaceBetween,
                        cross_align: CrossAlign::Stretch,
                    });
                }
                Content::Shape(shape) => {
                    shape.geometry = ShapeGeometry::Ellipse;
                    shape.corner_radius = Scalar::new(4.0).expect("finite");
                }
                Content::Text(text) => {
                    text.content = "مرحبا\nhello".into();
                    text.font_family = "Noto Sans Arabic".into();
                    text.font_size = Scalar::new(18.0).expect("finite");
                    text.font_weight = 700;
                    text.direction = TextDirection::Rtl;
                    text.align = TextAlign::End;
                }
                Content::Image(image) => {
                    image.asset = Some(AssetId::new());
                }
                Content::None => {}
            }
            round_trip(*kind, &props);
        }
    }

    #[test]
    fn a_record_for_another_kind_is_rejected() {
        let dto = props_to_dto(&NodeProps::default_for(NodeKind::Text));
        assert_eq!(
            props_from_dto(NodeKind::Shape, &dto),
            Err(PropsDtoError::KindMismatch {
                found: "text",
                expected: "shape",
            })
        );
    }

    #[test]
    fn an_unknown_vocabulary_name_is_rejected() {
        let mut dto = props_to_dto(&NodeProps::default_for(NodeKind::Shape));
        dto.width = "banana".into();
        assert_eq!(
            props_from_dto(NodeKind::Shape, &dto),
            Err(PropsDtoError::UnknownName {
                field: "width",
                found: "banana".into(),
            })
        );
    }

    #[test]
    fn a_non_finite_number_is_rejected() {
        let mut dto = props_to_dto(&NodeProps::default_for(NodeKind::Shape));
        dto.transform[4] = f64::NAN;
        assert!(matches!(
            props_from_dto(NodeKind::Shape, &dto),
            Err(PropsDtoError::Number {
                field: "transform",
                ..
            })
        ));
    }

    #[test]
    fn a_rule_violation_is_reported_as_such() {
        let mut dto = props_to_dto(&NodeProps::default_for(NodeKind::Text));
        dto.text.as_mut().expect("a text record").font_size = 0.0;
        assert!(matches!(
            props_from_dto(NodeKind::Text, &dto),
            Err(PropsDtoError::Rule(_))
        ));
    }

    #[test]
    fn an_absent_optional_member_means_its_default() {
        let json = r#"{"transform":[1,0,0,1,0,0],"size":[0,0]}"#;
        let dto: DtoProps = serde_json::from_str(json).expect("a minimal record should parse");
        assert_eq!(dto.width, SIZING_FIXED);
        assert_eq!(
            props_from_dto(NodeKind::Frame, &dto).expect("defaults should apply"),
            NodeProps::default_for(NodeKind::Frame)
        );
    }

    #[test]
    fn unknown_members_are_preserved() {
        let json = r#"{
            "transform": [1, 0, 0, 1, 0, 0],
            "size": [10, 10],
            "futureField": {"a": 1},
            "shape": {"geometry": "rect", "corner_radius": 0, "futureShape": true}
        }"#;
        let dto: DtoProps = serde_json::from_str(json).expect("unknown members should be kept");
        assert!(dto.extensions.contains_key("futureField"));
        let shape = dto.shape.as_ref().expect("a shape record");
        assert!(shape.extensions.contains_key("futureShape"));

        let text = serde_json::to_string(&dto).expect("the record should serialize");
        assert!(text.contains("futureField"));
        assert!(text.contains("futureShape"));
    }
}
