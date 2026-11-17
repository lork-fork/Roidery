//! Longarm quilting (Q-series): continuous-line designs for computerized longarm machines, in
//! millimetres, y down. A longarm sews one unbroken line (it can't trim and jump as an
//! embroidery machine does), so everything here makes and keeps lines continuous:
//!
//! * [`join`] (Q1): pieces chained into one line, nearest end to nearest end, with a report of
//!   the joins it had to make;
//! * [`to_edges`] (Q2): a line led in from the left edge of its box and out to the right edge at
//!   the same height, so copies set side by side run on from one to the next;
//! * [`pantograph`] (Q3): an edge-to-edge design, the line repeated across a quilt in rows that
//!   nest (every other row half a repeat along);
//! * [`border`] and [`corner`] (Q4): the line fitted a whole number of times along a strip, and
//!   two strips meeting at a corner;
//! * [`meander`], [`echo`] (Q5): continuous fills of a closed shape;
//! * [`plt`], [`dxf`], [`svg`] (Q6, Q7): single-stroke files for quilting software.

use crate::geom::{BBox, Point, Polygon, poly, polyline};

/// Most points a quilting line may have (larger ones are refused, not built).
pub const MAX_POINTS: usize = 2_000_000;
/// Most pieces [`join`] chains.
pub const MAX_PIECES: usize = 5_000;

/// A line's pieces, joined into one.
#[derive(Clone, Debug, PartialEq)]
pub struct Joined {
    pub line: Vec<Point>,
    /// How many joins were made between pieces, and the longest (mm).
    pub joins: usize,
    pub longest_join: f64,
}

/// Q1: `pieces` (open lines; a closed one is a ring whose first and last points meet) chained
/// into one continuous line: from the piece with the leftmost end, each time the nearest end of
/// a piece not used yet (a ring entered at its nearest point and sewn all the way round). Gaps
/// under `tolerance` mm aren't counted as joins. `None` with no piece of two points or more.
pub fn join(pieces: &[Vec<Point>], tolerance: f64) -> Option<Joined> {
    let mut left: Vec<Vec<Point>> = pieces.iter().take(MAX_PIECES).filter(|p| p.len() >= 2).cloned().collect();
    if left.is_empty() {
        return None;
    }
    let closed = |p: &[Point]| p.len() > 2 && p.first().zip(p.last()).is_some_and(|(a, b)| a.distance(*b) < 1e-9);
    // the start: the leftmost end of any piece
    let (mut i0, mut rev0) = (0usize, false);
    let mut best = f64::INFINITY;
    for (i, p) in left.iter().enumerate() {
        for (rev, q) in [(false, p.first()), (true, p.last())] {
            if let Some(q) = q
                && q.x < best
            {
                best = q.x;
                (i0, rev0) = (i, rev);
            }
        }
    }
    let mut first = left.swap_remove(i0);
    if rev0 {
        first.reverse();
    }
    let mut line = first;
    let (mut joins, mut longest) = (0usize, 0.0f64);
    while !left.is_empty() {
        let Some(&here) = line.last() else { break };
        // the nearest end, or a ring's nearest point
        let mut pick: Option<(usize, usize, bool, f64)> = None;
        for (i, p) in left.iter().enumerate() {
            if closed(p) {
                for (k, q) in p.iter().enumerate() {
                    let d = q.distance(here);
                    if pick.is_none_or(|b| d < b.3) {
                        pick = Some((i, k, false, d));
                    }
                }
            } else {
                for (rev, q) in [(false, p.first()), (true, p.last())] {
                    let Some(q) = q else { continue };
                    let d = q.distance(here);
                    if pick.is_none_or(|b| d < b.3) {
                        pick = Some((i, 0, rev, d));
                    }
                }
            }
        }
        let Some((i, k, rev, d)) = pick else { break };
        let mut next = left.swap_remove(i);
        if closed(&next) {
            // round from point k back to it
            next.pop();
            let k = k.min(next.len().saturating_sub(1));
            next.rotate_left(k);
            if let Some(&start) = next.first() {
                next.push(start);
            }
        } else if rev {
            next.reverse();
        }
        if d > tolerance {
            joins += 1;
            longest = longest.max(d);
        } else if next.first().is_some_and(|a| a.distance(here) < 1e-9) {
            next.remove(0);
        }
        line.extend(next);
        if line.len() > MAX_POINTS {
            line.truncate(MAX_POINTS);
            break;
        }
    }
    Some(Joined { line, joins, longest_join: longest })
}

/// Where a line enters and leaves its box: [`to_edges`]'s height.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EdgeHeight {
    /// The height the line starts at.
    Start,
    /// Halfway down its box.
    Middle,
}

/// Q2: `line` led in from the left edge of its box and out to the right edge, both at one
/// height, so a copy set its width along starts where this one ends. The line is first turned
/// round when its end is further left than its start.
pub fn to_edges(line: &[Point], height: EdgeHeight) -> Vec<Point> {
    let (Some(&a), Some(&b)) = (line.first(), line.last()) else { return Vec::new() };
    let mut pts: Vec<Point> = line.to_vec();
    if b.x < a.x {
        pts.reverse();
    }
    let bb = polyline::bbox(&pts);
    let (Some(&start), Some(&end)) = (pts.first(), pts.last()) else { return Vec::new() };
    let y = match height {
        EdgeHeight::Start => start.y,
        EdgeHeight::Middle => (bb.min.y + bb.max.y) * 0.5,
    };
    let mut out = Vec::with_capacity(pts.len() + 4);
    let lead_in = Point::new(bb.min.x, y);
    if lead_in.distance(start) > 1e-9 {
        out.push(lead_in);
    }
    out.extend(pts);
    let lead_out = Point::new(bb.max.x, y);
    if lead_out.distance(end) > 1e-9 {
        out.push(lead_out);
    }
    out
}

fn translate(pts: &[Point], dx: f64, dy: f64) -> impl Iterator<Item = Point> + '_ {
    pts.iter().map(move |p| Point::new(p.x + dx, p.y + dy))
}

/// Q3: an edge-to-edge (pantograph) design over a `width` × `height` mm quilt: `motif` (a line
/// run edge to edge, [`to_edges`]) repeated across in rows `row` mm apart, each row one
/// continuous line from the left edge to past the right; with `nest`, every other row starts
/// half a repeat along, so the rows' shapes fit into one another. The rows, top first.
pub fn pantograph(motif: &[Point], width: f64, height: f64, row: f64, nest: bool) -> Vec<Vec<Point>> {
    let bb = polyline::bbox(motif);
    let mw = bb.width();
    if motif.len() < 2 || !(mw > 0.5 && width.is_finite() && height.is_finite() && row.is_finite() && row > 0.5) {
        return Vec::new();
    }
    let (width, height) = (width.clamp(1.0, 20_000.0), height.clamp(1.0, 20_000.0));
    let rows = ((height / row).ceil() as usize).clamp(1, 2_000);
    let per_row = ((width / mw).ceil() as usize + 2).min(10_000);
    if rows.saturating_mul(per_row).saturating_mul(motif.len()) > MAX_POINTS {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(rows);
    for r in 0..rows {
        let shift = if nest && r % 2 == 1 { -mw * 0.5 } else { 0.0 };
        let dy = r as f64 * row - bb.min.y;
        let mut line: Vec<Point> = Vec::with_capacity(per_row * motif.len());
        for k in 0..per_row {
            let dx = shift + k as f64 * mw - bb.min.x;
            if dx + bb.min.x > width {
                break;
            }
            for q in translate(motif, dx, dy) {
                // copies meet end to start: one point where they do
                if line.last().is_none_or(|l: &Point| l.distance(q) > 1e-9) {
                    line.push(q);
                }
            }
        }
        out.push(line);
    }
    out
}

/// `motif` scaled to `w` × `h` mm, its box's top left at (x, y).
fn fit(motif: &[Point], x: f64, y: f64, w: f64, h: f64) -> Vec<Point> {
    let bb = polyline::bbox(motif);
    let (sx, sy) = (if bb.width() > 1e-9 { w / bb.width() } else { 1.0 }, if bb.height() > 1e-9 { h / bb.height() } else { 1.0 });
    motif.iter().map(|p| Point::new(x + (p.x - bb.min.x) * sx, y + (p.y - bb.min.y) * sy)).collect()
}

/// Q4: a border `length` × `depth` mm: `motif` (edge to edge) fitted a whole number of times
/// along it (as close to its own proportions as that allows), one continuous line from the left.
pub fn border(motif: &[Point], length: f64, depth: f64) -> Vec<Point> {
    let bb = polyline::bbox(motif);
    if motif.len() < 2 || bb.height() <= 1e-9 || !(length.is_finite() && depth.is_finite() && length > 1.0 && depth > 1.0) {
        return Vec::new();
    }
    let natural = bb.width() * depth / bb.height();
    let n = ((length / natural.max(1e-6)).round() as usize).clamp(1, 10_000);
    if n.saturating_mul(motif.len()) > MAX_POINTS {
        return Vec::new();
    }
    let w = length / n as f64;
    let mut line: Vec<Point> = Vec::with_capacity(n * motif.len());
    for k in 0..n {
        for q in fit(motif, k as f64 * w, 0.0, w, depth) {
            if line.last().is_none_or(|l: &Point| l.distance(q) > 1e-9) {
                line.push(q);
            }
        }
    }
    line
}

/// Q4: two borders meeting at a corner: one `along` mm across the top, then one `down` mm down
/// the right side (the first turned a quarter clockwise), `depth` deep, one continuous line.
pub fn corner(motif: &[Point], along: f64, down: f64, depth: f64) -> Vec<Point> {
    let top = border(motif, along, depth);
    let side = border(motif, down, depth);
    if top.is_empty() || side.is_empty() {
        return top;
    }
    // the side strip turned a quarter clockwise: (x, y) → (along - y, x), its top at the corner
    let mut line = top;
    let turned = side.iter().map(|p| Point::new(along - p.y, p.x));
    for q in turned {
        if line.last().is_none_or(|l: &Point| l.distance(q) > 1e-9) {
            line.push(q);
        }
    }
    line
}

/// Q5: a continuous meander inside `shape` (stipple with `random` > 0, a regular meander with
/// 0), its paths about `spacing` mm apart. One line where the shape is one piece.
pub fn meander(shape: &Polygon, spacing: f64, random: f64) -> Vec<Point> {
    if !(spacing.is_finite() && random.is_finite()) {
        return Vec::new();
    }
    crate::engine::meander_line(shape, spacing.clamp(1.0, 100.0), random.clamp(0.0, 1.0))
}

/// Q5: echo quilting inside `shape`: rings `spacing` mm apart, from half a spacing in to the
/// middle, each joined to the next where they are closest, one continuous line from the outside
/// in.
pub fn echo(shape: &Polygon, spacing: f64) -> Vec<Point> {
    if !(spacing.is_finite() && spacing >= 1.0) {
        return Vec::new();
    }
    let mut rings: Vec<Vec<Point>> = Vec::new();
    for k in 0..500 {
        let inset = poly::offset(std::slice::from_ref(shape), -(spacing * (k as f64 + 0.5)));
        let Some(biggest) = inset.into_iter().max_by(|a, b| a.area().abs().total_cmp(&b.area().abs())) else { break };
        if biggest.area().abs() < spacing * spacing * 0.25 {
            break;
        }
        rings.push(polyline::closed(&biggest.outer));
    }
    let pieces: Vec<Vec<Point>> = rings;
    join(&pieces, 1e-6).map(|j| j.line).unwrap_or_default()
}

/// The box of all `lines`.
pub fn bounds(lines: &[Vec<Point>]) -> BBox {
    let pts: Vec<Point> = lines.iter().flatten().copied().collect();
    polyline::bbox(&pts)
}

/// Q6: an HP-GL (PLT) file of `lines` (mm), 40 plotter units a millimetre, y up, the box's lower
/// left at the origin: pen up to each line's start, pen down along it.
pub fn plt(lines: &[Vec<Point>]) -> String {
    let bb = bounds(lines);
    let unit = |p: &Point| ((p.x - bb.min.x) * 40.0).round() as i64;
    let unit_y = |p: &Point| ((bb.max.y - p.y) * 40.0).round() as i64;
    let mut out = String::from("IN;SP1;\n");
    for line in lines.iter().filter(|l| l.len() >= 2) {
        let Some(first) = line.first() else { continue };
        out.push_str(&format!("PU{},{};\n", unit(first), unit_y(first)));
        let rest: Vec<String> = line.iter().skip(1).map(|p| format!("{},{}", unit(p), unit_y(p))).collect();
        for chunk in rest.chunks(64) {
            out.push_str(&format!("PD{};\n", chunk.join(",")));
        }
    }
    out.push_str("PU;SP0;\n");
    out
}

/// Q7: a single-stroke DXF (R12, ASCII) of `lines` (mm, y up as DXF has it): one POLYLINE each,
/// on layer QUILT, units millimetres.
pub fn dxf(lines: &[Vec<Point>]) -> String {
    let bb = bounds(lines);
    let mut out = String::new();
    let mut put = |code: i32, value: &str| {
        out.push_str(&format!("{code}\n{value}\n"));
    };
    put(0, "SECTION");
    put(2, "HEADER");
    put(9, "$INSUNITS");
    put(70, "4");
    put(0, "ENDSEC");
    put(0, "SECTION");
    put(2, "ENTITIES");
    for line in lines.iter().filter(|l| l.len() >= 2) {
        put(0, "POLYLINE");
        put(8, "QUILT");
        put(66, "1");
        put(70, "0");
        for p in line {
            put(0, "VERTEX");
            put(8, "QUILT");
            put(10, &format!("{:.4}", p.x - bb.min.x));
            put(20, &format!("{:.4}", bb.max.y - p.y));
        }
        put(0, "SEQEND");
    }
    put(0, "ENDSEC");
    put(0, "EOF");
    out
}

/// Q7: a single-stroke SVG of `lines` (mm): one unfilled polyline each, the page the lines' box.
pub fn svg(lines: &[Vec<Point>]) -> String {
    let bb = bounds(lines);
    let (w, h) = (bb.width().max(0.1), bb.height().max(0.1));
    let mut out = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w:.3}mm\" height=\"{h:.3}mm\" viewBox=\"0 0 {w:.3} {h:.3}\">\n"
    );
    for line in lines.iter().filter(|l| l.len() >= 2) {
        let pts: Vec<String> = line.iter().map(|p| format!("{:.3},{:.3}", p.x - bb.min.x, p.y - bb.min.y)).collect();
        out.push_str(&format!("<polyline fill=\"none\" stroke=\"#000\" stroke-width=\"0.3\" points=\"{}\"/>\n", pts.join(" ")));
    }
    out.push_str("</svg>\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f64, y: f64) -> Point {
        Point::new(x, y)
    }

    #[test]
    fn pieces_join_nearest_end_to_nearest_end() {
        // three dashes, the middle one backwards, and a square ring
        let pieces = vec![
            vec![p(20.0, 0.0), p(30.0, 0.0)],
            vec![p(19.0, 0.0), p(10.0, 0.0)],
            vec![p(0.0, 0.0), p(9.5, 0.0)],
            vec![p(40.0, -5.0), p(50.0, -5.0), p(50.0, 5.0), p(40.0, 5.0), p(40.0, -5.0)],
        ];
        let j = join(&pieces, 0.6).unwrap();
        // starts at the leftmost end and runs left to right
        assert_eq!(j.line.first(), Some(&p(0.0, 0.0)));
        let xs: Vec<f64> = j.line.iter().take(6).map(|q| q.x).collect();
        assert_eq!(xs, vec![0.0, 9.5, 10.0, 19.0, 20.0, 30.0]);
        // joins: 9.5 → 10 is under the tolerance; 19 → 20 and 30 → the ring are joins
        assert_eq!(j.joins, 2);
        assert!((j.longest_join - p(30.0, 0.0).distance(p(40.0, -5.0))).abs() < 1e-9);
        // the ring is sewn all the way round, back to where it was entered
        assert_eq!(j.line.len(), 6 + 5);
        assert_eq!(j.line.last(), Some(&p(40.0, -5.0)));
        assert!(join(&[vec![p(1.0, 1.0)]], 0.5).is_none());
    }

    #[test]
    fn a_line_runs_edge_to_edge_and_copies_run_on() {
        // a hump from (5, 10) up to (15, 0) and down to (25, 8), drawn right to left
        let hump = vec![p(25.0, 8.0), p(15.0, 0.0), p(5.0, 10.0)];
        let e = to_edges(&hump, EdgeHeight::Start);
        assert_eq!(e.first(), Some(&p(5.0, 10.0)), "turned to run left to right; starts on the left edge");
        assert_eq!(e.last(), Some(&p(25.0, 10.0)), "ends on the right edge at the start's height");
        // a pantograph of it: rows of copies, each row one line, every other row half along
        let rows = pantograph(&e, 100.0, 45.0, 15.0, true);
        assert_eq!(rows.len(), 3);
        let first = rows.first().unwrap();
        // the hump's longest segment is 14.2 mm; no join between copies is longer
        assert!(first.windows(2).all(|w| w[0].distance(w[1]) <= 14.2), "no jumps within a row");
        assert!(first.last().unwrap().x >= 100.0 - 20.0);
        assert_eq!(rows[1].first().unwrap().x, -10.0, "the second row half a repeat (10 mm) along");
        assert!((rows[1].first().unwrap().y - rows[0].first().unwrap().y - 15.0).abs() < 1e-9);
    }

    #[test]
    fn borders_fit_whole_repeats_and_corners_turn() {
        let wave = vec![p(0.0, 5.0), p(5.0, 0.0), p(10.0, 5.0), p(15.0, 10.0), p(20.0, 5.0)];
        // 20 × 10 motif at 10 deep: 20 mm each; 105 mm takes 5 of 21 mm
        let b = border(&wave, 105.0, 10.0);
        let bb = polyline::bbox(&b);
        assert!((bb.width() - 105.0).abs() < 1e-9 && (bb.height() - 10.0).abs() < 1e-9);
        assert_eq!(b.len(), 5 * 4 + 1);
        let c = corner(&wave, 105.0, 60.0, 10.0);
        let cb = polyline::bbox(&c);
        assert!((cb.max.x - 105.0).abs() < 1e-9 && (cb.max.y - 60.0).abs() < 1e-9, "{cb:?}");
        assert!(c.windows(2).all(|w| w[0].distance(w[1]) < 15.0), "one line, round the corner");
    }

    #[test]
    fn fills_are_one_line_inside_the_shape() {
        let sq = Polygon::from_ring(vec![p(0.0, 0.0), p(60.0, 0.0), p(60.0, 40.0), p(0.0, 40.0)]);
        for (k, line) in [meander(&sq, 5.0, 0.6), meander(&sq, 5.0, 0.0), echo(&sq, 5.0)].into_iter().enumerate() {
            assert!(line.len() >= 16, "{}", line.len());
            assert!(line.iter().all(|q| q.x > -0.5 && q.x < 60.5 && q.y > -0.5 && q.y < 40.5));
            // meanders take short steps; echo rings run straight along the sides, and their
            // joins from ring to ring are short
            let limit = if k < 2 { 3.0 } else { 56.0 };
            assert!(line.windows(2).all(|w| w[0].distance(w[1]) < limit), "fill {k}: a long jump");
        }
        assert!(meander(&sq, f64::NAN, 0.0).is_empty() && echo(&sq, 0.0).is_empty());
    }

    #[test]
    fn files_hold_every_line() {
        let lines = vec![vec![p(0.0, 0.0), p(10.0, 0.0), p(10.0, 5.0)], vec![p(20.0, 20.0), p(25.0, 25.0)]];
        let h = plt(&lines);
        assert!(h.starts_with("IN;SP1;") && h.trim_end().ends_with("PU;SP0;"));
        // y up from the box's bottom: (0, 0) is 25 mm above it
        assert!(h.contains("PU0,1000;") && h.contains("PD400,1000,400,800;"), "{h}");
        let d = dxf(&lines);
        assert_eq!(d.matches("\nPOLYLINE\n").count(), 2);
        assert_eq!(d.matches("\nVERTEX\n").count(), 5);
        assert!(d.trim_end().ends_with("EOF"));
        let s = svg(&lines);
        assert_eq!(s.matches("<polyline").count(), 2);
        assert!(s.contains("width=\"25.000mm\""));
    }
}
