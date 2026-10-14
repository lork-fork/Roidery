//! Gradient meshes: `/Mesh X!`, then on `%_` lines `value /key X#` entries — the colour space
//! (`/CS`), and per patch its place (`/P [row col]`) and four corners (`/N`, from the corner nearest
//! the origin round to the one beside it), ended by `/E` — and `/End X!`.
//!
//! A corner is `[colour… opacity x y` then the handle towards the next corner, a weight, the handle
//! towards the previous one and a weight `]`, in art space. Each patch becomes a 1×1 mesh.
//!
//! `X!`/`X#` aren't publicly documented (the AI7 specification predates meshes).

use kurbo::{Affine, Point, Vec2};
use vectorcraft_color::Color;
use vectorcraft_doc::live::{GradientMesh, H_DOWN, H_LEFT, H_RIGHT, H_UP, MeshPoint};

use super::obj::V;

/// The most patches read from one mesh.
const MAX_PATCHES: usize = 4096;

#[derive(Debug, Default)]
pub(super) struct MeshReader {
    space: String,
    corners: Vec<Vec<f64>>,
    pub patches: Vec<GradientMesh>,
    /// Something in it this doesn't read.
    pub unread: bool,
}

impl MeshReader {
    /// `value /key X#`; `to_doc`: art space → the document.
    pub fn entry(&mut self, vals: &[V], to_doc: Affine) {
        let Some(at) = vals.iter().rposition(|v| matches!(v, V::Name(_))) else { return };
        let key = vals.get(at).and_then(V::text).unwrap_or_default();
        let value = at.checked_sub(1).and_then(|i| vals.get(i));
        match key.as_str() {
            "CS" => self.space = value.and_then(V::text).unwrap_or_default(),
            "N" => {
                if let Some(V::Arr(a)) = value {
                    if self.corners.len() < 4 {
                        self.corners.push(a.iter().filter_map(V::num).take(64).collect());
                    } else {
                        self.unread = true;
                    }
                }
            }
            "E" => {
                let corners = std::mem::take(&mut self.corners);
                match self.patch(&corners, to_doc) {
                    Some(m) if self.patches.len() < MAX_PATCHES => self.patches.push(m),
                    _ => self.unread = true,
                }
            }
            _ => {}
        }
    }

    fn color(&self, c: &[f64]) -> Option<Color> {
        let u = |i: usize| c.get(i).filter(|v| v.is_finite()).map(|v| v.clamp(0.0, 1.0) as f32);
        Some(match (self.space.as_str(), c.len()) {
            ("DeviceRGB", 3) => Color::rgb(u(0)?, u(1)?, u(2)?),
            ("DeviceCMYK", 4) => Color::cmyk(u(0)?, u(1)?, u(2)?, u(3)?),
            ("DeviceGray", 1) => Color::gray(1.0 - u(0)?),
            _ => return None,
        })
    }

    fn patch(&self, corners: &[Vec<f64>], to_doc: Affine) -> Option<GradientMesh> {
        let [n0, n1, n2, n3] = corners else { return None };
        // (anchor, towards the next corner, towards the previous one, colour, opacity)
        let read = |n: &Vec<f64>| -> Option<(Point, Vec2, Vec2, Color, f32)> {
            let comps = n.len().checked_sub(9)?;
            let &[opacity, x, y, nx, ny, _, px, py, _] = n.get(comps..)? else { return None };
            if ![opacity, x, y, nx, ny, px, py].iter().all(|v| v.is_finite()) {
                return None;
            }
            let at = to_doc * Point::new(x, y);
            let color = self.color(n.get(..comps)?)?;
            Some((at, to_doc * Point::new(nx, ny) - at, to_doc * Point::new(px, py) - at, color, opacity.clamp(0.0, 1.0) as f32))
        };
        let (a, b, c, d) = (read(n0)?, read(n1)?, read(n2)?, read(n3)?);
        // Rows run from the first corner to the second, columns from the first to the fourth.
        let point = |(p, _, _, color, opacity): (Point, Vec2, Vec2, Color, f32), handles: [(usize, Vec2); 2]| {
            let mut h = [Vec2::ZERO; 4];
            for (i, v) in handles {
                if let Some(slot) = h.get_mut(i) {
                    *slot = v;
                }
            }
            MeshPoint { p, color, opacity, handles: h }
        };
        let points = vec![
            point(a, [(H_DOWN, a.1), (H_RIGHT, a.2)]),
            point(d, [(H_LEFT, d.1), (H_DOWN, d.2)]),
            point(b, [(H_RIGHT, b.1), (H_UP, b.2)]),
            point(c, [(H_UP, c.1), (H_LEFT, c.2)]),
        ];
        Some(GradientMesh { rows: 1, cols: 1, points })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn corner(values: &[f64]) -> Vec<V> {
        vec![V::Arr(values.iter().map(|v| V::Num(*v)).collect()), V::Name("N".into())]
    }

    fn patch(reader: &mut MeshReader, first: &[f64]) {
        reader.entry(&corner(first), Affine::IDENTITY);
        for (x, y) in [(0.0, 1.0), (1.0, 1.0), (1.0, 0.0)] {
            reader.entry(&corner(&[0.5, 1.0, x, y, x, y, 1.0, x, y, 1.0]), Affine::IDENTITY);
        }
        reader.entry(&[V::Num(0.0), V::Name("E".into())], Affine::IDENTITY);
    }

    #[test]
    fn hostile_meshes_are_capped_and_checked() {
        let mut reader = MeshReader { space: "DeviceGray".into(), ..MeshReader::default() };
        for _ in 0..=MAX_PATCHES {
            patch(&mut reader, &[0.5, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0]);
        }
        assert!(reader.unread && reader.patches.len() == MAX_PATCHES);
        // NaN passes `clamp`: a non-finite opacity or colour leaves the patch unread.
        for bad in [[0.5, f64::NAN], [f64::INFINITY, 1.0]] {
            let mut reader = MeshReader { space: "DeviceGray".into(), ..MeshReader::default() };
            patch(&mut reader, &[bad[0], bad[1], 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0]);
            assert!(reader.unread && reader.patches.is_empty(), "{bad:?}");
        }
        // More corners than a patch has.
        let mut reader = MeshReader { space: "DeviceGray".into(), ..MeshReader::default() };
        for _ in 0..1000 {
            reader.entry(&corner(&[0.5, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0]), Affine::IDENTITY);
        }
        assert!(reader.unread && reader.corners.len() == 4);
    }
}
