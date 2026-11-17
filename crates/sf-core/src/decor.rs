//! Multi-decoration (phase 4): sequins, beads, rhinestones, cord and boring, placed along lines,
//! packed inside shapes or round their edges, or one at each point drawn. Sequins and beads are
//! sewn on with fixing stitches; cord is couched with a zigzag over it; a boring hole is outlined,
//! cut and edged with satin; rhinestones aren't sewn at all (their template is cut and set by hand).
//! The machine stops before each piece that needs the operator (to load sequins, lay a cord or cut
//! a hole): vendor device codes for sequin and cording heads aren't publicly documented.

use serde::{Deserialize, Serialize};

use crate::geom::{Point, Polygon};
use crate::pattern::{Cmd, Stitch};

/// What a decoration object places.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DecorKind {
    #[default]
    Sequin,
    Bead,
    /// Rhinestones (bling): placed on a template, not sewn.
    Stone,
    /// A cord couched along the line with a zigzag over it.
    Cord,
    /// A cut-work hole: outlined, cut (the machine stops), its edge sewn with satin.
    Boring,
    /// Printed ink (screen, transfer or direct print) beside the embroidery: shown, not sewn.
    Print,
    /// Foil pressed on beside the embroidery: shown with a metallic sheen, not sewn.
    Foil,
}

impl DecorKind {
    pub fn id(self) -> &'static str {
        match self {
            DecorKind::Sequin => "sequin",
            DecorKind::Bead => "bead",
            DecorKind::Stone => "stone",
            DecorKind::Cord => "cord",
            DecorKind::Boring => "boring",
            DecorKind::Print => "print",
            DecorKind::Foil => "foil",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            DecorKind::Sequin => "Sequins",
            DecorKind::Bead => "Beads",
            DecorKind::Stone => "Rhinestones",
            DecorKind::Cord => "Cord",
            DecorKind::Boring => "Boring",
            DecorKind::Print => "Print",
            DecorKind::Foil => "Foil",
        }
    }

    /// Shown with the design, never sewn: no stitches, no stop.
    pub fn is_virtual(self) -> bool {
        matches!(self, DecorKind::Print | DecorKind::Foil)
    }
}

/// A sequin's or stone's shape.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DecorShape {
    #[default]
    Round,
    Square,
    Star,
    Heart,
    Flower,
}

/// Where the pieces go.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DecorLayout {
    /// Along the line (or the shape's edge), a piece every size + spacing.
    #[default]
    Run,
    /// Packed inside the shape.
    Fill,
    /// Round the inside of the shape's edge, half a piece in.
    Border,
    /// One at each point of the path.
    Manual,
}

/// How a fill packs its pieces.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DecorPacking {
    /// Offset rows (a honeycomb).
    #[default]
    Hex,
    /// A square grid.
    Grid,
}

/// How a sequin is held.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Fixing {
    /// One stitch from the hole over the edge, the way the run goes (the next sequin covers it).
    #[default]
    Single,
    /// Over the edge on both sides.
    Double,
    /// Three stitches out from the hole.
    Tri,
    /// Four stitches out from the hole, a cross.
    Cross,
}

/// How a decoration object is made.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DecorParams {
    pub kind: DecorKind,
    pub shape: DecorShape,
    /// Each piece across (mm): a sequin's or stone's diameter, a bead's length, a cord's width,
    /// a boring hole's satin edge.
    pub size: f64,
    /// Sizes taken in turn (multi-sequin runs and fills); empty: `size` alone.
    pub sizes: Vec<f64>,
    /// Gap between pieces (mm); negative overlaps them (a half-overlapping "split" run at
    /// minus half the size).
    pub spacing: f64,
    pub layout: DecorLayout,
    pub packing: DecorPacking,
    /// Fill rows' angle (degrees).
    pub angle: f64,
    pub fixing: Fixing,
    /// Every other piece turned over (sequins show their other face).
    pub flip: bool,
    /// The pieces' own colour (sequins, beads, stones, cord); `None`: the thread's.
    pub color: Option<crate::threads::Rgb>,
    /// Stop the machine before the object, for the operator to load sequins, lay the cord or cut
    /// the holes.
    pub stop: bool,
}

impl Default for DecorParams {
    fn default() -> Self {
        DecorParams {
            kind: DecorKind::Sequin,
            shape: DecorShape::Round,
            size: 5.0,
            sizes: Vec::new(),
            spacing: 0.0,
            layout: DecorLayout::Run,
            packing: DecorPacking::Hex,
            angle: 0.0,
            fixing: Fixing::Single,
            flip: false,
            color: None,
            stop: true,
        }
    }
}

impl DecorParams {
    /// The defaults for `kind`: 5 mm sequins, 3 mm seed beads, SS16 stones (3.9 mm), a 2 mm cord,
    /// a 1.2 mm satin edge round a boring hole.
    pub fn of(kind: DecorKind) -> Self {
        let size = match kind {
            DecorKind::Sequin => 5.0,
            DecorKind::Bead => 3.0,
            DecorKind::Stone => 3.9,
            DecorKind::Cord => 2.0,
            DecorKind::Boring => 1.2,
            DecorKind::Print | DecorKind::Foil => 1.0,
        };
        let spacing = match kind {
            DecorKind::Stone => 0.6,
            DecorKind::Bead => 0.5,
            _ => 0.0,
        };
        let stop = !(matches!(kind, DecorKind::Stone | DecorKind::Bead) || kind.is_virtual());
        DecorParams { kind, size, spacing, stop, ..Default::default() }
    }

    /// The size of the `i`-th piece.
    pub fn size_at(&self, i: usize) -> f64 {
        let s = if self.sizes.is_empty() { self.size } else { self.sizes.get(i % self.sizes.len()).copied().unwrap_or(self.size) };
        // a size read from a file may be anything
        if s.is_finite() { s.clamp(0.5, 30.0) } else { 5.0 }
    }
}

/// One placed piece.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DecorItem {
    /// Its middle (mm).
    pub at: Point,
    /// Across (mm).
    pub size: f64,
    /// The way the run goes there (radians).
    pub angle: f64,
    #[serde(default)]
    pub flip: bool,
}

/// A decoration object: its pieces, and for cord and boring the lines they follow.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DecorObject {
    pub params: DecorParams,
    pub items: Vec<DecorItem>,
    /// Cord: the lines it lies along; boring: the holes' outlines (closed).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lines: Vec<(Vec<Point>, bool)>,
}

/// The most pieces one object places.
pub const MAX_ITEMS: usize = 20_000;

/// The most stitches one decoration object sews. A cord's zigzag follows the path's own length,
/// which document coordinates let reach kilometres, and a file can hold any number of pieces or
/// holes, so sewing stops here instead of exhausting memory.
pub const MAX_STITCHES: usize = 200_000;

/// Standard sizes to pick from: sequins (mm), rhinestones by their SS size, beads.
pub fn palette() -> Vec<(DecorKind, &'static str, f64)> {
    let mut v: Vec<(DecorKind, &'static str, f64)> =
        [("3 mm", 3.0), ("4 mm", 4.0), ("5 mm", 5.0), ("6 mm", 6.0), ("7 mm", 7.0), ("8 mm", 8.0), ("9 mm", 9.0)]
            .into_iter()
            .map(|(n, s)| (DecorKind::Sequin, n, s))
            .collect();
    // stone sizes by the trade's SS numbers, their usual diameters
    v.extend(
        [("SS6", 2.0), ("SS10", 2.8), ("SS12", 3.1), ("SS16", 3.9), ("SS20", 4.7), ("SS30", 6.4)].into_iter().map(|(n, s)| (DecorKind::Stone, n, s)),
    );
    v.extend([("Seed 11/0", 2.2), ("Seed 8/0", 3.0), ("Seed 6/0", 4.0), ("Bugle 6 mm", 6.0)].into_iter().map(|(n, s)| (DecorKind::Bead, n, s)));
    v
}

/// Pieces along a polyline (mm), one every size + spacing, the first half a piece in.
pub fn along(line: &[Point], closed: bool, p: &DecorParams, start: usize) -> Vec<DecorItem> {
    let mut pts: Vec<Point> = line.iter().copied().filter(|q| q.is_finite()).collect();
    if closed && let Some(first) = pts.first().copied() {
        pts.push(first);
    }
    let mut out = Vec::new();
    let mut k = start;
    let mut next = p.size_at(k) / 2.0;
    let mut walked = 0.0;
    for w in pts.windows(2) {
        let (Some(a), Some(b)) = (w.first().copied(), w.get(1).copied()) else { continue };
        let seg = a.distance(b);
        if seg < 1e-9 {
            continue;
        }
        while next.is_finite() && next <= walked + seg && out.len() < MAX_ITEMS {
            let t = (next - walked) / seg;
            let size = p.size_at(k);
            out.push(DecorItem { at: a.lerp(b, t), size, angle: (b - a).angle(), flip: p.flip && k % 2 == 1 });
            k += 1;
            let gap = if p.spacing.is_finite() { p.spacing.max(-size * 0.75) } else { 0.0 };
            next += (size + p.size_at(k)) / 2.0 + gap;
        }
        walked += seg;
    }
    out
}

/// Pieces packed inside a shape (mm): rows `angle` apart by a piece and the spacing, every
/// other row offset by half a piece for a honeycomb; only those whole inside.
pub fn packed(shape: &Polygon, p: &DecorParams) -> Vec<DecorItem> {
    let b = shape.bbox();
    let size = p.size_at(0);
    let gap = if p.spacing.is_finite() { p.spacing } else { 0.0 };
    let pitch = (size + gap).max(0.5);
    let (dx, row) = match p.packing {
        DecorPacking::Hex => (pitch, pitch * 3f64.sqrt() / 2.0),
        DecorPacking::Grid => (pitch, pitch),
    };
    let a = if p.angle.is_finite() { p.angle.to_radians() } else { 0.0 };
    let centre = b.center();
    let reach = (b.width().hypot(b.height())) / 2.0 + pitch;
    let rows = (2.0 * reach / row).ceil() as i64;
    let cols = (2.0 * reach / dx).ceil() as i64;
    if rows.saturating_mul(cols) > (MAX_ITEMS as i64) * 8 {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut k = 0usize;
    for r in 0..=rows {
        let v = -reach + r as f64 * row;
        let shift = if p.packing == DecorPacking::Hex && r % 2 == 1 { dx / 2.0 } else { 0.0 };
        let mut line = Vec::new();
        for c in 0..=cols {
            let u = -reach + c as f64 * dx + shift;
            let at = centre + Point::new(u, v).rotated(a);
            let size = p.size_at(k + line.len());
            if shape.contains(at) && shape.distance_to_boundary(at) >= size / 2.0 * 0.9 {
                line.push(DecorItem { at, size, angle: a, flip: false });
            }
        }
        // back and forth, row after row, so the way from one row to the next is short
        if r % 2 == 1 {
            line.reverse();
        }
        for mut it in line {
            it.flip = p.flip && k % 2 == 1;
            out.push(it);
            k += 1;
            if out.len() >= MAX_ITEMS {
                return out;
            }
        }
    }
    out
}

/// Pieces round the inside of a shape's edge, half a piece in.
pub fn border(shape: &Polygon, p: &DecorParams) -> Vec<DecorItem> {
    let inset = crate::geom::poly::offset(std::slice::from_ref(shape), -p.size_at(0) / 2.0);
    let mut out = Vec::new();
    for poly in &inset {
        for ring in poly.rings() {
            let k = out.len();
            out.extend(along(ring, true, p, k));
        }
    }
    out
}

/// A run of stitches from `a` to `b` in steps of at most `step` (mm), `a` left out.
fn walk(out: &mut Vec<Stitch>, a: Point, b: Point, step: f64) {
    let d = a.distance(b);
    let n = (d / step).ceil().max(1.0) as usize;
    for i in 1..=n.min(2000) {
        out.push(Stitch::at(a.lerp(b, i as f64 / n as f64), Cmd::Stitch));
    }
}

/// The stitches that sew `obj` on, starting near `hint` (mm). Stones sew nothing.
pub fn stitches(obj: &DecorObject, hint: Option<Point>) -> Vec<Stitch> {
    let p = &obj.params;
    let mut out: Vec<Stitch> = Vec::new();
    if p.kind.is_virtual() {
        return out;
    }
    let first = match p.kind {
        DecorKind::Cord | DecorKind::Boring => obj.lines.first().and_then(|(l, _)| l.first().copied()),
        _ => obj.items.first().map(|i| i.at),
    };
    let Some(first) = first else { return out };
    let _ = hint;
    out.push(Stitch::at(first, Cmd::Jump));
    if p.stop && p.kind != DecorKind::Stone {
        out.push(Stitch::at(first, Cmd::Stop));
    }
    match p.kind {
        DecorKind::Stone => return Vec::new(),
        DecorKind::Sequin => {
            let mut here = first;
            for it in &obj.items {
                if out.len() >= MAX_STITCHES {
                    break;
                }
                walk(&mut out, here, it.at, 2.5);
                let r = it.size / 2.0 + 0.3;
                let dirs: &[f64] = match p.fixing {
                    Fixing::Single => &[0.0],
                    Fixing::Double => &[0.0, 180.0],
                    Fixing::Tri => &[90.0, 210.0, 330.0],
                    Fixing::Cross => &[0.0, 90.0, 180.0, 270.0],
                };
                for d in dirs {
                    let edge = it.at + Point::new(r, 0.0).rotated(it.angle + d.to_radians());
                    out.push(Stitch::at(edge, Cmd::Stitch));
                    out.push(Stitch::at(it.at, Cmd::Stitch));
                }
                // a single fixing ends over the edge, under the next sequin
                if p.fixing == Fixing::Single {
                    out.pop();
                    here = out.last().map_or(it.at, Stitch::pos);
                } else {
                    here = it.at;
                }
            }
        }
        DecorKind::Bead => {
            let mut here = first;
            for it in &obj.items {
                if out.len() >= MAX_STITCHES {
                    break;
                }
                let half = Point::new(it.size / 2.0, 0.0).rotated(it.angle);
                let (a, b) = (it.at - half, it.at + half);
                walk(&mut out, here, a, 2.5);
                // through the bead and back, so it lies flat
                out.push(Stitch::at(b, Cmd::Stitch));
                out.push(Stitch::at(a, Cmd::Stitch));
                out.push(Stitch::at(b, Cmd::Stitch));
                here = b;
            }
        }
        DecorKind::Cord => {
            // a zigzag over the cord, as wide as it and a little more, 2.5 mm a swing
            for (line, closed) in &obj.lines {
                if out.len() >= MAX_STITCHES {
                    break;
                }
                let mut pts = line.clone();
                if *closed && let Some(f) = pts.first().copied() {
                    pts.push(f);
                }
                let w = p.size / 2.0 + 0.4;
                let mut side = 1.0;
                let mut walked = 0.0;
                if let Some(s) = pts.first() {
                    out.push(Stitch::at(*s, Cmd::Stitch));
                }
                for seg in pts.windows(2) {
                    let (Some(a), Some(b)) = (seg.first().copied(), seg.get(1).copied()) else { continue };
                    let len = a.distance(b);
                    if !len.is_finite() || len < 1e-9 {
                        continue;
                    }
                    let n = (b - a).normalized().perp();
                    let mut t = 2.5 - walked % 2.5;
                    while t <= len && out.len() < MAX_STITCHES {
                        out.push(Stitch::at(a.lerp(b, t / len) + n * (w * side), Cmd::Stitch));
                        side = -side;
                        t += 2.5;
                    }
                    walked += len;
                }
                if let Some(e) = pts.last() {
                    out.push(Stitch::at(*e, Cmd::Stitch));
                }
            }
        }
        DecorKind::Boring => {
            // the hole's outline run, a stop to cut it, then a satin edge round it
            for (ring, _) in &obj.lines {
                if out.len() >= MAX_STITCHES {
                    break;
                }
                let Some(start) = ring.first().copied() else { continue };
                let mut prev = start;
                out.push(Stitch::at(start, Cmd::Stitch));
                for q in ring.iter().skip(1).chain(std::iter::once(&start)) {
                    walk(&mut out, prev, *q, 2.0);
                    prev = *q;
                }
                out.push(Stitch::at(start, Cmd::Stop));
                let poly = Polygon::from_ring(ring.clone());
                let outer = crate::geom::poly::offset(std::slice::from_ref(&poly), p.size);
                let edge: Vec<Point> = outer.first().map(|o| o.outer.clone()).unwrap_or_default();
                if edge.len() < 3 {
                    continue;
                }
                // swing between the hole's edge and the satin's outer edge, 0.4 mm apart
                let perimeter: f64 = ring.iter().zip(ring.iter().cycle().skip(1)).map(|(a, b)| a.distance(*b)).sum();
                let n = ((perimeter / 0.4).ceil() as usize).clamp(8, 4000);
                let at = |line: &[Point], t: f64| -> Point {
                    let total: f64 = line.iter().zip(line.iter().cycle().skip(1)).map(|(a, b)| a.distance(*b)).sum();
                    let mut goal = t * total;
                    for (a, b) in line.iter().zip(line.iter().cycle().skip(1)) {
                        let l = a.distance(*b);
                        if goal <= l && l > 1e-12 {
                            return a.lerp(*b, goal / l);
                        }
                        goal -= l;
                    }
                    line.first().copied().unwrap_or(start)
                };
                // the outer ring starts nearest the hole's start
                let off = edge.iter().enumerate().min_by(|a, b| a.1.distance(start).total_cmp(&b.1.distance(start))).map_or(0, |(i, _)| i);
                let mut edge_r: Vec<Point> = edge.iter().skip(off).chain(edge.iter().take(off)).copied().collect();
                // the same way round as the hole
                let turn = |r: &[Point]| r.iter().zip(r.iter().cycle().skip(1)).map(|(a, b)| a.cross(*b)).sum::<f64>();
                if turn(ring) * turn(&edge_r) < 0.0 {
                    edge_r.reverse();
                    edge_r.rotate_right(1);
                }
                for i in 0..=n {
                    let t = i as f64 / n as f64;
                    let q = if i % 2 == 0 { at(ring, t) } else { at(&edge_r, t) };
                    out.push(Stitch::at(q, Cmd::Stitch));
                }
            }
        }
        DecorKind::Print | DecorKind::Foil => {}
    }
    out
}

/// A rough outline of each piece (mm) for display and hit-testing.
pub fn outlines(obj: &DecorObject) -> Vec<Vec<Point>> {
    match obj.params.kind {
        DecorKind::Cord | DecorKind::Boring | DecorKind::Print | DecorKind::Foil => obj.lines.iter().map(|(l, _)| l.clone()).collect(),
        _ => obj
            .items
            .iter()
            .map(|it| (0..=12).map(|k| it.at + Point::new(it.size / 2.0, 0.0).rotated(std::f64::consts::TAU * f64::from(k) / 12.0)).collect())
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn print_and_foil_are_shown_not_sewn() {
        let ring = vec![Point::new(0.0, 0.0), Point::new(10.0, 0.0), Point::new(10.0, 10.0), Point::new(0.0, 10.0)];
        for kind in [DecorKind::Print, DecorKind::Foil] {
            let obj = DecorObject { params: DecorParams::of(kind), items: Vec::new(), lines: vec![(ring.clone(), true)] };
            assert!(stitches(&obj, None).is_empty(), "{kind:?}");
            assert!(!obj.params.stop);
            assert_eq!(outlines(&obj).len(), 1);
        }
    }

    #[test]
    fn pieces_go_along_lines_inside_shapes_and_round_edges() {
        let p = DecorParams { size: 4.0, spacing: 1.0, ..DecorParams::default() };
        let line = vec![Point::new(0.0, 0.0), Point::new(50.0, 0.0)];
        let items = along(&line, false, &p, 0);
        // the first at 2 mm, then every 5 mm: 2, 7 … 47
        assert_eq!(items.len(), 10);
        assert!((items[0].at.x - 2.0).abs() < 1e-9 && (items[1].at.x - 7.0).abs() < 1e-9);
        let square = Polygon::from_ring(vec![Point::new(0.0, 0.0), Point::new(30.0, 0.0), Point::new(30.0, 30.0), Point::new(0.0, 30.0)]);
        let fill = packed(&square, &p);
        // a honeycomb packs more than a grid
        let grid = packed(&square, &DecorParams { packing: DecorPacking::Grid, ..p.clone() });
        assert!(fill.len() > grid.len() && grid.len() >= 25, "{} {}", fill.len(), grid.len());
        assert!(fill.iter().all(|i| square.distance_to_boundary(i.at) >= 1.8));
        let edge = border(&square, &p);
        assert!(edge.len() >= 20 && edge.iter().all(|i| square.distance_to_boundary(i.at) > 1.5), "{}", edge.len());
        // sizes in turn, every other one flipped
        let multi = DecorParams { sizes: vec![3.0, 6.0], flip: true, ..p.clone() };
        let m = along(&line, false, &multi, 0);
        assert_eq!((m[0].size, m[1].size, m[0].flip, m[1].flip), (3.0, 6.0, false, true));
    }

    #[test]
    fn a_very_long_cord_stops_at_the_stitch_cap() {
        // issue #66: one 10 km line used to give 4,000,004 stitches
        let p = DecorParams::of(DecorKind::Cord);
        let line = vec![Point::new(0.0, 0.0), Point::new(10_000_000.0, 0.0)];
        let lines = vec![(line.clone(), false), (line, false)];
        let st = stitches(&DecorObject { params: p, items: Vec::new(), lines }, None);
        assert!(st.len() <= MAX_STITCHES + 8, "{}", st.len());
        // a non-finite length sews nothing along it rather than looping
        let bad = vec![Point::new(0.0, 0.0), Point::new(f64::INFINITY, 0.0)];
        let st = stitches(&DecorObject { params: DecorParams::of(DecorKind::Cord), items: Vec::new(), lines: vec![(bad, false)] }, None);
        assert!(st.len() <= 8, "{}", st.len());
        // pieces from a file, each far from the last, stop at the cap too
        let far: Vec<DecorItem> =
            (0..2_000).map(|k| DecorItem { at: Point::new(if k % 2 == 0 { 0.0 } else { 10_000.0 }, 0.0), ..DecorItem::default() }).collect();
        for kind in [DecorKind::Sequin, DecorKind::Bead] {
            let st = stitches(&DecorObject { params: DecorParams::of(kind), items: far.clone(), lines: Vec::new() }, None);
            assert!(st.len() <= MAX_STITCHES + 2_100, "{kind:?}: {}", st.len());
        }
    }

    #[test]
    fn sequins_beads_and_cord_are_sewn_stones_are_not() {
        let line = vec![Point::new(0.0, 0.0), Point::new(30.0, 0.0)];
        let obj = |kind: DecorKind| {
            let p = DecorParams::of(kind);
            DecorObject { items: along(&line, false, &p, 0), lines: vec![(line.clone(), false)], params: p }
        };
        let seq = stitches(&obj(DecorKind::Sequin), None);
        assert!(seq.iter().any(|s| s.cmd == Cmd::Stop), "a stop to load the sequins");
        assert!(seq.iter().filter(|s| s.cmd == Cmd::Stitch).count() >= 12);
        assert!(stitches(&obj(DecorKind::Stone), None).is_empty());
        let bead = stitches(&obj(DecorKind::Bead), None);
        assert!(!bead.iter().any(|s| s.cmd == Cmd::Stop) && bead.len() > 20);
        let cord = stitches(&obj(DecorKind::Cord), None);
        // the zigzag swings to both sides of the cord
        assert!(cord.iter().any(|s| s.y > 1.0) && cord.iter().any(|s| s.y < -1.0));
        let ring: Vec<Point> = (0..24).map(|k| Point::new(10.0, 0.0).rotated(std::f64::consts::TAU * f64::from(k) / 24.0)).collect();
        let bore = DecorObject { params: DecorParams::of(DecorKind::Boring), items: Vec::new(), lines: vec![(ring, true)] };
        let st = stitches(&bore, None);
        assert!(st.iter().filter(|s| s.cmd == Cmd::Stop).count() >= 2, "load, then cut");
        assert!(st.iter().any(|s| s.pos().length() > 10.8), "the satin edge reaches beyond the hole");
    }
}
