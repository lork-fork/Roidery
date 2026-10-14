//! Roidery mockups: what a design will look like sewn, as a picture to show a customer.
//!
//! Two steps on an RGBA image (straight alpha, row-major): [`texture`] gives the fabric under
//! the design its weave (woven, knit, piqué, terry, fleece, denim, twill or leather), and
//! [`draw_threads`] draws each stitch as a strand of thread over a soft shadow. The thread is
//! shaded as the realistic view on the canvas shades it ([`shade`]: two twisted plies lit from
//! the top left, with a sheen along the fibres), computed here on the CPU so it runs anywhere,
//! the web included.
//!
//! The garment itself (its outline and colour) is drawn by the caller; everything here is
//! generated in code, with no image assets.
#![forbid(unsafe_code)]

pub mod shade;

/// Largest image side, and pixel count, a mockup may have.
pub const MAX_SIDE: u32 = 8000;
pub const MAX_PIXELS: u64 = 40_000_000;

/// An RGBA8 image, straight (not premultiplied) alpha, row-major.
#[derive(Clone, Debug, PartialEq)]
pub struct Canvas {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Canvas {
    /// A `width` × `height` image filled with `rgba`; `None` when it is empty or too large.
    pub fn new(width: u32, height: u32, rgba: [u8; 4]) -> Option<Self> {
        if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE || u64::from(width) * u64::from(height) > MAX_PIXELS {
            return None;
        }
        let n = width as usize * height as usize;
        let mut v = Vec::with_capacity(n * 4);
        for _ in 0..n {
            v.extend_from_slice(&rgba);
        }
        Some(Self { width, height, rgba: v })
    }

    /// An image from pixels; `None` when their count doesn't match the size.
    pub fn from_rgba(width: u32, height: u32, rgba: Vec<u8>) -> Option<Self> {
        (rgba.len() as u64 == u64::from(width) * u64::from(height) * 4).then_some(Self { width, height, rgba })
    }

    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        let i = (y as usize * self.width as usize + x as usize) * 4;
        let p = self.rgba.get(i..i + 4)?;
        Some([p[0], p[1], p[2], p[3]])
    }

    /// Paint `rgb` (0..1) over pixel (`x`, `y`) with coverage `a`.
    fn blend(&mut self, x: usize, y: usize, rgb: [f64; 3], a: f64) {
        let i = (y * self.width as usize + x) * 4;
        let Some(p) = self.rgba.get_mut(i..i + 4) else { return };
        let a = a.clamp(0.0, 1.0);
        let da = f64::from(p[3]) / 255.0;
        let out_a = a + da * (1.0 - a);
        if out_a <= 0.0 {
            return;
        }
        for (k, c) in rgb.iter().enumerate() {
            let d = f64::from(p[k]) / 255.0;
            let v = (c.clamp(0.0, 1.0) * a + d * da * (1.0 - a)) / out_a;
            p[k] = (v * 255.0).round() as u8;
        }
        p[3] = (out_a * 255.0).round() as u8;
    }

    /// Lay the image over a solid `rgb` background (every pixel opaque after).
    pub fn flatten(&mut self, rgb: [u8; 3]) {
        for p in self.rgba.chunks_exact_mut(4) {
            let a = u32::from(p[3]);
            for k in 0..3 {
                p[k] = ((u32::from(p[k]) * a + u32::from(rgb[k]) * (255 - a) + 127) / 255) as u8;
            }
            p[3] = 255;
        }
    }
}

/// A fabric's weave.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Texture {
    Woven,
    Knit,
    Pique,
    Terry,
    Fleece,
    Denim,
    Twill,
    Leather,
    /// No weave: the colour only.
    Smooth,
}

impl Texture {
    pub const ALL: [Texture; 9] = [
        Texture::Woven,
        Texture::Knit,
        Texture::Pique,
        Texture::Terry,
        Texture::Fleece,
        Texture::Denim,
        Texture::Twill,
        Texture::Leather,
        Texture::Smooth,
    ];

    /// The texture for a fabric id (Roidery's fabric presets use the same ids); unknown: woven.
    pub fn from_id(id: &str) -> Self {
        match id {
            "knit" => Texture::Knit,
            "pique" => Texture::Pique,
            "terry" => Texture::Terry,
            "fleece" => Texture::Fleece,
            "denim" => Texture::Denim,
            "twill" => Texture::Twill,
            "leather" => Texture::Leather,
            "smooth" => Texture::Smooth,
            _ => Texture::Woven,
        }
    }
}

fn smoothstep(e0: f64, e1: f64, x: f64) -> f64 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A repeatable pseudo-random number in 0..1 for a grid cell.
fn hash(x: i64, y: i64) -> f64 {
    let mut h = (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (y as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    h ^= h >> 31;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 29;
    (h >> 11) as f64 / (1u64 << 53) as f64
}

/// Smooth value noise in 0..1 with features about one unit across.
fn noise(x: f64, y: f64) -> f64 {
    let (fx, fy) = (x.floor(), y.floor());
    let (ix, iy) = (fx as i64, fy as i64);
    let (tx, ty) = (x - fx, y - fy);
    let (sx, sy) = (tx * tx * (3.0 - 2.0 * tx), ty * ty * (3.0 - 2.0 * ty));
    let a = hash(ix, iy) + (hash(ix + 1, iy) - hash(ix, iy)) * sx;
    let b = hash(ix, iy + 1) + (hash(ix + 1, iy + 1) - hash(ix, iy + 1)) * sx;
    a + (b - a) * sy
}

/// How much of a pattern repeating every `pitch_mm` to keep at `ppm` pixels per millimetre: none
/// when it would be under two pixels (it would only make moiré).
fn keep(pitch_mm: f64, ppm: f64) -> f64 {
    smoothstep(2.0, 5.0, pitch_mm * ppm)
}

const TAU: f64 = std::f64::consts::TAU;

/// The brightness of `t` at (`u`, `v`) mm, about 1.
fn weave(t: Texture, u: f64, v: f64, ppm: f64) -> f64 {
    // A little unevenness everywhere, as real cloth has.
    let mottle = (noise(u / 12.0, v / 12.0) - 0.5) * 0.025;
    let grain = |pitch: f64, amp: f64| (noise(u / pitch, v / pitch) - 0.5) * 2.0 * amp * keep(pitch, ppm);
    mottle
        + 1.0
        + match t {
            Texture::Woven => {
                let p = 0.35;
                (TAU * u / p).sin() * (TAU * v / p).sin() * 0.05 * keep(p, ppm) + grain(0.5, 0.03)
            }
            Texture::Knit => {
                // Jersey: columns of Vs.
                let (wale, course) = (0.9, 0.7);
                let across = (u / wale).fract();
                let v_shape = (across - 0.5).abs();
                ((TAU * (v / course + v_shape)).sin() * 0.06 + (TAU * across).cos() * 0.03) * keep(course, ppm) + grain(0.4, 0.02)
            }
            Texture::Pique => {
                // Small raised cells in staggered rows.
                let cell = 1.1;
                let row = (v / cell).floor();
                let (cu, cv) = ((u / cell + row * 0.5).fract() - 0.5, (v / cell).fract() - 0.5);
                let d = (cu * cu + cv * cv).sqrt() * 2.0;
                (0.05 - d * d * 0.1) * keep(cell, ppm) + grain(0.4, 0.02)
            }
            Texture::Terry => grain(0.55, 0.12) + grain(1.6, 0.05),
            Texture::Fleece => grain(1.4, 0.05) + grain(0.35, 0.03),
            Texture::Denim => {
                // Twill lines, and slubs along the warp.
                let p = 0.6;
                (TAU * (u + v) / p).sin() * 0.07 * keep(p, ppm) + (noise(u * 2.5, v / 8.0) - 0.5) * 0.12 + grain(0.4, 0.02)
            }
            Texture::Twill => {
                let p = 0.5;
                (TAU * (u + v) / p).sin() * 0.06 * keep(p, ppm) + grain(0.5, 0.02)
            }
            Texture::Leather => {
                let pebble = noise(u / 0.8, v / 0.8);
                (pebble - 0.5) * 0.08 * keep(0.8, ppm) + grain(3.0, 0.03)
            }
            Texture::Smooth => 0.0,
        }
}

/// Give the painted pixels of `c` (alpha above 0) the weave of `t`, at `ppm` pixels per mm.
pub fn texture(c: &mut Canvas, t: Texture, ppm: f64) {
    if t == Texture::Smooth || !ppm.is_finite() || ppm <= 0.0 {
        return;
    }
    let w = c.width as usize;
    for (i, p) in c.rgba.chunks_exact_mut(4).enumerate() {
        if p[3] == 0 {
            continue;
        }
        let (x, y) = ((i % w) as f64 + 0.5, (i / w) as f64 + 0.5);
        let m = weave(t, x / ppm, y / ppm, ppm);
        for v in p.iter_mut().take(3) {
            *v = (f64::from(*v) * m).round().clamp(0.0, 255.0) as u8;
        }
    }
}

/// Light the painted pixels of `c` softly from the top left, as cloth on a table is lit:
/// `strength` 0 leaves them, 0.2 darkens the far corner by about a fifth.
pub fn light(c: &mut Canvas, strength: f64) {
    if !strength.is_finite() || strength <= 0.0 {
        return;
    }
    let (w, h) = (f64::from(c.width), f64::from(c.height));
    let (lx, ly) = (w * 0.25, h * 0.15);
    let reach = w.hypot(h);
    let cols = c.width as usize;
    for (i, p) in c.rgba.chunks_exact_mut(4).enumerate() {
        if p[3] == 0 {
            continue;
        }
        let (x, y) = ((i % cols) as f64, (i / cols) as f64);
        let m = 1.0 + strength * 0.25 - strength * ((x - lx).hypot(y - ly) / reach);
        for v in p.iter_mut().take(3) {
            *v = (f64::from(*v) * m).round().clamp(0.0, 255.0) as u8;
        }
    }
}

/// One stitch: the thread from `a` to `b`, in pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Segment {
    pub a: [f64; 2],
    pub b: [f64; 2],
    pub rgb: [u8; 3],
}

/// The shadow's strength and its offset, as a fraction of the thread's half width.
const SHADOW_ALPHA: f64 = 0.35;
const SHADOW_OFFSET: [f64; 2] = [0.35, 0.55];

/// Draw `segments` in order as thread `half_width` pixels across each side of its centre: their
/// shadows first, then the threads.
pub fn draw_threads(c: &mut Canvas, segments: &[Segment], half_width: f64) {
    if !half_width.is_finite() || half_width <= 0.0 {
        return;
    }
    let ok = |s: &Segment| s.a.iter().chain(s.b.iter()).all(|v| v.is_finite() && v.abs() < 1e7);
    for shadow in [true, false] {
        for (i, s) in segments.iter().enumerate().filter(|(_, s)| ok(s)) {
            strand(c, s, half_width, shadow, hash(i as i64, 7));
        }
    }
}

/// One thread (or its shadow).
fn strand(c: &mut Canvas, s: &Segment, hw0: f64, shadow: bool, seed: f64) {
    let hw = if shadow { hw0 * 1.25 } else { hw0 };
    let off = if shadow { [SHADOW_OFFSET[0] * hw0, SHADOW_OFFSET[1] * hw0] } else { [0.0, 0.0] };
    let a = [s.a[0] + off[0], s.a[1] + off[1]];
    let b = [s.b[0] + off[0], s.b[1] + off[1]];
    let d = [b[0] - a[0], b[1] - a[1]];
    let len = d[0].hypot(d[1]);
    let dir = if len > 1e-6 { [d[0] / len, d[1] / len] } else { [1.0, 0.0] };
    let n = [-dir[1], dir[0]];
    let (x0, x1) = (a[0].min(b[0]) - hw - 1.0, a[0].max(b[0]) + hw + 1.0);
    let (y0, y1) = (a[1].min(b[1]) - hw - 1.0, a[1].max(b[1]) + hw + 1.0);
    let (w, h) = (f64::from(c.width), f64::from(c.height));
    let (x0, x1) = (x0.max(0.0).floor() as usize, x1.min(w).ceil().max(0.0) as usize);
    let (y0, y1) = (y0.max(0.0).floor() as usize, y1.min(h).ceil().max(0.0) as usize);
    // The shadow fades out with the thread's round shape when it gets smaller than a pixel.
    let form = shade::form(hw);
    let color = s.rgb.map(|v| f64::from(v) / 255.0);
    for y in y0..y1 {
        for x in x0..x1 {
            let p = [x as f64 + 0.5 - a[0], y as f64 + 0.5 - a[1]];
            let lx = p[0] * dir[0] + p[1] * dir[1];
            let ly = p[0] * n[0] + p[1] * n[1];
            let q = [(lx - lx.clamp(0.0, len)) / hw, ly / hw];
            let dist = q[0].hypot(q[1]);
            let edge = ((1.0 - dist) * hw + 0.5).clamp(0.0, 1.0);
            if edge <= 0.0 {
                continue;
            }
            if shadow {
                let alpha = SHADOW_ALPHA * edge * (1.0 - smoothstep(0.1, 1.0, dist)) * form;
                c.blend(x, y, [0.0; 3], alpha);
                continue;
            }
            let rgb = shade::thread(color, shade::GLOSS, &shade::Point { along: lx, len, q, dir, hw, seed });
            c.blend(x, y, rgb, edge);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_are_bounded() {
        assert!(Canvas::new(0, 10, [0; 4]).is_none());
        assert!(Canvas::new(MAX_SIDE + 1, 10, [0; 4]).is_none());
        assert!(Canvas::new(7000, 7000, [0; 4]).is_none());
        assert!(Canvas::from_rgba(2, 2, vec![0; 15]).is_none());
        assert_eq!(Canvas::new(3, 2, [1, 2, 3, 4]).unwrap().pixel(2, 1), Some([1, 2, 3, 4]));
    }

    #[test]
    fn a_thread_is_lit_from_the_top_left_and_casts_a_shadow_down_right() {
        let mut c = Canvas::new(60, 40, [200, 200, 200, 255]).unwrap();
        let red = Segment { a: [10.0, 20.0], b: [50.0, 20.0], rgb: [200, 30, 30] };
        draw_threads(&mut c, &[red], 4.0);
        let px = |x, y| c.pixel(x, y).unwrap();
        // On the thread: red, brighter on its upper side than its lower.
        let (top, bottom) = (px(30, 17), px(30, 22));
        assert!(top[0] > top[1] * 2 && bottom[0] > bottom[1] * 2, "{top:?} {bottom:?}");
        assert!(u32::from(top[0]) + u32::from(top[1]) > u32::from(bottom[0]) + u32::from(bottom[1]), "{top:?} {bottom:?}");
        // Just below it, the shadow darkens the fabric; far away the fabric is untouched.
        assert!(px(30, 25)[1] < 200, "{:?}", px(30, 25));
        assert_eq!(px(30, 2), [200, 200, 200, 255]);
        // Off the image and non-finite segments are skipped.
        draw_threads(
            &mut c,
            &[Segment { a: [-100.0, -100.0], b: [-90.0, -90.0], rgb: [0; 3] }, Segment { a: [f64::NAN, 0.0], b: [1.0, 1.0], rgb: [0; 3] }],
            4.0,
        );
    }

    #[test]
    fn textures_change_the_cloth_but_keep_its_colour() {
        for t in Texture::ALL {
            let mut c = Canvas::new(80, 80, [120, 160, 200, 255]).unwrap();
            // A transparent pixel stays as it was.
            c.rgba[0..4].copy_from_slice(&[9, 9, 9, 0]);
            texture(&mut c, t, 12.0);
            assert_eq!(c.pixel(0, 0), Some([9, 9, 9, 0]));
            let px: Vec<[u8; 4]> = (1..80).map(|i| c.pixel(i, i).unwrap()).collect();
            let mean = px.iter().map(|p| f64::from(p[2])).sum::<f64>() / px.len() as f64;
            assert!((mean - 200.0).abs() < 20.0, "{t:?} {mean}");
            let varies = px.iter().any(|p| p != &px[0]);
            assert_eq!(varies, t != Texture::Smooth, "{t:?}");
        }
        assert_eq!(Texture::from_id("pique"), Texture::Pique);
        assert_eq!(Texture::from_id("anything"), Texture::Woven);
    }

    #[test]
    fn light_falls_from_the_top_left() {
        let mut c = Canvas::new(100, 100, [150, 150, 150, 255]).unwrap();
        light(&mut c, 0.2);
        let (tl, br) = (c.pixel(20, 10).unwrap()[0], c.pixel(99, 99).unwrap()[0]);
        assert!(tl > 150 && br < 140, "{tl} {br}");
    }

    #[test]
    fn flatten_lays_the_image_on_the_background() {
        let mut c = Canvas::new(1, 1, [255, 0, 0, 128]).unwrap();
        c.flatten([0, 0, 255]);
        assert_eq!(c.pixel(0, 0), Some([128, 0, 127, 255]));
    }
}
