//! How a thread is lit, on the CPU. The realistic view's GPU shader (`ui-egui`'s `thread_gpu.rs`)
//! does the same sums in WGSL: keep the two in step.
//!
//! - The thread's colour becomes a surface that reflects 4 % to 94 % of the light, never 0 or
//!   100 %, so the light and shadow show on black and white thread too.
//! - Two plies twist round the thread as a helix; each is lit as its own small cylinder, with a
//!   darker groove between them and, close up, fine streaks of fibre along it.
//! - The sheen runs along the fibres, as hair is shaded (Kajiya–Kay): a white lobe from light off
//!   the surface, which shows on black, and a softer one tinted by the thread.
//! - Grooves, the thread's sides and its ends, where it dives into the fabric, are darker; that is
//!   what gives white its shape.
//! - A soft shoulder instead of clipping keeps the highlight visible on white.
//! - Each detail fades out as it gets smaller than a pixel, and far out the thread is its average
//!   colour, so a colour looks the same at every zoom.

/// The plies' angle to the thread's axis, radians (40°).
pub const TWIST: f64 = 0.698_131_7;
const PLIES: f64 = 2.0;
const PI: f64 = std::f64::consts::PI;

/// Gloss for a polyester or rayon thread (the default); cotton is about 0.35, metallic 1.6.
pub const GLOSS: f64 = 1.0;

/// The light, from the top left of the screen: normalised (-0.45, -0.65, 0.62).
pub const LIGHT: [f64; 3] = [-0.447_899_8, -0.646_966_4, 0.617_106_4];

/// One point on a thread, in the thread's own frame.
#[derive(Clone, Copy, Debug)]
pub struct Point {
    /// Along the stitch from its start, pixels.
    pub along: f64,
    /// The stitch's length, pixels.
    pub len: f64,
    /// Across, as a fraction of the half width: `q[0]` past an end, `q[1]` from the centre line.
    pub q: [f64; 2],
    /// The stitch's direction on screen (unit).
    pub dir: [f64; 2],
    /// Half the thread's width, pixels.
    pub hw: f64,
    /// A stable random number in 0..1 per stitch (the twist's phase).
    pub seed: f64,
}

pub fn smoothstep(e0: f64, e1: f64, x: f64) -> f64 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The thread's colour (0..1) as a surface: 0.04 to 0.94, with mid tones kept where they were.
pub fn base(rgb: [f64; 3]) -> [f64; 3] {
    rgb.map(|v| 0.04 + v.clamp(0.0, 1.0).powf(1.25) * 0.90)
}

/// Linear up to 0.8, then a soft roll-off towards 1 instead of clipping.
pub fn tone(x: f64) -> f64 {
    const K: f64 = 0.8;
    if x < K { x.max(0.0) } else { K + (1.0 - K) * (1.0 - (-(x - K) / (1.0 - K)).exp()) }
}

/// How strongly the round shape (and the cast shadow) shows at this half width in pixels.
pub fn form(hw: f64) -> f64 {
    smoothstep(0.5, 1.5, hw)
}

/// The thread's colour far out, where it is too small to shade.
pub fn flat(rgb: [f64; 3]) -> [f64; 3] {
    base(rgb).map(|v| tone(v * 0.92))
}

fn hash1(x: f64) -> f64 {
    ((x * 127.1 + 311.7).sin() * 43_758.545_3).rem_euclid(1.0)
}

fn vnoise(x: f64) -> f64 {
    let i = x.floor();
    let f = x - i;
    let u = f * f * (3.0 - 2.0 * f);
    hash1(i) * (1.0 - u) + hash1(i + 1.0) * u
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn norm(v: [f64; 3]) -> [f64; 3] {
    let l = dot(v, v).sqrt().max(1e-12);
    v.map(|x| x / l)
}

fn add(a: [f64; 3], b: [f64; 3], k: f64) -> [f64; 3] {
    [a[0] + b[0] * k, a[1] + b[1] * k, a[2] + b[2] * k]
}

/// Kajiya–Kay: a highlight along the fibre tangent `t`, shifted along the normal `n`.
fn kk(t: [f64; 3], n: [f64; 3], h: [f64; 3], shift: f64, e: f64) -> f64 {
    let th = dot(norm(add(t, n, shift)), h);
    (1.0 - th * th).max(0.0).sqrt().powf(e)
}

/// The colour (0..1) of thread `rgb` with `gloss` at point `p`.
pub fn thread(rgb: [f64; 3], gloss: f64, p: &Point) -> [f64; 3] {
    let hw = p.hw.max(1e-3);
    let q = p.q;
    let form = form(hw);
    let ply_d = smoothstep(2.5, 6.0, hw);
    let fibre_d = smoothstep(4.0, 9.0, hw);
    let sharp = 0.25 + 0.75 * smoothstep(1.5, 6.0, hw);
    let base = base(rgb);

    let along = [p.dir[0], p.dir[1], 0.0];
    let across = [-p.dir[1], p.dir[0], 0.0];
    let nz0 = (1.0 - q[0] * q[0] - q[1] * q[1]).max(0.0).sqrt();
    let n0 = [along[0] * q[0] + across[0] * q[1], along[1] * q[0] + across[1] * q[1], nz0];
    let around = [across[0] * nz0, across[1] * nz0, -q[1]];

    // The plies: lines of equal phase follow the fibres, which lean at TWIST and bend round
    // the thread's edges.
    let phi = q[1].clamp(-1.0, 1.0).asin();
    let t = (p.along * TWIST.tan() / hw - phi) * PLIES / (2.0 * PI) + p.seed;
    let pn = t.rem_euclid(1.0) * 2.0 - 1.0;
    let ply_h = (1.0 - pn * pn).max(0.0).sqrt();
    let (ct, st) = (TWIST.cos(), TWIST.sin());
    let fibre_t = norm([along[0] * ct + around[0] * st, along[1] * ct + around[1] * st, around[2] * st]);
    let ply_x = [around[0] * ct - along[0] * st, around[1] * ct - along[1] * st, around[2] * ct];
    let mut nrm = add(n0, ply_x, pn * 0.45 * ply_d);
    nrm[2] += 1e-4;
    let nrm = norm(nrm);

    let fibre = 1.0 + (vnoise(pn * 5.0 + t.floor() * 17.0 + p.seed * 31.0) - 0.5) * 0.22 * fibre_d;

    let wrap = ((dot(nrm, LIGHT) + 0.35) / 1.35).clamp(0.0, 1.0);
    let groove = 1.0 + (0.68 + 0.32 * smoothstep(0.0, 0.8, ply_h) - 1.0) * ply_d;
    let rim = 0.62 + 0.38 * nz0;
    let tuck = 0.72 + 0.28 * smoothstep(-0.8, 0.5, p.along.min(p.len - p.along) / hw);
    let light = (0.42 + 0.78 * wrap) * groove * rim * tuck * fibre;

    let h = norm([LIGHT[0], LIGHT[1], LIGHT[2] + 1.0]);
    let vis = groove * tuck * (0.6 + 0.4 * fibre) * gloss * (0.5 + 0.5 * sharp);
    let white = kk(fibre_t, nrm, h, 0.12, 90.0 * sharp) * 0.34 * vis;
    let tint = kk(fibre_t, nrm, h, -0.10, 24.0 * sharp) * 0.25 * vis;

    base.map(|b| {
        let lit = b * light + white + b * tint;
        tone(b * 0.92 + (lit - b * 0.92) * form)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn across(rgb: [f64; 3], hw: f64) -> Vec<f64> {
        // Luminance across the middle of a long horizontal stitch, edge to edge.
        (0..=40)
            .map(|i| {
                let y = -0.95 + 1.9 * f64::from(i) / 40.0;
                let p = Point { along: 50.0 * hw, len: 100.0 * hw, q: [0.0, y], dir: [1.0, 0.0], hw, seed: 0.3 };
                let c = thread(rgb, GLOSS, &p);
                0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
            })
            .collect()
    }

    fn range(v: &[f64]) -> f64 {
        v.iter().copied().fold(f64::MIN, f64::max) - v.iter().copied().fold(f64::MAX, f64::min)
    }

    #[test]
    fn light_and_shade_show_on_white_and_black() {
        for rgb in [[1.0; 3], [0.0; 3], [0.1, 0.27, 0.78]] {
            let v = across(rgb, 8.0);
            assert!(range(&v) > 0.25, "{rgb:?}: {v:?}");
            assert!(v.iter().all(|x| (0.0..=1.0).contains(x)));
        }
        // White keeps a highlight below pure white rather than clipping a whole band.
        let white = across([1.0; 3], 8.0);
        assert!(white.iter().filter(|&&x| x > 0.995).count() < 4, "{white:?}");
    }

    #[test]
    fn a_colour_looks_the_same_at_every_zoom() {
        let mean = |hw: f64| {
            let mut s = [0.0; 3];
            let mut n = 0.0;
            for i in 0..60 {
                for j in 0..30 {
                    let p = Point {
                        along: hw * (2.0 + f64::from(i) * 0.6),
                        len: hw * 40.0,
                        q: [0.0, -0.98 + 1.96 * f64::from(j) / 29.0],
                        dir: [1.0, 0.0],
                        hw,
                        seed: 0.6,
                    };
                    let c = thread([0.5, 0.3, 0.7], GLOSS, &p);
                    for k in 0..3 {
                        s[k] += c[k];
                    }
                    n += 1.0;
                }
            }
            s.map(|v| v / n)
        };
        let (near, far) = (mean(10.0), mean(0.2));
        for k in 0..3 {
            assert!((near[k] - far[k]).abs() < 0.08, "{near:?} vs {far:?}");
        }
    }

    #[test]
    fn mid_tones_stay_put_and_the_ends_are_in_range() {
        assert!((base([0.5; 3])[0] - 0.418).abs() < 0.01);
        assert_eq!(tone(0.5), 0.5);
        assert!(tone(5.0) < 1.0 && tone(0.9) > 0.85);
        assert_eq!(base([0.0; 3]), [0.04; 3]);
    }
}
