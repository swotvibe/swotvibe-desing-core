//! Glyph outlines in a backend-neutral path vocabulary.
//!
//! The renderer must be able to draw text without knowing which shaping library
//! produced the glyphs, and the text crate must not know which rasterizer will
//! fill them. [`PathCommand`] is the neutral form between the two.
//!
//! ## Why quadratic curves survive
//!
//! TrueType outlines are quadratic. Keeping [`PathCommand::QuadTo`] as its own
//! variant means the conversion to a backend path never has to elevate a
//! quadratic to a cubic, which would introduce a rounding difference in a
//! reference image for no reason.

use std::fmt;

use skrifa::outline::OutlinePen;

/// One command of an outline, in design units.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum PathCommand {
    /// Starts a new contour at the given point.
    MoveTo {
        /// The contour's start point, `(x, y)`.
        x: f64,
        /// The start point's `y`.
        y: f64,
    },
    /// A straight segment to the given point.
    LineTo {
        /// The segment's end point, `(x, y)`.
        x: f64,
        /// The end point's `y`.
        y: f64,
    },
    /// A quadratic segment with one control point.
    QuadTo {
        /// The control point's `x`.
        cx: f64,
        /// The control point's `y`.
        cy: f64,
        /// The end point's `x`.
        x: f64,
        /// The end point's `y`.
        y: f64,
    },
    /// A cubic segment with two control points.
    CubicTo {
        /// The first control point's `x`.
        c1x: f64,
        /// The first control point's `y`.
        c1y: f64,
        /// The second control point's `x`.
        c2x: f64,
        /// The second control point's `y`.
        c2y: f64,
        /// The end point's `x`.
        x: f64,
        /// The end point's `y`.
        y: f64,
    },
    /// Closes the current contour.
    Close,
}

/// The outline of one glyph.
///
/// Coordinates are in the glyph's own space at the size the outline was
/// requested for, with the origin on the baseline at the pen position. The fill
/// rule is non-zero, which is what a TrueType or CFF outline expects.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GlyphOutline {
    /// The path commands, in order.
    pub commands: Vec<PathCommand>,
    /// The number of contours the commands describe.
    pub contours: usize,
}

impl GlyphOutline {
    /// Whether the outline has no drawable segment.
    ///
    /// A space has an advance but no ink, so it produces an empty outline rather
    /// than an error.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        !self
            .commands
            .iter()
            .any(|command| !matches!(command, PathCommand::MoveTo { .. } | PathCommand::Close))
    }
}

impl fmt::Display for GlyphOutline {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} contours, {} commands",
            self.contours,
            self.commands.len()
        )
    }
}

/// Records pen calls into a [`GlyphOutline`].
///
/// The recorder is public so the text crate's own tests, and any future reader
/// of a second outline source, can build the same neutral value.
#[derive(Debug, Default)]
pub struct PathRecorder {
    outline: GlyphOutline,
    open: bool,
}

impl PathRecorder {
    /// A recorder with no commands.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The recorded outline.
    #[must_use]
    pub fn finish(self) -> GlyphOutline {
        self.outline
    }

    /// The number of commands recorded so far.
    #[must_use]
    pub fn len(&self) -> usize {
        self.outline.commands.len()
    }

    /// Whether nothing has been recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.outline.commands.is_empty()
    }
}

impl OutlinePen for PathRecorder {
    fn move_to(&mut self, x: f32, y: f32) {
        self.outline.contours += 1;
        self.open = true;
        self.outline.commands.push(PathCommand::MoveTo {
            x: f64::from(x),
            y: f64::from(y),
        });
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.outline.commands.push(PathCommand::LineTo {
            x: f64::from(x),
            y: f64::from(y),
        });
    }

    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        self.outline.commands.push(PathCommand::QuadTo {
            cx: f64::from(cx),
            cy: f64::from(cy),
            x: f64::from(x),
            y: f64::from(y),
        });
    }

    fn curve_to(&mut self, c1x: f32, c1y: f32, c2x: f32, c2y: f32, x: f32, y: f32) {
        self.outline.commands.push(PathCommand::CubicTo {
            c1x: f64::from(c1x),
            c1y: f64::from(c1y),
            c2x: f64::from(c2x),
            c2y: f64::from(c2y),
            x: f64::from(x),
            y: f64::from(y),
        });
    }

    fn close(&mut self) {
        if self.open {
            self.open = false;
            self.outline.commands.push(PathCommand::Close);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_recorder_keeps_the_command_order_and_counts_contours() {
        let mut recorder = PathRecorder::new();
        recorder.move_to(0.0, 0.0);
        recorder.line_to(10.0, 0.0);
        recorder.quad_to(12.0, 2.0, 10.0, 4.0);
        recorder.curve_to(8.0, 6.0, 6.0, 6.0, 4.0, 4.0);
        recorder.close();
        recorder.move_to(1.0, 1.0);
        recorder.close();

        let outline = recorder.finish();
        assert_eq!(outline.contours, 2);
        assert_eq!(outline.commands.len(), 7);
        assert_eq!(
            outline.commands[2],
            PathCommand::QuadTo {
                cx: 12.0,
                cy: 2.0,
                x: 10.0,
                y: 4.0
            }
        );
    }

    #[test]
    fn an_outline_of_moves_only_has_no_ink() {
        let mut recorder = PathRecorder::new();
        recorder.move_to(0.0, 0.0);
        recorder.close();
        assert!(recorder.finish().is_empty());
    }
}
