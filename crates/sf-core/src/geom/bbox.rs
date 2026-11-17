use super::Point;
use serde::{Deserialize, Serialize};

/// Axis-aligned bounding box. An empty box has `min > max`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct BBox {
    pub min: Point,
    pub max: Point,
}

impl Default for BBox {
    fn default() -> Self {
        Self::EMPTY
    }
}

impl BBox {
    pub const EMPTY: BBox = BBox { min: Point { x: f64::INFINITY, y: f64::INFINITY }, max: Point { x: f64::NEG_INFINITY, y: f64::NEG_INFINITY } };

    pub fn new(a: Point, b: Point) -> Self {
        BBox { min: a.min(b), max: a.max(b) }
    }

    pub fn from_points<I: IntoIterator<Item = Point>>(pts: I) -> Self {
        let mut b = BBox::EMPTY;
        for p in pts {
            b.include(p);
        }
        b
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        !(self.min.x <= self.max.x && self.min.y <= self.max.y)
    }

    #[inline]
    pub fn include(&mut self, p: Point) {
        self.min = self.min.min(p);
        self.max = self.max.max(p);
    }

    pub fn union(self, o: BBox) -> BBox {
        if self.is_empty() {
            return o;
        }
        if o.is_empty() {
            return self;
        }
        BBox { min: self.min.min(o.min), max: self.max.max(o.max) }
    }

    #[inline]
    pub fn width(&self) -> f64 {
        if self.is_empty() { 0.0 } else { self.max.x - self.min.x }
    }

    #[inline]
    pub fn height(&self) -> f64 {
        if self.is_empty() { 0.0 } else { self.max.y - self.min.y }
    }

    #[inline]
    pub fn center(&self) -> Point {
        if self.is_empty() { Point::ZERO } else { self.min.midpoint(self.max) }
    }

    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.min.x && p.x <= self.max.x && p.y >= self.min.y && p.y <= self.max.y
    }

    pub fn contains_box(&self, o: &BBox) -> bool {
        !o.is_empty() && self.contains(o.min) && self.contains(o.max)
    }

    pub fn intersects(&self, o: &BBox) -> bool {
        !(self.is_empty() || o.is_empty()) && self.min.x <= o.max.x && o.min.x <= self.max.x && self.min.y <= o.max.y && o.min.y <= self.max.y
    }

    pub fn expanded(&self, d: f64) -> BBox {
        if self.is_empty() {
            return *self;
        }
        BBox { min: Point::new(self.min.x - d, self.min.y - d), max: Point::new(self.max.x + d, self.max.y + d) }
    }

    /// The four corners, clockwise on screen starting top-left.
    pub fn corners(&self) -> [Point; 4] {
        [self.min, Point::new(self.max.x, self.min.y), self.max, Point::new(self.min.x, self.max.y)]
    }
}
