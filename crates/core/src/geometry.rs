//! Coordinate contract: finite scalars, sizes, and 2D affine transforms.
//!
//! Ownership: the numeric value types of the design model and the rules that
//! keep them finite. Not owned: pixels, devices, or any renderer's own matrix
//! type.
//!
//! This module implements ADR-0002.
//!
//! ## Conventions
//!
//! - Lengths are **design units**: logical, resolution independent, never
//!   pixels. A renderer chooses the pixels-per-unit scale at export time.
//! - The coordinate system is **y-down**: the origin is the top-left corner,
//!   `x` grows to the right and `y` grows downward.
//! - A node's transform is a 2D affine map from the node's local space into
//!   its parent's space. A page root maps into page space.
//! - A [`Transform`] stores six coefficients `[a, b, c, d, e, f]` that map a
//!   point as `x' = a*x + c*y + e` and `y' = b*x + d*y + f`. This is the same
//!   order as the SVG `matrix(a b c d e f)` and the Kurbo/Vello `Affine`, so
//!   adapters pass the coefficients through unchanged.
//! - A positive rotation angle turns the `+x` axis toward `+y`. Because `y`
//!   points down, that is a **clockwise** rotation on screen.
//! - Composition is parent-after-child: `parent.compose(&child)` maps a point
//!   through `child` first and then through `parent`, so it is the child's
//!   transform expressed in the parent's parent space.
//!
//! ## Finite values
//!
//! Every stored number is finite and bounded by [`Scalar::MAX_MAGNITUDE`].
//! `NaN` and infinities are rejected at construction rather than propagated,
//! and negative zero is normalised to zero. A value that survives
//! construction therefore compares with a total, reflexive equality, which is
//! why [`Scalar`] and everything built from it implements [`Eq`].

use std::fmt;

/// Why a geometric value was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum GeometryError {
    /// The value was `NaN` or infinite.
    NotFinite,
    /// The value's magnitude exceeds [`Scalar::MAX_MAGNITUDE`].
    OutOfRange,
    /// A length that must not be negative was negative.
    Negative,
    /// The transform has no inverse because its determinant is zero.
    Singular,
}

impl fmt::Display for GeometryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NotFinite => "the value is not finite",
            Self::OutOfRange => "the value is outside the supported range",
            Self::Negative => "the value must not be negative",
            Self::Singular => "the transform is not invertible",
        })
    }
}

impl std::error::Error for GeometryError {}

/// A finite, bounded number.
///
/// # Examples
///
/// ```
/// use swotvibe_core::geometry::Scalar;
///
/// assert!(Scalar::new(1.5).is_ok());
/// assert!(Scalar::new(f64::NAN).is_err());
/// assert_eq!(Scalar::new(-0.0).unwrap(), Scalar::ZERO);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct Scalar(f64);

// Sound because construction rejects NaN, so equality is reflexive.
impl Eq for Scalar {}

impl Scalar {
    /// The largest magnitude a scalar may hold.
    ///
    /// Chosen so that the product of two scalars cannot overflow `f64`, which
    /// keeps composition of a bounded depth finite in the common case.
    pub const MAX_MAGNITUDE: f64 = 1.0e9;

    /// Zero.
    pub const ZERO: Self = Self(0.0);

    /// One.
    pub const ONE: Self = Self(1.0);

    /// Creates a scalar.
    ///
    /// # Errors
    ///
    /// Returns [`GeometryError::NotFinite`] for `NaN` or an infinity, and
    /// [`GeometryError::OutOfRange`] when the magnitude exceeds
    /// [`Self::MAX_MAGNITUDE`].
    pub fn new(value: f64) -> Result<Self, GeometryError> {
        if !value.is_finite() {
            return Err(GeometryError::NotFinite);
        }
        if value.abs() > Self::MAX_MAGNITUDE {
            return Err(GeometryError::OutOfRange);
        }
        // `+ 0.0` turns negative zero into positive zero.
        Ok(Self(value + 0.0))
    }

    /// The stored value.
    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }
}

impl TryFrom<f64> for Scalar {
    type Error = GeometryError;

    fn try_from(value: f64) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl fmt::Display for Scalar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// A width and height in design units. Neither is negative.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Size {
    width: Scalar,
    height: Scalar,
}

impl Size {
    /// A zero-sized extent.
    pub const ZERO: Self = Self {
        width: Scalar::ZERO,
        height: Scalar::ZERO,
    };

    /// Creates a size.
    ///
    /// # Errors
    ///
    /// Returns [`GeometryError::Negative`] for a negative side, or the error
    /// [`Scalar::new`] gives for a non-finite or out-of-range value.
    pub fn new(width: f64, height: f64) -> Result<Self, GeometryError> {
        let width = Scalar::new(width)?;
        let height = Scalar::new(height)?;
        if width < Scalar::ZERO || height < Scalar::ZERO {
            return Err(GeometryError::Negative);
        }
        Ok(Self { width, height })
    }

    /// The width in design units.
    #[must_use]
    pub const fn width(self) -> f64 {
        self.width.get()
    }

    /// The height in design units.
    #[must_use]
    pub const fn height(self) -> f64 {
        self.height.get()
    }
}

/// A 2D affine transform. See the [module documentation](self) for the
/// conventions.
///
/// # Examples
///
/// ```
/// use swotvibe_core::geometry::Transform;
///
/// let parent = Transform::translate(100.0, 50.0).unwrap();
/// let child = Transform::scale(2.0, 2.0).unwrap();
/// // A child point is scaled first, then moved by the parent.
/// let world = parent.compose(&child).unwrap();
/// assert_eq!(world.apply(1.0, 1.0), (102.0, 52.0));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Transform {
    a: Scalar,
    b: Scalar,
    c: Scalar,
    d: Scalar,
    e: Scalar,
    f: Scalar,
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Transform {
    /// The identity transform.
    pub const IDENTITY: Self = Self {
        a: Scalar::ONE,
        b: Scalar::ZERO,
        c: Scalar::ZERO,
        d: Scalar::ONE,
        e: Scalar::ZERO,
        f: Scalar::ZERO,
    };

    /// Creates a transform from the coefficients `[a, b, c, d, e, f]`.
    ///
    /// # Errors
    ///
    /// Returns the error [`Scalar::new`] gives for any non-finite or
    /// out-of-range coefficient.
    pub fn new(coefficients: [f64; 6]) -> Result<Self, GeometryError> {
        let [a, b, c, d, e, f] = coefficients;
        Ok(Self {
            a: Scalar::new(a)?,
            b: Scalar::new(b)?,
            c: Scalar::new(c)?,
            d: Scalar::new(d)?,
            e: Scalar::new(e)?,
            f: Scalar::new(f)?,
        })
    }

    /// A translation by `(x, y)` design units.
    ///
    /// # Errors
    ///
    /// As [`Self::new`].
    pub fn translate(x: f64, y: f64) -> Result<Self, GeometryError> {
        Self::new([1.0, 0.0, 0.0, 1.0, x, y])
    }

    /// A scale about the origin.
    ///
    /// # Errors
    ///
    /// As [`Self::new`].
    pub fn scale(x: f64, y: f64) -> Result<Self, GeometryError> {
        Self::new([x, 0.0, 0.0, y, 0.0, 0.0])
    }

    /// A rotation about the origin by `radians`.
    ///
    /// A positive angle turns `+x` toward `+y`, which is clockwise on screen
    /// in the y-down space.
    ///
    /// # Errors
    ///
    /// Returns [`GeometryError::NotFinite`] when `radians` is not finite.
    pub fn rotate(radians: f64) -> Result<Self, GeometryError> {
        if !radians.is_finite() {
            return Err(GeometryError::NotFinite);
        }
        let (sin, cos) = radians.sin_cos();
        Self::new([cos, sin, -sin, cos, 0.0, 0.0])
    }

    /// The coefficients `[a, b, c, d, e, f]`.
    #[must_use]
    pub const fn coefficients(&self) -> [f64; 6] {
        [
            self.a.get(),
            self.b.get(),
            self.c.get(),
            self.d.get(),
            self.e.get(),
            self.f.get(),
        ]
    }

    /// The determinant of the linear part. A negative value means the
    /// transform flips orientation.
    #[must_use]
    pub const fn determinant(&self) -> f64 {
        self.a.get() * self.d.get() - self.b.get() * self.c.get()
    }

    /// Maps the point `(x, y)` through this transform.
    #[must_use]
    pub const fn apply(&self, x: f64, y: f64) -> (f64, f64) {
        let [a, b, c, d, e, f] = self.coefficients();
        (a * x + c * y + e, b * x + d * y + f)
    }

    /// Composes two transforms: the result maps a point through `inner`
    /// first and through `self` second.
    ///
    /// A node's transform into its grandparent's space is
    /// `parent.compose(&node)`.
    ///
    /// # Errors
    ///
    /// Returns the error [`Scalar::new`] gives when a coefficient of the
    /// result is not finite or leaves the supported range.
    pub fn compose(&self, inner: &Self) -> Result<Self, GeometryError> {
        let [a1, b1, c1, d1, e1, f1] = self.coefficients();
        let [a2, b2, c2, d2, e2, f2] = inner.coefficients();
        Self::new([
            a1 * a2 + c1 * b2,
            b1 * a2 + d1 * b2,
            a1 * c2 + c1 * d2,
            b1 * c2 + d1 * d2,
            a1 * e2 + c1 * f2 + e1,
            b1 * e2 + d1 * f2 + f1,
        ])
    }

    /// The inverse transform.
    ///
    /// # Errors
    ///
    /// Returns [`GeometryError::Singular`] when the determinant is zero or too
    /// small to invert, and the error [`Scalar::new`] gives when the inverse
    /// leaves the supported range.
    pub fn invert(&self) -> Result<Self, GeometryError> {
        let det = self.determinant();
        if !det.is_finite() || det.abs() < f64::EPSILON * f64::EPSILON {
            return Err(GeometryError::Singular);
        }
        let [a, b, c, d, e, f] = self.coefficients();
        let inv = 1.0 / det;
        Self::new([
            d * inv,
            -b * inv,
            -c * inv,
            a * inv,
            (c * f - d * e) * inv,
            (b * e - a * f) * inv,
        ])
    }

    /// The SVG `matrix(a b c d e f)` attribute value for this transform.
    ///
    /// Numbers use the shortest text that round-trips, so the output is
    /// deterministic across platforms.
    #[must_use]
    pub fn to_svg_matrix(&self) -> String {
        let [a, b, c, d, e, f] = self.coefficients();
        format!("matrix({a} {b} {c} {d} {e} {f})")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::{FRAC_PI_2, FRAC_PI_4};

    fn close(actual: (f64, f64), expected: (f64, f64)) -> bool {
        (actual.0 - expected.0).abs() < 1e-9 && (actual.1 - expected.1).abs() < 1e-9
    }

    #[test]
    fn scalar_rejects_non_finite_and_out_of_range() {
        assert_eq!(Scalar::new(f64::NAN), Err(GeometryError::NotFinite));
        assert_eq!(Scalar::new(f64::INFINITY), Err(GeometryError::NotFinite));
        assert_eq!(Scalar::new(1.0e10), Err(GeometryError::OutOfRange));
        assert_eq!(Scalar::new(-1.0e10), Err(GeometryError::OutOfRange));
    }

    #[test]
    fn scalar_normalises_negative_zero() {
        let zero = Scalar::new(-0.0).unwrap();
        assert!(zero.get().is_sign_positive());
        assert_eq!(zero, Scalar::ZERO);
    }

    #[test]
    fn size_rejects_negative_sides() {
        assert_eq!(Size::new(-1.0, 2.0), Err(GeometryError::Negative));
        assert_eq!(Size::new(1.0, -2.0), Err(GeometryError::Negative));
        let size = Size::new(3.0, 4.0).unwrap();
        assert_eq!((size.width(), size.height()), (3.0, 4.0));
    }

    #[test]
    fn identity_leaves_points_unchanged() {
        assert_eq!(Transform::IDENTITY.apply(7.0, -3.0), (7.0, -3.0));
        assert_eq!(Transform::default(), Transform::IDENTITY);
    }

    #[test]
    fn golden_translate() {
        let t = Transform::translate(10.0, 20.0).unwrap();
        assert_eq!(t.coefficients(), [1.0, 0.0, 0.0, 1.0, 10.0, 20.0]);
        assert_eq!(t.apply(1.0, 2.0), (11.0, 22.0));
        assert_eq!(t.to_svg_matrix(), "matrix(1 0 0 1 10 20)");
    }

    #[test]
    fn golden_scale() {
        let t = Transform::scale(2.0, 3.0).unwrap();
        assert_eq!(t.coefficients(), [2.0, 0.0, 0.0, 3.0, 0.0, 0.0]);
        assert_eq!(t.apply(1.0, 1.0), (2.0, 3.0));
        assert_eq!(t.to_svg_matrix(), "matrix(2 0 0 3 0 0)");
    }

    #[test]
    fn golden_rotate_positive_angle_is_clockwise_on_screen() {
        let quarter = Transform::rotate(FRAC_PI_2).unwrap();
        // +x turns toward +y (down on screen); +y turns toward -x.
        assert!(close(quarter.apply(1.0, 0.0), (0.0, 1.0)));
        assert!(close(quarter.apply(0.0, 1.0), (-1.0, 0.0)));
        let eighth = Transform::rotate(FRAC_PI_4).unwrap();
        let root_half = std::f64::consts::FRAC_1_SQRT_2;
        assert!(close(eighth.apply(1.0, 0.0), (root_half, root_half)));
    }

    #[test]
    fn composition_applies_the_inner_transform_first() {
        let parent = Transform::translate(100.0, 0.0).unwrap();
        let child = Transform::scale(2.0, 2.0).unwrap();
        let world = parent.compose(&child).unwrap();
        assert_eq!(world.apply(1.0, 0.0), (102.0, 0.0));
        // The opposite order scales the translation too.
        let flipped = child.compose(&parent).unwrap();
        assert_eq!(flipped.apply(1.0, 0.0), (202.0, 0.0));
    }

    #[test]
    fn golden_nested_composition_matches_stepwise_application() {
        let grandparent = Transform::translate(10.0, 5.0).unwrap();
        let parent = Transform::rotate(FRAC_PI_2).unwrap();
        let child = Transform::scale(2.0, 4.0).unwrap();
        let world = grandparent
            .compose(&parent.compose(&child).unwrap())
            .unwrap();

        let stepwise = {
            let p = child.apply(1.0, 1.0);
            let p = parent.apply(p.0, p.1);
            grandparent.apply(p.0, p.1)
        };
        assert!(close(world.apply(1.0, 1.0), stepwise));
        // (1,1) -> (2,4) -> rotate 90 deg -> (-4,2) -> translate -> (6,7)
        assert!(close(world.apply(1.0, 1.0), (6.0, 7.0)));

        let regrouped = grandparent
            .compose(&parent)
            .unwrap()
            .compose(&child)
            .unwrap();
        for (x, y) in [(0.0, 0.0), (1.0, 1.0), (-3.0, 2.5)] {
            assert!(close(world.apply(x, y), regrouped.apply(x, y)));
        }
    }

    #[test]
    fn inverse_round_trips_points() {
        let t = Transform::translate(7.0, -2.0)
            .unwrap()
            .compose(&Transform::rotate(0.7).unwrap())
            .unwrap()
            .compose(&Transform::scale(1.5, 0.5).unwrap())
            .unwrap();
        let inverse = t.invert().unwrap();
        for (x, y) in [(0.0, 0.0), (3.0, -4.0), (100.0, 250.0)] {
            let mapped = t.apply(x, y);
            assert!(close(inverse.apply(mapped.0, mapped.1), (x, y)));
        }
    }

    #[test]
    fn singular_transform_has_no_inverse() {
        let flat = Transform::scale(0.0, 1.0).unwrap();
        assert_eq!(flat.invert(), Err(GeometryError::Singular));
    }

    #[test]
    fn mirrored_transform_has_negative_determinant() {
        assert!(Transform::scale(-1.0, 1.0).unwrap().determinant() < 0.0);
        assert!(Transform::rotate(1.0).unwrap().determinant() > 0.0);
    }

    #[test]
    fn composition_reports_overflow_instead_of_producing_infinity() {
        let big = Transform::scale(Scalar::MAX_MAGNITUDE, 1.0).unwrap();
        assert!(big.compose(&big).is_err());
    }

    #[test]
    fn non_finite_coefficients_are_rejected() {
        assert!(Transform::new([f64::NAN, 0.0, 0.0, 1.0, 0.0, 0.0]).is_err());
        assert!(Transform::rotate(f64::INFINITY).is_err());
    }
}
