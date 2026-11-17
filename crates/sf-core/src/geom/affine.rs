use super::Point;
use serde::{Deserialize, Serialize};

/// 2D affine transform `[a, b, c, d, e, f]`:
///
/// ```text
/// x' = a·x + c·y + e
/// y' = b·x + d·y + f
/// ```
///
/// (the same layout as SVG's `matrix(a b c d e f)`).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Affine(pub [f64; 6]);

impl Default for Affine {
    fn default() -> Self {
        Affine::IDENTITY
    }
}

impl Affine {
    pub const IDENTITY: Affine = Affine([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);

    pub fn translate(d: Point) -> Affine {
        Affine([1.0, 0.0, 0.0, 1.0, d.x, d.y])
    }

    pub fn scale(sx: f64, sy: f64) -> Affine {
        Affine([sx, 0.0, 0.0, sy, 0.0, 0.0])
    }

    /// Rotation by `radians` in the mathematical sense of the coordinate system
    /// (in y-down design space a positive angle turns clockwise on screen).
    pub fn rotate(radians: f64) -> Affine {
        let (s, c) = radians.sin_cos();
        Affine([c, s, -s, c, 0.0, 0.0])
    }

    /// Skew: `x' = x + kx·y`, `y' = y + ky·x`.
    pub fn skew(kx: f64, ky: f64) -> Affine {
        Affine([1.0, ky, kx, 1.0, 0.0, 0.0])
    }

    /// Conjugates `self` so that it acts around `center` instead of the origin.
    pub fn about(self, center: Point) -> Affine {
        Affine::translate(-center).then(&self).then(&Affine::translate(center))
    }

    pub fn rotate_about(radians: f64, center: Point) -> Affine {
        Affine::rotate(radians).about(center)
    }

    pub fn scale_about(sx: f64, sy: f64, center: Point) -> Affine {
        Affine::scale(sx, sy).about(center)
    }

    /// Composition: first apply `self`, then `next`.
    pub fn then(&self, next: &Affine) -> Affine {
        let [a1, b1, c1, d1, e1, f1] = self.0;
        let [a2, b2, c2, d2, e2, f2] = next.0;
        Affine([a2 * a1 + c2 * b1, b2 * a1 + d2 * b1, a2 * c1 + c2 * d1, b2 * c1 + d2 * d1, a2 * e1 + c2 * f1 + e2, b2 * e1 + d2 * f1 + f2])
    }

    #[inline]
    pub fn apply(&self, p: Point) -> Point {
        let [a, b, c, d, e, f] = self.0;
        Point::new(a * p.x + c * p.y + e, b * p.x + d * p.y + f)
    }

    /// Applies only the linear part (no translation) – for direction vectors.
    #[inline]
    pub fn apply_vector(&self, v: Point) -> Point {
        let [a, b, c, d, _, _] = self.0;
        Point::new(a * v.x + c * v.y, b * v.x + d * v.y)
    }

    pub fn determinant(&self) -> f64 {
        let [a, b, c, d, _, _] = self.0;
        a * d - b * c
    }

    pub fn invert(&self) -> Option<Affine> {
        let det = self.determinant();
        if det.abs() < 1e-15 {
            return None;
        }
        let [a, b, c, d, e, f] = self.0;
        let inv = 1.0 / det;
        let na = d * inv;
        let nb = -b * inv;
        let nc = -c * inv;
        let nd = a * inv;
        Some(Affine([na, nb, nc, nd, -(na * e + nc * f), -(nb * e + nd * f)]))
    }

    pub fn is_identity(&self) -> bool {
        self.0.iter().zip(Affine::IDENTITY.0.iter()).all(|(x, y)| (x - y).abs() < 1e-12)
    }

    /// True when the transform mirrors (negative determinant).
    pub fn is_mirroring(&self) -> bool {
        self.determinant() < 0.0
    }

    /// Mean linear scale factor (geometric mean of the axis scales).
    pub fn mean_scale(&self) -> f64 {
        self.determinant().abs().sqrt()
    }

    pub fn translation(&self) -> Point {
        Point::new(self.0[4], self.0[5])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compose_and_invert() {
        let m = Affine::rotate_about(0.7, Point::new(3.0, 4.0)).then(&Affine::scale(2.0, 0.5)).then(&Affine::translate(Point::new(-1.0, 9.0)));
        let p = Point::new(1.25, -7.5);
        let q = m.apply(p);
        let back = m.invert().unwrap().apply(q);
        assert!(back.approx_eq(p, 1e-9));
    }

    #[test]
    fn then_order() {
        let t = Affine::translate(Point::new(10.0, 0.0));
        let s = Affine::scale(2.0, 2.0);
        // translate first, then scale
        assert!(t.then(&s).apply(Point::new(1.0, 0.0)).approx_eq(Point::new(22.0, 0.0), 1e-12));
        // scale first, then translate
        assert!(s.then(&t).apply(Point::new(1.0, 0.0)).approx_eq(Point::new(12.0, 0.0), 1e-12));
    }
}
