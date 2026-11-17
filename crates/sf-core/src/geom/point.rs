use serde::{Deserialize, Serialize};
use std::ops::{Add, AddAssign, Div, Mul, MulAssign, Neg, Sub, SubAssign};

/// A point or vector in design space (millimetres, +y down).
///
/// Serialised compactly as `[x, y]`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(from = "[f64; 2]", into = "[f64; 2]")]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl From<[f64; 2]> for Point {
    #[inline]
    fn from(v: [f64; 2]) -> Self {
        Point::new(v[0], v[1])
    }
}

impl From<Point> for [f64; 2] {
    #[inline]
    fn from(p: Point) -> Self {
        [p.x, p.y]
    }
}

impl From<(f64, f64)> for Point {
    #[inline]
    fn from(v: (f64, f64)) -> Self {
        Point::new(v.0, v.1)
    }
}

impl Point {
    pub const ZERO: Point = Point { x: 0.0, y: 0.0 };

    #[inline]
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    #[inline]
    pub fn dot(self, o: Point) -> f64 {
        self.x * o.x + self.y * o.y
    }

    /// z-component of the 3D cross product.
    #[inline]
    pub fn cross(self, o: Point) -> f64 {
        self.x * o.y - self.y * o.x
    }

    #[inline]
    pub fn length(self) -> f64 {
        self.x.hypot(self.y)
    }

    #[inline]
    pub fn length_sq(self) -> f64 {
        self.x * self.x + self.y * self.y
    }

    #[inline]
    pub fn distance(self, o: Point) -> f64 {
        (self - o).length()
    }

    #[inline]
    pub fn distance_sq(self, o: Point) -> f64 {
        (self - o).length_sq()
    }

    /// Unit vector in the same direction, or zero for a (near) zero vector.
    #[inline]
    pub fn normalized(self) -> Point {
        let l = self.length();
        if l > 1e-12 { self / l } else { Point::ZERO }
    }

    /// Rotates by +90° in the mathematical sense: `(x, y) -> (-y, x)`.
    #[inline]
    pub fn perp(self) -> Point {
        Point::new(-self.y, self.x)
    }

    #[inline]
    pub fn rotated(self, radians: f64) -> Point {
        let (s, c) = radians.sin_cos();
        Point::new(self.x * c - self.y * s, self.x * s + self.y * c)
    }

    #[inline]
    pub fn rotated_about(self, center: Point, radians: f64) -> Point {
        (self - center).rotated(radians) + center
    }

    #[inline]
    pub fn lerp(self, o: Point, t: f64) -> Point {
        self + (o - self) * t
    }

    #[inline]
    pub fn midpoint(self, o: Point) -> Point {
        Point::new((self.x + o.x) * 0.5, (self.y + o.y) * 0.5)
    }

    /// Angle of the vector in radians, `atan2(y, x)`.
    #[inline]
    pub fn angle(self) -> f64 {
        self.y.atan2(self.x)
    }

    #[inline]
    pub fn from_angle(radians: f64) -> Point {
        let (s, c) = radians.sin_cos();
        Point::new(c, s)
    }

    #[inline]
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }

    #[inline]
    pub fn min(self, o: Point) -> Point {
        Point::new(self.x.min(o.x), self.y.min(o.y))
    }

    #[inline]
    pub fn max(self, o: Point) -> Point {
        Point::new(self.x.max(o.x), self.y.max(o.y))
    }

    /// Approximate equality within `eps` on both axes.
    #[inline]
    pub fn approx_eq(self, o: Point, eps: f64) -> bool {
        (self.x - o.x).abs() <= eps && (self.y - o.y).abs() <= eps
    }
}

impl Add for Point {
    type Output = Point;
    #[inline]
    fn add(self, o: Point) -> Point {
        Point::new(self.x + o.x, self.y + o.y)
    }
}

impl AddAssign for Point {
    #[inline]
    fn add_assign(&mut self, o: Point) {
        self.x += o.x;
        self.y += o.y;
    }
}

impl Sub for Point {
    type Output = Point;
    #[inline]
    fn sub(self, o: Point) -> Point {
        Point::new(self.x - o.x, self.y - o.y)
    }
}

impl SubAssign for Point {
    #[inline]
    fn sub_assign(&mut self, o: Point) {
        self.x -= o.x;
        self.y -= o.y;
    }
}

impl Mul<f64> for Point {
    type Output = Point;
    #[inline]
    fn mul(self, s: f64) -> Point {
        Point::new(self.x * s, self.y * s)
    }
}

impl MulAssign<f64> for Point {
    #[inline]
    fn mul_assign(&mut self, s: f64) {
        self.x *= s;
        self.y *= s;
    }
}

impl Div<f64> for Point {
    type Output = Point;
    #[inline]
    fn div(self, s: f64) -> Point {
        Point::new(self.x / s, self.y / s)
    }
}

impl Neg for Point {
    type Output = Point;
    #[inline]
    fn neg(self) -> Point {
        Point::new(-self.x, -self.y)
    }
}

/// Direction vector for an angle given in degrees **counter-clockwise on screen**
/// (the convention used for stitch angles throughout StitchFoundry, as in most
/// digitizing software). Because design space is y-down, this is `(cos θ, -sin θ)`.
#[inline]
pub fn screen_dir(degrees: f64) -> Point {
    let r = degrees.to_radians();
    Point::new(r.cos(), -r.sin())
}

/// Inverse of [`screen_dir`]: the on-screen counter-clockwise angle (degrees) of a vector.
#[inline]
pub fn screen_angle(v: Point) -> f64 {
    (-v.y).atan2(v.x).to_degrees()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_is_compact() {
        let p = Point::new(1.5, -2.0);
        let s = serde_json::to_string(&p).unwrap();
        assert_eq!(s, "[1.5,-2.0]");
        let q: Point = serde_json::from_str(&s).unwrap();
        assert_eq!(p, q);
    }

    #[test]
    fn screen_angles_round_trip() {
        for deg in [0.0, 30.0, 45.0, 90.0, 135.0, -60.0] {
            let v = screen_dir(deg);
            assert!((screen_angle(v) - deg).abs() < 1e-9);
        }
        // 90° points up the screen, i.e. towards -y.
        assert!(screen_dir(90.0).approx_eq(Point::new(0.0, -1.0), 1e-12));
    }
}
