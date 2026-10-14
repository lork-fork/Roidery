//! Create Strokes: the parts of a colour layer no wider than the Stroke width, traced as stroked
//! centre lines instead of filled outlines.
//!
//! 1. **Distance**: a chamfer (3-4) distance transform gives each pixel its distance to the nearest
//!    pixel outside the layer.
//! 2. **Lines**: an 8-connected component whose widest point (twice its largest distance, less
//!    one) is within the stroke width is a line; wider ones stay filled areas.
//! 3. **Skeleton**: the line is thinned to one pixel (Zhang–Suen, applied pixel by pixel so it
//!    keeps its topology and two-pixel-wide lines don't vanish), then staircase corners that
//!    don't connect anything are dropped.
//! 4. **Graph**: runs of skeleton pixels between ends and junctions (touching junction pixels
//!    merged into one point) become polylines through the pixel centres; rings without ends
//!    become closed ones. Short spurs that thinning leaves at junctions are pruned and the runs
//!    on either side of a junction left with two of them are joined.
//! 5. **Ink**: the skeleton is right to a pixel only, so each of its points is moved to the middle
//!    of the line's ink, read from the image: across the line (along the pixel column or row most
//!    nearly square to it) each pixel is as much ink as its colour is the line's rather than the
//!    paper's, and the ink's centroid is the line's middle there and its sum the line's width (an
//!    anti-aliased or blurred line says both to a fraction of a pixel). The stroke width is the
//!    median of those widths, and each end is put where the ink ends, less the half width a round
//!    cap adds.
//! 6. **Curves**: each polyline is fitted as outlines are ([`fit::fit_piece`]): a straight line
//!    is two anchors, corners stay corners, the rest is as few curves as Paths allows.

use std::collections::HashSet;

use vectorcraft_geom::{PathData, Point};

use crate::Raster;
use crate::fit;
use crate::logo::Style;

/// One line of a colour layer: its centre lines and the width to stroke them with.
pub(crate) struct Line {
    pub path: PathData,
    /// Stroke width in pixels.
    pub width: f64,
    pub pixels: usize,
    /// Its ends are cut square (the ink is as long beside the line's middle as along it), not round.
    pub square: bool,
}

/// The lines of colour `label` in `labels` (`w` × `h`, row-major) no wider than `max_width`
/// pixels, and which pixels they cover (to leave out of the filled areas).
/// `ink` is the colour of the layer's ink in `img` (its palette colour; in Black and White the
/// image's own black), which the lines' middles and widths are read against.
#[allow(clippy::too_many_arguments)]
pub(crate) fn lines(
    labels: &[u16],
    label: u16,
    w: usize,
    h: usize,
    max_width: f64,
    img: &Raster,
    ink: [f64; 3],
    style: &Style,
) -> (Vec<bool>, Vec<Line>) {
    let mut thin = vec![false; labels.len()];
    let mut out = vec![];
    if w == 0 || h == 0 || labels.len() != w * h {
        return (thin, out);
    }
    let mask = |i: usize| labels.get(i) == Some(&label);
    let dt = distance(&mask, w, h);
    let mut seen = vec![false; w * h];
    let mut stack = vec![];
    for start in 0..w * h {
        if !mask(start) || seen[start] {
            continue;
        }
        let mut comp = vec![];
        seen[start] = true;
        stack.push(start);
        while let Some(i) = stack.pop() {
            comp.push(i);
            for j in neighbours(i, w, h) {
                if mask(j) && !seen[j] {
                    seen[j] = true;
                    stack.push(j);
                }
            }
        }
        let widest = 2.0 * f64::from(comp.iter().map(|&i| dt[i]).max().unwrap_or(0)) / 3.0 - 1.0;
        if widest > max_width {
            continue;
        }
        let Some(line) = trace_line(&comp, w, max_width, img, ink, style) else { continue };
        for &i in &comp {
            thin[i] = true;
        }
        out.push(line);
    }
    (thin, out)
}

/// The 8 neighbours of pixel `i` inside the `w` × `h` image.
fn neighbours(i: usize, w: usize, h: usize) -> impl Iterator<Item = usize> {
    let (x, y) = ((i % w) as i64, (i / w) as i64);
    const AROUND: [(i64, i64); 8] = [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)];
    AROUND.into_iter().filter_map(move |(dx, dy)| {
        let (nx, ny) = (x + dx, y + dy);
        (nx >= 0 && ny >= 0 && nx < w as i64 && ny < h as i64).then(|| ny as usize * w + nx as usize)
    })
}

/// Chamfer 3-4 distance (3 per pixel step, saturating) of each pixel in `mask` to the nearest
/// pixel outside it; outside the image counts as outside.
fn distance(mask: &impl Fn(usize) -> bool, w: usize, h: usize) -> Vec<u16> {
    let mut d = vec![0u16; w * h];
    let at = |d: &[u16], x: i64, y: i64| {
        if x < 0 || y < 0 || x >= w as i64 || y >= h as i64 { 0 } else { d.get(y as usize * w + x as usize).copied().unwrap_or(0) }
    };
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if mask(i) {
                let (x, y) = (x as i64, y as i64);
                let steps = [(x - 1, y, 3), (x - 1, y - 1, 4), (x, y - 1, 3), (x + 1, y - 1, 4)];
                d[i] = steps.iter().map(|&(nx, ny, c)| at(&d, nx, ny).saturating_add(c)).min().unwrap_or(0);
            }
        }
    }
    for y in (0..h).rev() {
        for x in (0..w).rev() {
            let i = y * w + x;
            if mask(i) {
                let (xi, yi) = (x as i64, y as i64);
                let steps = [(xi + 1, yi, 3), (xi + 1, yi + 1, 4), (xi, yi + 1, 3), (xi - 1, yi + 1, 4)];
                let v = steps.iter().map(|&(nx, ny, c)| at(&d, nx, ny).saturating_add(c)).min().unwrap_or(0);
                d[i] = d[i].min(v);
            }
        }
    }
    d
}

/// A grid around one line: its pixels with a one-pixel empty border.
struct Grid {
    on: Vec<bool>,
    w: usize,
    h: usize,
    /// Image position of grid pixel (0, 0).
    x0: i64,
    y0: i64,
}

impl Grid {
    /// The 8 neighbours (N, NE, E, SE, S, SW, W, NW) of interior pixel `i`.
    fn ring(&self, i: usize) -> [bool; 8] {
        let w = self.w;
        let g = |j: usize| self.on.get(j).copied().unwrap_or(false);
        [g(i - w), g(i - w + 1), g(i + 1), g(i + w + 1), g(i + w), g(i + w - 1), g(i - 1), g(i - w - 1)]
    }
    fn interior(&self) -> impl Iterator<Item = usize> + use<> {
        let (w, h) = (self.w, self.h);
        (1..h.saturating_sub(1)).flat_map(move |y| (1..w.saturating_sub(1)).map(move |x| y * w + x))
    }
    fn neighbours(&self, i: usize) -> impl Iterator<Item = usize> + '_ {
        let w = self.w;
        [i - w, i - w + 1, i + 1, i + w + 1, i + w, i + w - 1, i - 1, i - w - 1].into_iter().filter(|&j| self.on.get(j).copied().unwrap_or(false))
    }
    fn degree(&self, i: usize) -> usize {
        self.neighbours(i).count()
    }
    /// The centre of pixel `i` in image coordinates.
    fn centre(&self, i: usize) -> Point {
        Point::new((self.x0 + (i % self.w) as i64) as f64 + 0.5, (self.y0 + (i / self.w) as i64) as f64 + 0.5)
    }
}

/// Zhang–Suen thinning. Each sub-iteration picks the pixels to remove from the same state, so the
/// skeleton stays centred; they are then removed one by one, each only if it still doesn't
/// disconnect anything, so a two-pixel-wide line thins to one pixel instead of vanishing.
fn thin(g: &mut Grid) {
    let removable = |p: &[bool; 8]| {
        let b = p.iter().filter(|v| **v).count();
        let a = (0..8).filter(|&k| !p[k] && p[(k + 1) % 8]).count();
        (2..=6).contains(&b) && a == 1
    };
    let mut picked = vec![];
    loop {
        let mut changed = false;
        for step in 0..2 {
            picked.clear();
            for i in g.interior() {
                if !g.on[i] {
                    continue;
                }
                let p = g.ring(i);
                let (n, e, s, w) = (p[0], p[2], p[4], p[6]);
                let side = if step == 0 { !(e && s && (n || w)) } else { !(n && w && (e || s)) };
                if side && removable(&p) {
                    picked.push(i);
                }
            }
            for &i in &picked {
                if removable(&g.ring(i)) {
                    g.on[i] = false;
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    // Staircase corners: a pixel whose neighbours stay connected without it (Yokoi's
    // 8-connectivity number 1) and that doesn't end the line.
    loop {
        let mut changed = false;
        for i in g.interior() {
            if !g.on[i] || g.degree(i) < 2 {
                continue;
            }
            let p = g.ring(i);
            // x1 = E, x2 = NE, x3 = N, x4 = NW, x5 = W, x6 = SW, x7 = S, x8 = SE.
            let x = [p[2], p[1], p[0], p[7], p[6], p[5], p[4], p[3]];
            let off = |k: usize| !x[k % 8];
            let c8: usize = [0, 2, 4, 6].iter().map(|&k| usize::from(off(k)) - usize::from(off(k) && off(k + 1) && off(k + 2))).sum();
            if c8 == 1 {
                g.on[i] = false;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
}

/// A run of skeleton pixels between two nodes (`None`: none, a ring).
#[derive(Clone)]
struct Run {
    pts: Vec<Point>,
    ends: [Option<usize>; 2],
    closed: bool,
}

impl Run {
    fn length(&self) -> f64 {
        self.pts.windows(2).map(|p| p[0].distance(p[1])).sum::<f64>()
            + if self.closed { self.pts.first().zip(self.pts.last()).map_or(0.0, |(a, b)| a.distance(*b)) } else { 0.0 }
    }
}

/// The centre lines of one line component (pixel indices of the `w`-wide image).
fn trace_line(comp: &[usize], w: usize, max_width: f64, img: &Raster, ink: [f64; 3], style: &Style) -> Option<Line> {
    let xs = comp.iter().map(|&i| i % w);
    let ys = comp.iter().map(|&i| i / w);
    let (x0, x1, y0, y1) = (xs.clone().min()?, xs.max()?, ys.clone().min()?, ys.max()?);
    let (gw, gh) = (x1 - x0 + 3, y1 - y0 + 3);
    let mut g = Grid { on: vec![false; gw * gh], w: gw, h: gh, x0: x0 as i64 - 1, y0: y0 as i64 - 1 };
    for &i in comp {
        g.on[(i / w - y0 + 1) * gw + (i % w - x0 + 1)] = true;
    }
    thin(&mut g);
    let skeleton: Vec<usize> = g.interior().filter(|&i| g.on[i]).collect();
    if skeleton.len() < 2 {
        return None;
    }
    // Nodes: line ends, and junctions with the junction pixels touching them.
    let mut node = vec![usize::MAX; g.on.len()];
    let mut centres: Vec<Point> = vec![];
    for &i in &skeleton {
        let d = g.degree(i);
        if d == 2 || node[i] != usize::MAX {
            continue;
        }
        let id = centres.len();
        let mut members = vec![i];
        node[i] = id;
        if d > 2 {
            let mut k = 0;
            while let Some(&p) = members.get(k) {
                k += 1;
                for q in g.neighbours(p).collect::<Vec<_>>() {
                    if node[q] == usize::MAX && g.degree(q) > 2 {
                        node[q] = id;
                        members.push(q);
                    }
                }
            }
        }
        let sum = members.iter().fold(Point::ZERO, |s, &m| s + g.centre(m).to_vec2());
        centres.push(Point::new(sum.x / members.len() as f64, sum.y / members.len() as f64));
    }
    // Runs from each node, then the rings left.
    let mut used: HashSet<(usize, usize)> = HashSet::new();
    let edge = |a: usize, b: usize| (a.min(b), a.max(b));
    let mut runs: Vec<Run> = vec![];
    for &p in skeleton.iter().filter(|&&i| node[i] != usize::MAX) {
        for q in g.neighbours(p).collect::<Vec<_>>() {
            if node[q] == node[p] || !used.insert(edge(p, q)) {
                continue;
            }
            let mut pts = vec![centres[node[p]], g.centre(q)];
            let (mut prev, mut cur) = (p, q);
            while node[cur] == usize::MAX {
                let Some(next) = g.neighbours(cur).find(|&n| n != prev && !used.contains(&edge(cur, n))) else { break };
                used.insert(edge(cur, next));
                pts.push(g.centre(next));
                (prev, cur) = (cur, next);
            }
            let end = (node[cur] != usize::MAX).then_some(node[cur]);
            if let (Some(e), Some(last)) = (end, pts.last_mut()) {
                *last = centres[e];
            }
            runs.push(Run { pts, ends: [Some(node[p]), end], closed: false });
        }
    }
    for &s in &skeleton {
        if node[s] != usize::MAX || g.neighbours(s).any(|n| used.contains(&edge(s, n))) {
            continue;
        }
        let mut pts = vec![g.centre(s)];
        let (mut prev, mut cur) = (usize::MAX, s);
        let mut closed = false;
        while let Some(next) = g.neighbours(cur).find(|&n| n != prev && !used.contains(&edge(cur, n))) {
            used.insert(edge(cur, next));
            if next == s {
                closed = true;
                break;
            }
            pts.push(g.centre(next));
            (prev, cur) = (cur, next);
        }
        runs.push(Run { pts, ends: [None, None], closed });
    }
    // The line's mean width: its pixels over its length (the skeleton stops about half a width
    // short of each end).
    let width = |runs: &[Run]| {
        let length: f64 = runs.iter().map(Run::length).sum();
        let px = comp.len() as f64;
        ((-length + (length * length + 4.0 * px).sqrt()) / 2.0).clamp(1.0, max_width.max(1.0))
    };
    let spur = width(&runs);
    let runs = join(prune(runs, centres.len(), spur), centres.len());
    let mut runs = runs;
    let coarse = width(&runs);
    let ink = Ink { img, color: ink, reach: (max_width / 2.0).ceil().clamp(1.0, 64.0) as i64 + 2 };
    // How many runs end at each node: one is an end of the line.
    let mut meet = vec![0usize; centres.len()];
    for e in runs.iter().flat_map(|r| r.ends.iter().flatten()) {
        meet[*e] += 1;
    }
    let (mut widths, mut soft, mut read) = (vec![], 0usize, 0usize);
    for r in &mut runs {
        let n = r.pts.len();
        let old = r.pts.clone();
        for i in 0..n {
            // A junction stays where its pixels put it.
            let end = if i == 0 {
                r.ends[0]
            } else if i + 1 == n {
                r.ends[1]
            } else {
                None
            };
            if end.is_some_and(|e| meet[e] > 1) {
                continue;
            }
            let (a, b) = if r.closed { (old[(i + n - 2 % n) % n], old[(i + 2) % n]) } else { (old[i.saturating_sub(2)], old[(i + 2).min(n - 1)]) };
            let d = b - a;
            if d.hypot() < 1e-9 {
                continue;
            }
            let upright = d.x.abs() >= d.y.abs();
            let Some(across) = ink.across(old[i], upright) else { continue };
            r.pts[i] = old[i] + if upright { kurbo::Vec2::new(0.0, across.middle) } else { kurbo::Vec2::new(across.middle, 0.0) };
            widths.push(across.width * if upright { d.x.abs() } else { d.y.abs() } / d.hypot());
            read += 1;
            soft += usize::from(across.soft);
        }
    }
    // The stroke is as wide as the ink at the line's middle point (where no ink could be read, as
    // wide as its pixels make it).
    widths.sort_by(f64::total_cmp);
    let width = widths.get(widths.len() / 2).copied().unwrap_or(coarse).clamp(0.25, max_width.max(1.0));
    // Each end goes where the ink ends, less the half width its cap adds. The cap is square when
    // the ends wide enough to tell are, taken together: the ink reaches as far a third of the width to either side
    // of the line's middle as along it, where a round end stops short by a seventh of the width.
    let (mut short_by, mut told) = (0.0, 0);
    for r in runs.iter_mut().filter(|r| !r.closed && r.pts.len() >= 2) {
        for k in 0..2 {
            if r.ends[k].is_some_and(|e| meet[e] > 1) {
                continue;
            }
            let n = r.pts.len();
            let at = |i: usize| if k == 0 { i.min(n - 1) } else { n - 1 - i.min(n - 1) };
            // The last points of a wide line's skeleton turn towards a corner of its end: the end
            // lies where the line was going a width back, and they are put on that line.
            let back = (width.round() as usize + 2).max(3);
            let (tip, inner) = (r.pts[at(0)], r.pts[at(back)]);
            // (Only an end that has left that line: over so few pixels the line's direction is known
            // to a few degrees at best, less without anti-aliasing, and an end already on it is
            // better left where it is.)
            let before = inner - r.pts[at(2 * back)];
            let wide = width >= 3.0 && n > 2 * back && before.cross(tip - inner).abs() > (0.25 * width).max(1.0) * before.hypot();
            let d = if wide { before } else { tip - inner };
            if d.hypot() < 1e-9 {
                continue;
            }
            let along = d / d.hypot();
            let on_line = |p: Point| inner + along * (p - inner).dot(along).max(0.0);
            if wide {
                for i in 0..back {
                    r.pts[at(i)] = on_line(r.pts[at(i)]);
                }
            }
            let tip = on_line(tip);
            let reach = ink.beyond(tip, along);
            if width >= 3.0 {
                let aside = kurbo::Vec2::new(-along.y, along.x) * (0.35 * width);
                let short = reach - (ink.beyond(tip + aside, along) + ink.beyond(tip - aside, along)) / 2.0;
                short_by += short / width;
                told += 1;
            }
            let more = reach - width / 2.0;
            if more.abs() > 0.05 && more > 1.0 - (tip - inner).hypot() {
                r.pts[at(0)] = tip + along * more;
            }
        }
    }
    let mut style = *style;
    if read > 0 && soft * 4 < read {
        // Hard pixels: the middle is only known to about half a pixel (as for outlines).
        style.tol *= 2.5;
        style.line_tol *= 2.5;
        style.sigma = 1.5;
    }
    let subs: Vec<_> = runs
        .iter()
        .filter_map(|r| {
            if r.closed { fit::subpath(&fit::fit_closed(&r.pts, &style)?, true) } else { fit::subpath(&fit::fit_piece(&r.pts, &style, false), false) }
        })
        .collect();
    if subs.is_empty() {
        return None;
    }
    Some(Line { path: PathData::new(subs), width, pixels: comp.len(), square: told > 0 && short_by / f64::from(told) < 0.07 })
}

/// What the image says of a line's ink.
struct Ink<'a> {
    img: &'a Raster,
    /// The line's colour.
    color: [f64; 3],
    /// How far (pixels) from the skeleton the ink is looked for.
    reach: i64,
}

/// The ink across a line at one point: where its middle lies from the point, how wide it is, and
/// whether its edges are soft (some pixel is part ink, part paper).
struct Across {
    middle: f64,
    width: f64,
    soft: bool,
}

impl Ink<'_> {
    /// How far the colour of pixel `(x, y)` is from the line's (`None` off the image).
    fn far(&self, x: i64, y: i64) -> Option<f64> {
        let (w, h) = (i64::from(self.img.width), i64::from(self.img.height));
        if x < 0 || y < 0 || x >= w || y >= h {
            return None;
        }
        let at = (y * w + x) as usize * 4;
        let p = self.img.rgba.get(at..at + 3)?;
        Some((0..3).map(|k| (f64::from(p[k]) - self.color[k]).powi(2)).sum::<f64>().sqrt())
    }
    /// How far the paper's colour is from the line's around pixel `(x, y)`: the farthest colour
    /// within reach, along its row and its column.
    fn paper(&self, x: i64, y: i64) -> f64 {
        (-self.reach..=self.reach).flat_map(|k| [self.far(x + k, y), self.far(x, y + k)]).flatten().fold(0.0, f64::max)
    }
    /// The ink across the line through the pixel `p` lies in: along its column (`upright`) or row.
    fn across(&self, p: Point, upright: bool) -> Option<Across> {
        let (x, y) = (p.x.floor() as i64, p.y.floor() as i64);
        let paper = self.paper(x, y);
        // Paper too like the ink says nothing.
        if paper < 24.0 {
            return None;
        }
        let tone = |k: i64| (if upright { self.far(x, y + k) } else { self.far(x + k, y) }).map_or(0.0, |d| (1.0 - d / paper).clamp(0.0, 1.0));
        // From the pixel with most ink near the skeleton's (thinning leaves it up to a pixel or two
        // off the line's darkest row) out to where the ink ends, or rises again (another line).
        let peak = [0, -1, 1, -2, 2].into_iter().fold(0, |best, k| if tone(k) > tone(best) + 1e-9 { k } else { best });
        let here = tone(peak);
        let (mut sum, mut moment, mut soft) = (here, here * peak as f64, here > 0.1 && here < 0.9);
        for side in [-1, 1] {
            let mut last = here;
            for k in 1..=self.reach {
                let t = tone(peak + side * k);
                if t < 0.04 || t > last + 0.1 {
                    break;
                }
                sum += t;
                moment += t * (peak + side * k) as f64;
                soft |= t > 0.1 && t < 0.9;
                last = t;
            }
        }
        // The pixel's centre is at `p`'s own half-pixel; the middle is measured from `p`.
        let centre = if upright { y as f64 + 0.5 - p.y } else { x as f64 + 0.5 - p.x };
        (sum > 0.0).then(|| Across { middle: centre + moment / sum, width: sum, soft })
    }
    /// How much ink (pixels) lies beyond `tip`, going `dir` (a unit vector).
    fn beyond(&self, tip: Point, dir: kurbo::Vec2) -> f64 {
        let paper = self.paper(tip.x.floor() as i64, tip.y.floor() as i64);
        if paper < 24.0 {
            return 0.0;
        }
        let mut sum = 0.0;
        for step in 1..=4 * (self.reach + 2) {
            let q = tip + dir * (step as f64 * 0.25);
            let t = self.far(q.x.floor() as i64, q.y.floor() as i64).map_or(0.0, |d| (1.0 - d / paper).clamp(0.0, 1.0));
            if t < 0.04 {
                break;
            }
            sum += 0.25 * t;
        }
        sum
    }
}

/// The runs without the spurs thinning leaves: runs shorter than the line's `width` from a line end
/// to a junction that keeps at least two other runs.
fn prune(runs: Vec<Run>, nodes: usize, width: f64) -> Vec<Run> {
    let mut degree = vec![0usize; nodes];
    for e in runs.iter().flat_map(|r| r.ends.iter().flatten()) {
        degree[*e] += 1;
    }
    let mut order: Vec<usize> = (0..runs.len()).collect();
    order.sort_by(|a, b| runs[*a].length().total_cmp(&runs[*b].length()));
    let mut keep = vec![true; runs.len()];
    for i in order {
        let r = &runs[i];
        let [Some(a), Some(b)] = r.ends else { continue };
        let (tip, fork) = if degree[a] == 1 { (a, b) } else { (b, a) };
        if degree[tip] == 1 && degree[fork] >= 3 && r.length() < width.max(2.0) {
            keep[i] = false;
            degree[tip] -= 1;
            degree[fork] -= 1;
        }
    }
    runs.into_iter().zip(keep).filter_map(|(r, k)| k.then_some(r)).collect()
}

/// The runs joined through every node exactly two of them meet at (a run meeting itself closes).
fn join(runs: Vec<Run>, nodes: usize) -> Vec<Run> {
    let mut runs: Vec<Option<Run>> = runs.into_iter().map(Some).collect();
    for n in 0..nodes {
        let at: Vec<(usize, usize)> = runs
            .iter()
            .enumerate()
            .flat_map(|(i, r)| r.iter().flat_map(move |r| (0..2).filter(move |&k| r.ends[k] == Some(n)).map(move |k| (i, k))))
            .collect();
        let [(i, ki), (j, kj)] = at[..] else { continue };
        if i == j {
            if let Some(r) = runs[i].as_mut() {
                r.pts.pop();
                r.ends = [None, None];
                r.closed = r.pts.len() >= 3;
            }
            continue;
        }
        let (Some(mut a), Some(mut b)) = (runs[i].take(), runs[j].take()) else { continue };
        // a ends at n, b starts at n.
        if ki == 0 {
            a.pts.reverse();
            a.ends.reverse();
        }
        if kj == 1 {
            b.pts.reverse();
            b.ends.reverse();
        }
        a.pts.extend(b.pts.into_iter().skip(1));
        a.ends[1] = b.ends[1];
        runs[i] = Some(a);
    }
    runs.into_iter().flatten().collect()
}
