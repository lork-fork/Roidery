//! Redwork: line art sewn as one line that starts and ends at the same point. Every line is sewn
//! once in the run style; the way on to the next line retraces lines already sewn as a single run
//! on top, so no travel shows off the lines. The route: the lines' junctions (crossings, ends,
//! ends meeting other lines) and the pieces of line between them form a graph; junctions where
//! an odd number of pieces meet are paired along the shortest ways, which are sewn twice, and an
//! Euler tour passes every piece. Parts that don't touch are joined by travel or a trim.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};

use super::out::Out;
use super::run::{self, CORNER_ANGLE};
use crate::geom::{Contour, FLATTEN_TOLERANCE, Point, polyline};
use crate::model::RunParams;
use crate::pattern::{Cmd, Stitch};

/// Lines whose ends come this close (mm) to another line meet it.
const SNAP: f64 = 0.3;
/// Most line segments routed (more: the lines are simplified first).
const MAX_SEGMENTS: usize = 20_000;
/// Most pairs of segments looked at for crossings.
const MAX_PAIRS: usize = 4_000_000;

/// A piece of line between two junctions or ends.
struct Edge {
    a: usize,
    b: usize,
    pts: Vec<Point>,
    len: f64,
}

/// The stitches of redwork made of `lines`, starting and ending at the junction nearest `hint`.
pub(crate) fn generate(lines: &[Contour], params: &RunParams, hint: Option<Point>, trim: f64) -> Vec<Stitch> {
    let polys = polylines(lines);
    let (nodes, edges) = graph(&polys);
    if edges.is_empty() {
        return Vec::new();
    }
    let tours = route(&nodes, &edges, hint);
    sew(&tours, &edges, params, trim)
}

/// The lines as polylines (closed ones back to their start), finite points only, simplified until
/// there are at most [`MAX_SEGMENTS`] segments.
fn polylines(lines: &[Contour]) -> Vec<Vec<Point>> {
    let mut polys: Vec<Vec<Point>> = lines
        .iter()
        .take(MAX_SEGMENTS)
        .map(|c| {
            let pts: Vec<Point> = c.polyline(FLATTEN_TOLERANCE).into_iter().filter(|p| p.x.is_finite() && p.y.is_finite()).collect();
            let pts = if c.closed && pts.len() > 2 { polyline::closed(&pts) } else { pts };
            polyline::dedup(&pts, 1e-6)
        })
        .filter(|p| p.len() >= 2 && polyline::length(p) > 1e-6)
        .collect();
    let mut tol = 0.05;
    while polys.iter().map(|p| p.len().saturating_sub(1)).sum::<usize>() > MAX_SEGMENTS && tol < 1000.0 {
        for p in &mut polys {
            *p = polyline::simplify(p, tol);
        }
        tol *= 2.0;
    }
    polys
}

/// A segment of a line, for finding where lines meet.
struct Seg {
    line: usize,
    k: usize,
    a: Point,
    b: Point,
    lo: Point,
    hi: Point,
    /// The line's start (first segment) and end (last segment) on this segment.
    ends: [Option<Point>; 2],
}

/// Junctions (merged within [`SNAP`]) and the pieces of line between them.
fn graph(polys: &[Vec<Point>]) -> (Vec<Point>, Vec<Edge>) {
    let mut segs: Vec<Seg> = Vec::new();
    let mut first_seg = Vec::with_capacity(polys.len());
    for (line, p) in polys.iter().enumerate() {
        first_seg.push(segs.len());
        let last = p.len().saturating_sub(2);
        for (k, w) in p.windows(2).enumerate() {
            let (a, b) = (w[0], w[1]);
            let lo = Point::new(a.x.min(b.x), a.y.min(b.y));
            let hi = Point::new(a.x.max(b.x), a.y.max(b.y));
            let ends = [(k == 0).then_some(a), (k == last).then_some(b)];
            segs.push(Seg { line, k, a, b, lo, hi, ends });
        }
    }
    // Candidate junctions: each line's two ends, then the points where segments are cut.
    let mut cand: Vec<Point> = Vec::new();
    for p in polys {
        if let (Some(&a), Some(&b)) = (p.first(), p.last()) {
            cand.extend([a, b]);
        }
    }
    // cuts on each segment: (t along it, candidate)
    let mut cuts: Vec<Vec<(f64, usize)>> = (0..segs.len()).map(|_| Vec::new()).collect();
    let mut order: Vec<usize> = (0..segs.len()).collect();
    order.sort_by(|&i, &j| segs[i].lo.x.total_cmp(&segs[j].lo.x));
    let mut pairs = 0usize;
    'sweep: for (n, &i) in order.iter().enumerate() {
        for &j in order.iter().skip(n + 1) {
            let (s, t) = (&segs[i], &segs[j]);
            if t.lo.x > s.hi.x + SNAP {
                break;
            }
            pairs += 1;
            if pairs > MAX_PAIRS {
                break 'sweep;
            }
            if t.lo.y > s.hi.y + SNAP || s.lo.y > t.hi.y + SNAP {
                continue;
            }
            // neighbours along a line already meet at their shared point
            let neighbours = s.line == t.line && s.k.abs_diff(t.k) <= 1;
            if !neighbours && let Some((u, v)) = polyline::segment_intersection(s.a, s.b, t.a, t.b) {
                let c = cand.len();
                cand.push(s.a.lerp(s.b, u));
                cuts[i].push((u, c));
                cuts[j].push((v, c));
            }
            // a line ending on (or just short of) another: a T junction
            for (end_of, on) in [(i, j), (j, i)] {
                let (e, o) = (&segs[end_of], &segs[on]);
                if e.line == o.line && e.k.abs_diff(o.k) <= 1 {
                    continue;
                }
                for p in e.ends.into_iter().flatten() {
                    let (q, t) = polyline::closest_on_segment(p, o.a, o.b);
                    if q.distance(p) <= SNAP {
                        let c = cand.len();
                        cand.push(q);
                        cuts[on].push((t, c));
                    }
                }
            }
        }
    }
    // Candidates within SNAP of each other are one junction.
    let node_of = cluster(&cand);
    let count = node_of.iter().copied().max().map_or(0, |m| m + 1);
    let mut nodes = vec![Point::ZERO; count];
    let mut placed = vec![false; count];
    for (c, &n) in node_of.iter().enumerate() {
        if let (Some(slot), Some(done), Some(&p)) = (nodes.get_mut(n), placed.get_mut(n), cand.get(c))
            && !*done
        {
            *slot = p;
            *done = true;
        }
    }
    // Walk each line from junction to junction.
    let mut edges = Vec::new();
    let mut ends = 0usize;
    for (line, p) in polys.iter().enumerate() {
        let (start, end) = (node_of.get(ends).copied(), node_of.get(ends + 1).copied());
        ends += 2;
        let (Some(start), Some(end)) = (start, end) else { continue };
        let mut cur = start;
        let mut piece = vec![nodes[cur]];
        let base = first_seg[line];
        for k in 0..p.len().saturating_sub(1) {
            let mut here = cuts.get(base + k).cloned().unwrap_or_default();
            here.sort_by(|x, y| x.0.total_cmp(&y.0));
            for (_, c) in here {
                let Some(&node) = node_of.get(c) else { continue };
                piece.push(nodes[node]);
                push_edge(&mut edges, cur, node, std::mem::replace(&mut piece, vec![nodes[node]]));
                cur = node;
            }
            if let Some(&b) = p.get(k + 1) {
                piece.push(b);
            }
        }
        if let Some(last) = piece.last_mut() {
            *last = nodes[end];
        }
        push_edge(&mut edges, cur, end, piece);
    }
    (nodes, edges)
}

/// Adds the piece of line from junction `a` to `b`, unless it is a point.
fn push_edge(edges: &mut Vec<Edge>, a: usize, b: usize, pts: Vec<Point>) {
    let pts = polyline::dedup(&pts, 1e-9);
    let len = polyline::length(&pts);
    if pts.len() >= 2 && (a != b || len > 2.0 * SNAP) {
        edges.push(Edge { a, b, pts, len });
    }
}

/// Groups points closer than [`SNAP`] (transitively): each point's group, numbered from 0.
fn cluster(pts: &[Point]) -> Vec<usize> {
    let mut parent: Vec<usize> = (0..pts.len()).collect();
    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while let Some(&p) = parent.get(i) {
            if p == i {
                break;
            }
            let gp = parent.get(p).copied().unwrap_or(p);
            if let Some(slot) = parent.get_mut(i) {
                *slot = gp;
            }
            i = p;
        }
        i
    }
    let cell = |p: Point| ((p.x / SNAP).floor() as i64, (p.y / SNAP).floor() as i64);
    let mut grid: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    for (i, &p) in pts.iter().enumerate() {
        let (cx, cy) = cell(p);
        for dx in -1..=1 {
            for dy in -1..=1 {
                for &j in grid.get(&(cx + dx, cy + dy)).map(Vec::as_slice).unwrap_or_default() {
                    if pts[j].distance(p) <= SNAP {
                        let (ri, rj) = (root(&mut parent, i), root(&mut parent, j));
                        if let Some(slot) = parent.get_mut(ri) {
                            *slot = rj;
                        }
                    }
                }
            }
        }
        grid.entry((cx, cy)).or_default().push(i);
    }
    let mut number: HashMap<usize, usize> = HashMap::new();
    (0..pts.len())
        .map(|i| {
            let r = root(&mut parent, i);
            let next = number.len();
            *number.entry(r).or_insert(next)
        })
        .collect()
}

/// A distance in Dijkstra's queue, smallest first.
#[derive(PartialEq)]
struct Near(f64, usize);

impl Eq for Near {}

impl PartialOrd for Near {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Near {
    fn cmp(&self, other: &Self) -> Ordering {
        other.0.total_cmp(&self.0).then_with(|| other.1.cmp(&self.1))
    }
}

/// One closed tour per part of the lines, the parts in the order the needle reaches them from
/// `hint`: each tour the edges it passes, and whether from their `a` to their `b`.
fn route(nodes: &[Point], edges: &[Edge], hint: Option<Point>) -> Vec<Vec<(usize, bool)>> {
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); nodes.len()];
    for (i, e) in edges.iter().enumerate() {
        if let Some(l) = adj.get_mut(e.a) {
            l.push(i);
        }
        if e.b != e.a
            && let Some(l) = adj.get_mut(e.b)
        {
            l.push(i);
        }
    }
    // the parts: junctions joined by lines
    let mut part = vec![usize::MAX; nodes.len()];
    let mut parts: Vec<Vec<usize>> = Vec::new();
    for s in 0..nodes.len() {
        if part[s] != usize::MAX || adj[s].is_empty() {
            continue;
        }
        let id = parts.len();
        let mut members = vec![s];
        part[s] = id;
        let mut i = 0;
        while let Some(&n) = members.get(i) {
            i += 1;
            for &e in &adj[n] {
                let o = other(&edges[e], n);
                if part[o] == usize::MAX {
                    part[o] = id;
                    members.push(o);
                }
            }
        }
        parts.push(members);
    }
    let mut done = vec![false; parts.len()];
    let mut at = hint.or_else(|| edges.first().map(|e| nodes[e.a]));
    let mut tours = Vec::new();
    while let Some((p, start)) = parts
        .iter()
        .enumerate()
        .filter(|(i, _)| !done[*i])
        .flat_map(|(i, m)| m.iter().map(move |&n| (i, n)))
        .min_by(|x, y| at.map_or(Ordering::Equal, |q| nodes[x.1].distance(q).total_cmp(&nodes[y.1].distance(q))))
    {
        done[p] = true;
        tours.push(tour(&parts[p], start, nodes, edges, &adj));
        at = Some(nodes[start]);
    }
    tours
}

/// The other end of `e` from junction `n`.
fn other(e: &Edge, n: usize) -> usize {
    if e.a == n { e.b } else { e.a }
}

/// A closed tour from `start` over every edge of one part: odd junctions paired along the
/// shortest ways (those edges passed twice), then Hierholzer's Euler tour.
fn tour(members: &[usize], start: usize, nodes: &[Point], edges: &[Edge], adj: &[Vec<usize>]) -> Vec<(usize, bool)> {
    let degree = |n: usize| adj[n].iter().map(|&e| if edges[e].a == edges[e].b { 2 } else { 1 }).sum::<usize>();
    let mut odd: Vec<usize> = members.iter().copied().filter(|&n| degree(n) % 2 == 1).collect();
    // every edge once, then the ways retraced
    let mut passes: Vec<usize> = members.iter().flat_map(|&n| adj[n].iter().copied().filter(move |&e| edges[e].a == n)).collect();
    while let Some(u) = odd.pop() {
        let Some((v, way)) = nearest(u, &odd, nodes.len(), edges, adj) else { break };
        odd.retain(|&o| o != v);
        passes.extend(way);
    }
    // the passes at each junction
    let mut at: HashMap<usize, Vec<usize>> = HashMap::new();
    for (k, &e) in passes.iter().enumerate() {
        at.entry(edges[e].a).or_default().push(k);
        if edges[e].b != edges[e].a {
            at.entry(edges[e].b).or_default().push(k);
        }
    }
    let mut used = vec![false; passes.len()];
    let mut next: HashMap<usize, usize> = HashMap::new();
    let mut stack: Vec<(usize, Option<(usize, bool)>)> = vec![(start, None)];
    let mut circuit = Vec::with_capacity(passes.len());
    while let Some(&(v, _)) = stack.last() {
        let list = at.get(&v).map(Vec::as_slice).unwrap_or_default();
        let i = next.entry(v).or_insert(0);
        let mut step = None;
        while let Some(&k) = list.get(*i) {
            *i += 1;
            if !used[k] {
                step = Some(k);
                break;
            }
        }
        match step {
            Some(k) => {
                used[k] = true;
                let e = passes[k];
                let forward = edges[e].a == v;
                stack.push((if forward { edges[e].b } else { edges[e].a }, Some((e, forward))));
            }
            None => {
                if let Some((_, Some(pass))) = stack.pop() {
                    circuit.push(pass);
                }
            }
        }
    }
    circuit.reverse();
    circuit
}

/// The odd junction nearest `u` along the lines, and the edges of the way there.
fn nearest(u: usize, odd: &[usize], count: usize, edges: &[Edge], adj: &[Vec<usize>]) -> Option<(usize, Vec<usize>)> {
    let mut dist = vec![f64::INFINITY; count];
    let mut via: Vec<Option<usize>> = vec![None; count];
    let mut heap = BinaryHeap::new();
    *dist.get_mut(u)? = 0.0;
    heap.push(Near(0.0, u));
    while let Some(Near(d, n)) = heap.pop() {
        if d > dist[n] {
            continue;
        }
        if n != u && odd.contains(&n) {
            let mut way = Vec::new();
            let mut at = n;
            while let Some(e) = via[at] {
                way.push(e);
                at = other(&edges[e], at);
            }
            return Some((n, way));
        }
        for &e in &adj[n] {
            let o = other(&edges[e], n);
            let nd = d + edges[e].len;
            if nd < dist[o] {
                dist[o] = nd;
                via[o] = Some(e);
                heap.push(Near(nd, o));
            }
        }
    }
    None
}

/// The tours' stitches: each edge sewn in the run style the first time it is passed, as a single
/// run when it is passed again.
fn sew(tours: &[Vec<(usize, bool)>], edges: &[Edge], params: &RunParams, trim: f64) -> Vec<Stitch> {
    let step = params.stitch_length.max(0.3);
    let once = RunParams { passes: 1, ..params.clone() };
    let mut out = Out::new();
    let mut sewn = vec![false; edges.len()];
    for tour in tours {
        let mut piece: Vec<Point> = Vec::new();
        let mut mode = None;
        for &(e, forward) in tour {
            let Some(edge) = edges.get(e) else { continue };
            let first = !sewn[e];
            sewn[e] = true;
            if mode != Some(first) {
                flush(&mut out, &piece, mode, (&once, step, trim));
                piece.clear();
                mode = Some(first);
            }
            let pts: Vec<Point> = if forward { edge.pts.clone() } else { edge.pts.iter().rev().copied().collect() };
            let skip = usize::from(piece.last().is_some_and(|l| pts.first().is_some_and(|f| f.distance(*l) < 1e-9)));
            piece.extend(pts.into_iter().skip(skip));
        }
        flush(&mut out, &piece, mode, (&once, step, trim));
    }
    out.finish()
}

/// Sews one stretch of a tour: in the run style (`Some(true)`) or as a single run.
fn flush(out: &mut Out, piece: &[Point], mode: Option<bool>, (once, step, trim): (&RunParams, f64, f64)) {
    let (Some(sew), Some(&first)) = (mode, piece.first()) else { return };
    if piece.len() < 2 {
        return;
    }
    out.connect(first, trim, 2.5);
    if sew {
        let stitches = run::generate(&Contour::from_points(piece, false), once, None, None);
        out.stitches(stitches.iter().filter(|s| s.cmd == Cmd::Stitch).map(Stitch::pos));
    } else {
        out.stitches(polyline::resample(piece, step, CORNER_ANGLE));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::RunKind;

    fn line(pts: &[(f64, f64)], closed: bool) -> Contour {
        Contour::from_points(&pts.iter().map(|&(x, y)| Point::new(x, y)).collect::<Vec<_>>(), closed)
    }

    fn needles(st: &[Stitch]) -> Vec<Point> {
        st.iter().filter(|s| s.cmd == Cmd::Stitch).map(|s| s.pos()).collect()
    }

    /// Distance from `q` to the nearest of the lines.
    fn off_lines(lines: &[Contour], q: Point) -> f64 {
        lines.iter().flat_map(|c| polylines(std::slice::from_ref(c))).map(|p| polyline::distance_to_polyline(&p, q)).fold(f64::INFINITY, f64::min)
    }

    /// A square with a cross in it: a crossing in the middle and four T junctions on the square.
    fn window() -> Vec<Contour> {
        vec![
            line(&[(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0)], true),
            line(&[(10.0, 0.0), (10.0, 20.0)], false),
            line(&[(0.0, 10.0), (20.0, 10.0)], false),
        ]
    }

    #[test]
    fn a_window_is_sewn_as_one_line_back_to_where_it_starts() {
        let lines = window();
        let p = RunParams { stitch_length: 2.0, ..Default::default() };
        let st = generate(&lines, &p, Some(Point::new(-5.0, -5.0)), 1e9);
        assert!(st.iter().all(|s| s.cmd == Cmd::Stitch), "one line, no jumps or trims");
        let pts = needles(&st);
        // From the corner nearest the needle, and back to it.
        assert_eq!((pts.first(), pts.last()), (Some(&Point::new(0.0, 0.0)), Some(&Point::new(0.0, 0.0))));
        // Every stitch lies along a line: no travel across the fabric.
        for w in pts.windows(2) {
            for k in 0..=4 {
                let q = w[0].lerp(w[1], f64::from(k) / 4.0);
                assert!(off_lines(&lines, q) < 1e-6, "{:?} → {:?} leaves the lines", w[0], w[1]);
            }
        }
        // Every line is sewn: each point along them is under a stitch.
        let near = |q: Point| pts.windows(2).any(|w| polyline::distance_to_segment(q, w[0], w[1]) < 1e-6);
        for c in &lines {
            for poly in polylines(std::slice::from_ref(c)) {
                let len = polyline::length(&poly);
                for k in 0..=40 {
                    let q = polyline::point_at(&poly, len * f64::from(k) / 40.0);
                    assert!(near(q), "{q:?} isn't sewn");
                }
            }
        }
        // 120 mm of lines; the four T junctions are odd, so two ways of 20 mm are sewn twice.
        let sewn: f64 = pts.windows(2).map(|w| w[0].distance(w[1])).sum();
        assert!((sewn - 160.0).abs() < 1e-6, "{sewn}");
    }

    #[test]
    fn a_line_is_sewn_then_retraced_and_triple_runs_retrace_singly() {
        let lines = [line(&[(0.0, 0.0), (30.0, 0.0)], false)];
        let p = RunParams { kind: RunKind::Triple, stitch_length: 3.0, ..Default::default() };
        let pts = needles(&generate(&lines, &p, Some(Point::new(31.0, 0.0)), 1e9));
        // from the end nearest the needle, sewn in triple stitches, back in single ones
        assert_eq!((pts.first(), pts.last()), (Some(&Point::new(30.0, 0.0)), Some(&Point::new(30.0, 0.0))));
        // ten triple stitches of three needle points each, then ten single stitches back
        assert_eq!(pts.len(), 1 + 30 + 10);
        assert!(pts[30].x.abs() < 1e-9 && pts[29].x > 2.9, "{:?}", &pts[27..32]);
        assert!(pts[30..].windows(2).all(|w| (w[1].x - w[0].x - 3.0).abs() < 1e-9), "{:?}", &pts[30..]);
    }

    #[test]
    fn parts_that_dont_touch_are_joined_and_near_misses_meet() {
        let p = RunParams { stitch_length: 2.0, ..Default::default() };
        // 10 mm apart with trims for jumps over 5 mm: a trim between the two tours.
        let apart = [line(&[(0.0, 0.0), (10.0, 0.0)], false), line(&[(0.0, 10.0), (10.0, 10.0)], false)];
        let st = generate(&apart, &p, Some(Point::new(0.0, 0.0)), 5.0);
        assert_eq!(st.iter().filter(|s| s.cmd == Cmd::Trim).count(), 1);
        // A line ending 0.2 mm short of another meets it: one line, no trim.
        let near = [line(&[(0.0, 0.0), (20.0, 0.0)], false), line(&[(10.0, 0.2), (10.0, 10.0)], false)];
        let st = generate(&near, &p, Some(Point::new(0.0, 0.0)), 5.0);
        assert!(st.iter().all(|s| s.cmd == Cmd::Stitch));
        let pts = needles(&st);
        assert_eq!((pts.first(), pts.last()), (Some(&Point::new(0.0, 0.0)), Some(&Point::new(0.0, 0.0))));
    }

    #[test]
    fn hostile_lines_route_safely() {
        let p = RunParams::default();
        assert!(generate(&[], &p, None, 2.0).is_empty());
        let odd = [line(&[(f64::NAN, 0.0), (1.0, f64::INFINITY)], false), line(&[(5.0, 5.0)], false), line(&[(0.0, 0.0), (1e-9, 0.0)], true)];
        assert!(generate(&odd, &p, None, 2.0).iter().all(|s| s.pos().is_finite()));
        // A tangle of 1,500 crossing lines routes, and stays on them.
        let mut rng = super::super::special::Rng::new(7);
        let tangle: Vec<Contour> =
            (0..1500).map(|_| line(&[(rng.unit() * 200.0, rng.unit() * 200.0), (rng.unit() * 200.0, rng.unit() * 200.0)], false)).collect();
        let st = generate(&tangle, &RunParams { stitch_length: 4.0, ..p }, None, 1e9);
        assert!(!st.is_empty() && st.iter().all(|s| s.pos().is_finite()));
    }
}
