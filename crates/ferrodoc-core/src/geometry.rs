//! Validated page geometry.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{CoreError, error::invalid_number};

/// Coordinate-space identity carried by every rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename_all = "snake_case")]
pub enum CoordinateSpace {
    /// PDF user space.
    Pdf,
    /// A raster image.
    Image,
    /// Page-normalized coordinates in the inclusive range zero through one.
    Normalized,
}

/// Unit associated with a coordinate space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename_all = "snake_case")]
pub enum Unit {
    /// One PDF point, equal to 1/72 inch.
    Point,
    /// One raster pixel.
    Pixel,
    /// Unitless normalized ratio.
    Ratio,
}

/// A finite, nonnegative-size rectangle in an explicit space and unit.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "RawRect")]
pub struct Rect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    space: CoordinateSpace,
    unit: Unit,
}

#[derive(Deserialize)]
struct RawRect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    space: CoordinateSpace,
    unit: Unit,
}

impl TryFrom<RawRect> for Rect {
    type Error = CoreError;

    fn try_from(raw: RawRect) -> Result<Self, Self::Error> {
        Self::new(raw.x, raw.y, raw.width, raw.height, raw.space, raw.unit)
    }
}

impl Rect {
    /// Creates a rectangle after validating coordinates, dimensions, edges, space, and unit.
    pub fn new(
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        space: CoordinateSpace,
        unit: Unit,
    ) -> Result<Self, CoreError> {
        if ![x, y, width, height].into_iter().all(f64::is_finite) {
            return Err(invalid_number("rectangle", "all values must be finite"));
        }
        if width < 0.0 || height < 0.0 {
            return Err(invalid_number(
                "rectangle",
                "width and height must be nonnegative",
            ));
        }
        if !matches!(
            (space, unit),
            (CoordinateSpace::Pdf, Unit::Point)
                | (CoordinateSpace::Image, Unit::Pixel)
                | (CoordinateSpace::Normalized, Unit::Ratio)
        ) {
            return Err(CoreError::IncompatibleGeometry(
                "coordinate space and unit do not match",
            ));
        }
        let right = x + width;
        let bottom = y + height;
        if !right.is_finite() || !bottom.is_finite() {
            return Err(invalid_number("rectangle", "an edge overflowed"));
        }
        if space == CoordinateSpace::Normalized
            && (x < 0.0 || y < 0.0 || right > 1.0 || bottom > 1.0)
        {
            return Err(invalid_number(
                "rectangle",
                "normalized coordinates must lie within zero and one",
            ));
        }
        Ok(Self {
            x,
            y,
            width,
            height,
            space,
            unit,
        })
    }

    /// Left edge.
    pub const fn x(self) -> f64 {
        self.x
    }

    /// Top edge.
    pub const fn y(self) -> f64 {
        self.y
    }

    /// Width.
    pub const fn width(self) -> f64 {
        self.width
    }

    /// Height.
    pub const fn height(self) -> f64 {
        self.height
    }

    /// Coordinate space.
    pub const fn space(self) -> CoordinateSpace {
        self.space
    }

    /// Coordinate unit.
    pub const fn unit(self) -> Unit {
        self.unit
    }

    /// Right edge, proven finite by construction.
    pub fn right(self) -> f64 {
        self.x + self.width
    }

    /// Bottom edge, proven finite by construction.
    pub fn bottom(self) -> f64 {
        self.y + self.height
    }

    /// Area in squared coordinate units.
    pub fn area(self) -> f64 {
        self.width * self.height
    }

    /// Returns the positive-area intersection. Touching edges have no intersection.
    pub fn intersection(self, other: Self) -> Result<Option<Self>, CoreError> {
        self.ensure_compatible(other)?;
        let left = self.x.max(other.x);
        let top = self.y.max(other.y);
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        if right <= left || bottom <= top {
            return Ok(None);
        }
        Self::new(left, top, right - left, bottom - top, self.space, self.unit).map(Some)
    }

    /// Computes intersection-over-union for compatible rectangles.
    pub fn iou(self, other: Self) -> Result<f64, CoreError> {
        let intersection = self.intersection(other)?.map_or(0.0, Self::area);
        if intersection == 0.0 {
            return Ok(0.0);
        }
        Ok(intersection / (self.area() + other.area() - intersection))
    }

    /// Clips this rectangle to `bounds`, calculating every original edge before construction.
    pub fn clipped_to(self, bounds: Self) -> Result<Option<Self>, CoreError> {
        self.intersection(bounds)
    }

    /// Expands all edges by a nonnegative margin and optionally clips to bounds.
    pub fn expanded(self, margin: f64, bounds: Option<Self>) -> Result<Option<Self>, CoreError> {
        if !margin.is_finite() || margin < 0.0 {
            return Err(invalid_number(
                "rectangle margin",
                "margin must be finite and nonnegative",
            ));
        }
        let left = self.x - margin;
        let top = self.y - margin;
        let right = self.right() + margin;
        let bottom = self.bottom() + margin;
        let expanded = Self::new(left, top, right - left, bottom - top, self.space, self.unit)?;
        match bounds {
            Some(bounds) => expanded.clipped_to(bounds),
            None => Ok(Some(expanded)),
        }
    }

    /// Translates a rectangle with checked finite edges.
    pub fn translated(self, dx: f64, dy: f64) -> Result<Self, CoreError> {
        if !dx.is_finite() || !dy.is_finite() {
            return Err(invalid_number(
                "rectangle translation",
                "offsets must be finite",
            ));
        }
        Self::new(
            self.x + dx,
            self.y + dy,
            self.width,
            self.height,
            self.space,
            self.unit,
        )
    }

    /// Returns true when `other` lies within this rectangle, including its boundary.
    pub fn contains(self, other: Self) -> Result<bool, CoreError> {
        self.ensure_compatible(other)?;
        Ok(other.x >= self.x
            && other.y >= self.y
            && other.right() <= self.right()
            && other.bottom() <= self.bottom())
    }

    fn ensure_compatible(self, other: Self) -> Result<(), CoreError> {
        if self.space == other.space && self.unit == other.unit {
            Ok(())
        } else {
            Err(CoreError::IncompatibleGeometry(
                "rectangles use different spaces or units",
            ))
        }
    }
}

/// A finite two-dimensional affine transform `[a, b, c, d, e, f]`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "[f64; 6]", into = "[f64; 6]")]
pub struct CoordinateTransform([f64; 6]);

impl CoordinateTransform {
    /// Identity transform.
    pub const IDENTITY: Self = Self([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);

    /// Creates a transform with finite coefficients.
    pub fn new(coefficients: [f64; 6]) -> Result<Self, CoreError> {
        if coefficients.into_iter().all(f64::is_finite) {
            Ok(Self(coefficients))
        } else {
            Err(invalid_number(
                "coordinate transform",
                "all coefficients must be finite",
            ))
        }
    }

    /// Returns the affine coefficients.
    pub const fn coefficients(self) -> [f64; 6] {
        self.0
    }
}

impl TryFrom<[f64; 6]> for CoordinateTransform {
    type Error = CoreError;

    fn try_from(value: [f64; 6]) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<CoordinateTransform> for [f64; 6] {
    fn from(value: CoordinateTransform) -> Self {
        value.0
    }
}

/// Geometry associated with a zero-based page index and source transform.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PageRect {
    /// Zero-based page index.
    pub page_index: u32,
    /// Rectangle in its declared coordinate space.
    pub rect: Rect,
    /// Transform from the source coordinate system to the rectangle space.
    pub source_transform: CoordinateTransform,
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn pixel_rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
        Rect::new(x, y, width, height, CoordinateSpace::Image, Unit::Pixel).unwrap()
    }

    #[test]
    fn rejects_invalid_values_and_extreme_margins() {
        assert!(Rect::new(f64::NAN, 0.0, 1.0, 1.0, CoordinateSpace::Image, Unit::Pixel).is_err());
        assert!(Rect::new(0.0, 0.0, -1.0, 1.0, CoordinateSpace::Image, Unit::Pixel).is_err());
        assert!(
            pixel_rect(0.0, 0.0, 1.0, 1.0)
                .expanded(f64::MAX, None)
                .is_err()
        );
    }

    #[test]
    fn touching_edges_have_zero_area_intersection() {
        let left = pixel_rect(0.0, 0.0, 10.0, 10.0);
        let right = pixel_rect(10.0, 0.0, 4.0, 10.0);
        assert_eq!(left.intersection(right).unwrap(), None);
        assert_eq!(left.iou(right).unwrap(), 0.0);
    }

    proptest! {
        #[test]
        fn iou_is_symmetric(
            ax in -1000.0f64..1000.0, ay in -1000.0f64..1000.0,
            aw in 0.0f64..1000.0, ah in 0.0f64..1000.0,
            bx in -1000.0f64..1000.0, by in -1000.0f64..1000.0,
            bw in 0.0f64..1000.0, bh in 0.0f64..1000.0,
        ) {
            let a = pixel_rect(ax, ay, aw, ah);
            let b = pixel_rect(bx, by, bw, bh);
            prop_assert!((a.iou(b).unwrap() - b.iou(a).unwrap()).abs() < 1e-12);
        }

        #[test]
        fn clipping_is_contained(
            x in 0.0f64..100.0, y in 0.0f64..100.0,
            width in 0.1f64..100.0, height in 0.1f64..100.0,
        ) {
            let bounds = pixel_rect(0.0, 0.0, 100.0, 100.0);
            let candidate = pixel_rect(x, y, width, height);
            if let Some(clipped) = candidate.clipped_to(bounds).unwrap() {
                prop_assert!(bounds.contains(clipped).unwrap());
            }
        }

        #[test]
        fn expansion_contains_original(
            x in -1000.0f64..1000.0, y in -1000.0f64..1000.0,
            width in 0.0f64..1000.0, height in 0.0f64..1000.0,
            margin in 0.0f64..1000.0,
        ) {
            let rect = pixel_rect(x, y, width, height);
            let expanded = rect.expanded(margin, None).unwrap().unwrap();
            prop_assert!(expanded.contains(rect).unwrap());
        }

        #[test]
        fn iou_is_translation_invariant(
            ax in -100.0f64..100.0, ay in -100.0f64..100.0,
            aw in 0.1f64..100.0, ah in 0.1f64..100.0,
            bx in -100.0f64..100.0, by in -100.0f64..100.0,
            bw in 0.1f64..100.0, bh in 0.1f64..100.0,
            dx in -1000.0f64..1000.0, dy in -1000.0f64..1000.0,
        ) {
            let a = pixel_rect(ax, ay, aw, ah);
            let b = pixel_rect(bx, by, bw, bh);
            let translated_a = a.translated(dx, dy).unwrap();
            let translated_b = b.translated(dx, dy).unwrap();
            prop_assert!((a.iou(b).unwrap() - translated_a.iou(translated_b).unwrap()).abs() < 1e-12);
        }

        #[test]
        fn serialization_round_trip(
            x in -1000.0f64..1000.0, y in -1000.0f64..1000.0,
            width in 0.0f64..1000.0, height in 0.0f64..1000.0,
        ) {
            let rect = pixel_rect(x, y, width, height);
            let json = serde_json::to_vec(&rect).unwrap();
            prop_assert_eq!(serde_json::from_slice::<Rect>(&json).unwrap(), rect);
        }
    }
}
