//! Stitch generation: turns embroidery objects into needle penetrations and
//! assembles a complete machine [`Pattern`] with connectors between objects.
//!
//! * [`object_stitches`] – one object → stitches (generators live in the
//!   submodules: [`run`], [`satin`], [`fill`], [`special`], [`applique`]).
//! * [`build`] / [`build_cached`] – the whole design → machine pattern, adding
//!   jumps, trims, tie-in/tie-off locks and colour changes, splitting stitches
//!   that are too long and merging ones that are too short.

mod applique;
mod fill;
mod flow;
mod out;
mod redwork;
mod run;
mod satin;
mod special;
mod turning;

use crate::direction::{Guide, strip_from_guides};
use crate::geom::{FLATTEN_TOLERANCE, Point, Polygon, Shape, poly, polyline};
use crate::model::{Design, DesignSettings, EmbObject, FillKind, FillParams, LetteringObject, ObjectId, ObjectKind};
use crate::pattern::{Cmd, Pattern, Stitch};
use fill::ScanConfig;
use out::Out;
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::ops::Range;
use std::sync::Arc;

/// Stitches for one object, in absolute design coordinates.
///
/// The first record is the object's entry point. Records use only
/// [`Cmd::Stitch`], [`Cmd::Jump`], [`Cmd::Trim`] and [`Cmd::Stop`].
/// `entry_hint` is where the needle currently is (the previous object's exit);
/// objects without an explicit entry point start as close to it as possible
/// ("closest join").
pub fn object_stitches(obj: &EmbObject, settings: &DesignSettings, entry_hint: Option<Point>) -> Vec<Stitch> {
    let hint = obj.entry.or(entry_hint);
    let exit = obj.exit;
    let trim = settings.trim_threshold;
    match &obj.kind {
        ObjectKind::Run(r) => run::generate(&r.path, &r.params, hint, exit),
        ObjectKind::Satin(s) => satin::generate(&s.column, &s.params, hint, exit, settings.max_stitch),
        ObjectKind::Fill(f) => {
            let job = FillJob { params: &f.params, splits: &f.splits, flow: &f.flow, extra_angle: 0.0 };
            fill_shape(&f.shape, job, &f.guides, hint, exit, (trim, settings.max_stitch))
        }
        ObjectKind::Applique(a) => applique::generate(&a.pieces().collect::<Vec<_>>(), &a.params, &a.covered, hint, trim),
        ObjectKind::Redwork(r) => redwork::generate(&r.lines, &r.params, hint, trim),
        ObjectKind::Lettering(l) => lettering(l, hint, exit, (trim, settings.max_stitch)),
        ObjectKind::Stitches(b) => b.stitches.iter().copied().filter(|s| s.cmd != Cmd::ColorChange && s.cmd != Cmd::End).collect(),
        ObjectKind::Decor(d) => crate::decor::stitches(d, hint),
    }
}

/// A motif's line, in a repeat one long and one wide: from `(0, 0)` to `(1, 0)`, along the run
/// and across it (positive to the right of the run: down on screen for a left-to-right line).
/// With a custom motif, its line (`m` when it has none to sew).
pub fn motif_line(m: crate::model::Motif, custom: Option<&crate::model::CustomMotif>) -> Vec<(f64, f64)> {
    run::motif_points(m, custom)
}

/// A motif made from a line drawn in the design (mm, y down), sewn as it was drawn along a
/// left-to-right run; none when the line is too small or too detailed to make one.
pub fn make_motif(name: &str, line: &[Point]) -> Option<crate::model::CustomMotif> {
    run::motif_from_line(name, line)
}

/// Number of boustrophedon sections and rows a straight fill of `polygon` at
/// `angle` degrees would use (used to choose stitch angles automatically).
pub fn fill_statistics(polygon: &Polygon, angle: f64, spacing: f64) -> Option<(usize, usize)> {
    fill::statistics(polygon, angle, spacing)
}

/// What a fill sews: its parameters, its split lines and flow lines (mm) and a turn added to
/// its stitch angle (a lettering glyph's).
#[derive(Clone, Copy)]
struct FillJob<'a> {
    params: &'a FillParams,
    splits: &'a [Vec<Point>],
    flow: &'a [Vec<Point>],
    extra_angle: f64,
}

/// Fills a shape (possibly made of several pieces) with underlay and top stitching.
/// `limits` are the trim threshold and the machine's longest stitch (mm).
fn fill_shape(shape: &Shape, job: FillJob, guides: &[Guide], hint: Option<Point>, exit: Option<Point>, limits: (f64, f64)) -> Vec<Stitch> {
    let (trim, max_stitch) = limits;
    let params = job.params;
    let mut pieces = fill::prepare(shape.polygon(FLATTEN_TOLERANCE));
    let mut out = Out::new();
    while !pieces.is_empty() {
        let here = out.pos().or(hint);
        let i = match here {
            Some(h) => {
                (0..pieces.len()).min_by(|&a, &b| pieces[a].distance_to_boundary(h).total_cmp(&pieces[b].distance_to_boundary(h))).unwrap_or(0)
            }
            None => 0,
        };
        let piece = pieces.remove(i);
        let ex = if pieces.is_empty() { exit } else { None };
        // Flow lines lead the rows of a tatami fill, whatever the inclination lines say.
        if !job.flow.is_empty() && params.kind == FillKind::Tatami {
            fill_piece(&piece, job, &mut out, here, ex, trim);
            continue;
        }
        // Two or more inclination lines bend the stitches along the shape.
        if guides.len() >= 2
            && piece.holes.is_empty()
            && matches!(params.kind, FillKind::Tatami | FillKind::Satin)
            && let Some(strip) = strip_from_guides(&piece.outer, guides)
        {
            let saved = std::mem::replace(&mut out.region, vec![piece.clone()]);
            turning::turning_fill(&piece, &strip, params, &mut out, (here, ex), trim, max_stitch);
            out.region = saved;
            continue;
        }
        // A single guide (or guides that don't form a strip) sets the angle.
        if let Some(g) = guides.first() {
            let p = FillParams { angle: g.angle(), ..params.clone() };
            fill_piece(&piece, FillJob { params: &p, extra_angle: 0.0, ..job }, &mut out, here, ex, trim);
        } else {
            fill_piece(&piece, job, &mut out, here, ex, trim);
        }
    }
    out.finish()
}

fn fill_piece(piece: &Polygon, job: FillJob, out: &mut Out, hint: Option<Point>, exit: Option<Point>, trim: f64) {
    // Everything inside the piece will be covered, so connections can run inside it.
    let saved_region = std::mem::replace(&mut out.region, vec![piece.clone()]);
    fill_piece_inner(piece, job, out, hint, exit, trim);
    out.region = saved_region;
}

/// `piece` less `push` mm across its rows at `angle` (degrees): what the rows' push leaves for
/// them to cover. `None` without push, or when nothing would be left.
fn push_in(piece: &Polygon, angle: f64, push: f64) -> Option<Polygon> {
    if !(push.is_finite() && push > 0.0) {
        return None;
    }
    let a = angle.to_radians();
    let n = Point::new(-a.sin(), a.cos()) * push.min(5.0);
    let moved = |d: Point| {
        let mut p = piece.clone();
        p.transform(&crate::geom::Affine::translate(d));
        p
    };
    let left: Vec<Polygon> = poly::intersection(&[moved(n)], &[moved(-n)]).into_iter().filter(|p| p.area() > 0.01).collect();
    // a narrow waist the push would cut through keeps the piece whole
    match <[Polygon; 1]>::try_from(left) {
        Ok([one]) => Some(one),
        Err(_) => None,
    }
}

fn fill_piece_inner(piece: &Polygon, job: FillJob, out: &mut Out, hint: Option<Point>, exit: Option<Point>, trim: f64) {
    let params = job.params;
    let angle = params.angle + job.extra_angle;
    let u = &params.underlay;
    if !matches!(params.kind, FillKind::Cross | FillKind::Meander | FillKind::Motif) && (u.edge_run || u.tatami) {
        let inset = poly::offset(std::slice::from_ref(piece), -u.inset.max(0.0));
        if u.edge_run {
            for p in &inset {
                for ring in p.rings() {
                    let here = out.pos().or(hint).unwrap_or(ring[0]);
                    let r = polyline::closed(&polyline::ring_starting_near(ring, here));
                    out.connect(r[0], trim, 2.5);
                    out.stitches(polyline::resample(&r, u.stitch_length.max(0.5), run::CORNER_ANGLE));
                }
            }
        }
        if u.tatami {
            let cfg = ScanConfig {
                angle: angle + u.angle_offset,
                row_spacing: u.spacing.max(0.5),
                stitch_length: u.stitch_length.max(0.5),
                offset: 0.5,
                travel_length: u.stitch_length.max(0.5),
                ..ScanConfig::default()
            };
            // the underlay ends back near the start, where the top stitching begins
            let h = out.pos().or(hint);
            fill::scan_fill_all(&inset, &cfg, out, h, hint, trim);
        }
    }
    // the top stitching starts at the object's start point (the underlay's end only without one)
    let here = hint.or(out.pos());
    // push compensation: rows stop short of the outline across them
    let pushed_piece = push_in(piece, angle, params.push_comp);
    let piece = pushed_piece.as_ref().unwrap_or(piece);
    match params.kind {
        FillKind::Tatami | FillKind::Satin => {
            let satin = params.kind == FillKind::Satin;
            // Satin spacing is measured between penetrations on the same side, so
            // consecutive rows (one stitch each) are half a spacing apart.
            let f = if satin { 0.5 } else { 1.0 };
            let cfg = ScanConfig {
                angle,
                row_spacing: params.spacing.max(0.1) * f,
                row_spacing_end: params.spacing_end * f,
                stitch_length: if satin { f64::INFINITY } else { params.stitch_length.max(0.5) },
                offset: params.offset,
                split_length: if satin { crate::model::effective_split_length(params.split_length) } else { 0.0 },
                pull_comp: params.pull_comp,
                travel_length: 2.5,
                random: params.random,
                texture: if satin { None } else { params.texture },
                splits: if satin { Vec::new() } else { job.splits.to_vec() },
                trapunto: params.trapunto,
                ragged: if satin || !params.ragged.is_finite() { 0.0 } else { params.ragged.clamp(0.0, 20.0) },
                profile: params.spacing_profile.iter().map(|&[t, s]| [t, s * f]).collect(),
            };
            // rows along flow lines, or straight when they make no frame for the piece
            if satin || job.flow.is_empty() || !flow::flow_fill(piece, job.flow, &cfg, out, here, exit, trim) {
                fill::scan_fill_all(std::slice::from_ref(piece), &cfg, out, here, exit, trim);
            }
        }
        FillKind::Contour => special::contour(piece, (params.spacing.max(0.2), params.rings), params.stitch_length.max(0.5), out, here, trim),
        FillKind::Spiral => special::spiral(piece, params.spacing.max(0.2), params.stitch_length.max(0.5), out, here, trim),
        FillKind::Radial => special::radial(piece, params.center, params.spacing.max(0.1), params.split_length, out, trim),
        FillKind::Cross => special::cross(piece, params.stitch_length, params.cross, angle, params.center, out, trim),
        FillKind::Meander => special::meander(piece, params.spacing.max(0.8), params.stitch_length.max(0.5), params.random, out, here, trim),
        FillKind::Motif => special::motif_fill(piece, params, angle, out, trim),
    }
}

fn lettering(l: &LetteringObject, hint: Option<Point>, exit: Option<Point>, limits: (f64, f64)) -> Vec<Stitch> {
    let trim = limits.0;
    let mut out = Out::new();
    let n = l.glyphs.len();
    for (i, g) in l.glyphs.iter().enumerate() {
        let here = out.pos().or(hint);
        let ex = if i + 1 == n { exit } else { None };
        let st = fill_shape(&g.shape, FillJob { params: &l.fill, splits: &[], flow: &[], extra_angle: g.angle }, &[], here, ex, limits);
        if let Some(first) = st.first() {
            out.connect(first.pos(), trim, 2.5);
            out.append(st);
        }
        if let Some(rp) = &l.outline {
            for ring in g.shape.polygon(FLATTEN_TOLERANCE).rings() {
                let here = out.pos().unwrap_or(ring[0]);
                let r = polyline::closed(&polyline::ring_starting_near(ring, here));
                let path = crate::geom::Contour::from_points(&r, false);
                let st = run::generate(&path, rp, None, None);
                if let Some(first) = st.first() {
                    out.connect(first.pos(), trim, 2.5);
                    out.append(st);
                }
            }
        }
    }
    out.finish()
}

// ---------------------------------------------------------------------------
// Pattern assembly
// ---------------------------------------------------------------------------

/// Where an object's stitches ended up in the built pattern.
#[derive(Clone, Debug, PartialEq)]
pub struct ObjectSpan {
    pub id: ObjectId,
    /// Index of the object in `Design::objects`.
    pub index: usize,
    /// Record range in [`Built::pattern`] (the object's stitches including its ties).
    pub range: Range<usize>,
    /// The object's own stitches as generated (no ties or connections).
    pub own: Arc<Vec<Stitch>>,
}

/// Result of [`build`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Built {
    pub pattern: Pattern,
    pub spans: Vec<ObjectSpan>,
    /// A chenille design's stops, each with what the operator sets at it (a chenille machine's
    /// single stop codes can't tell a colour from a stitch or a needle height). Empty for a
    /// design with no chenille.
    pub chenille_stops: Vec<ChenilleStop>,
}

/// One stop of a chenille design: its record in the pattern and what changes there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChenilleStop {
    pub record: usize,
    pub change: ChenilleChange,
}

/// What a chenille design's stop changes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ChenilleChange {
    /// The next thread (its index in the pattern's threads).
    Color(usize),
    /// Chain or moss from here.
    Stitch(crate::model::ChenilleStitch),
    /// The needle height from here.
    Height(u8),
}

impl Built {
    /// The object that produced record `i`, if any.
    pub fn object_at(&self, i: usize) -> Option<&ObjectSpan> {
        let k = self.spans.partition_point(|s| s.range.end <= i);
        self.spans.get(k).filter(|s| s.range.contains(&i))
    }
}

/// Per-object stitch cache so interactive edits only regenerate what changed.
#[derive(Default)]
pub struct StitchCache {
    map: HashMap<ObjectId, (u64, Arc<Vec<Stitch>>)>,
}

impl StitchCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.map.clear();
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Stitches for an object, regenerating only if the object, settings or
    /// entry hint changed since the last call.
    pub fn get(&mut self, obj: &EmbObject, settings: &DesignSettings, entry_hint: Option<Point>) -> Arc<Vec<Stitch>> {
        let key = cache_key(obj, settings, entry_hint);
        if let Some((k, v)) = self.map.get(&obj.id)
            && *k == key
        {
            return v.clone();
        }
        let v = Arc::new(object_stitches(obj, settings, entry_hint));
        self.map.insert(obj.id, (key, v.clone()));
        v
    }

    /// Drops entries for objects that no longer exist.
    pub fn retain_ids(&mut self, ids: &[ObjectId]) {
        self.map.retain(|k, _| ids.contains(k));
    }
}

fn cache_key(obj: &EmbObject, settings: &DesignSettings, entry_hint: Option<Point>) -> u64 {
    let mut h = DefaultHasher::new();
    serde_json::to_vec(&obj.kind).unwrap_or_default().hash(&mut h);
    serde_json::to_vec(&(obj.entry, obj.exit)).unwrap_or_default().hash(&mut h);
    serde_json::to_vec(settings).unwrap_or_default().hash(&mut h);
    if obj.entry.is_none()
        && let Some(p) = entry_hint
    {
        ((p.x * 100.0).round() as i64, (p.y * 100.0).round() as i64).hash(&mut h);
    }
    h.finish()
}

/// Streams records into a pattern while enforcing machine limits.
struct Emitter<'a> {
    p: Pattern,
    s: &'a DesignSettings,
    /// Thread is attached to the fabric (needs a tie-off before cutting).
    live: bool,
    /// Tie in at the start of the current object (its own setting, else the design's).
    tie_in: bool,
    /// Tie off before cutting after the object sewn last.
    tie_off: bool,
}

impl Emitter<'_> {
    fn last_move(&self) -> Option<Stitch> {
        self.p.stitches.iter().rev().find(|s| s.is_move()).copied()
    }

    fn stitch(&mut self, q: Point) {
        if let Some(prev) = self.last_move() {
            let d = prev.pos().distance(q);
            let prev_is_stitch = self.p.stitches.last().is_some_and(|s| s.cmd == Cmd::Stitch);
            if prev_is_stitch && d < 1e-9 {
                return;
            }
            if prev_is_stitch && d < self.s.min_stitch {
                // Too short to sew cleanly: skip the penetration (the next stitch
                // starts from the previous one, an error below `min_stitch`).
                return;
            }
            let max = self.s.max_stitch.max(1.0);
            if d > max {
                let k = (d / max).ceil() as usize;
                let from = prev.pos();
                for m in 1..k {
                    self.p.add(from.lerp(q, m as f64 / k as f64), Cmd::Stitch);
                }
            }
        }
        self.p.add(q, Cmd::Stitch);
    }

    fn jump(&mut self, q: Point) {
        // The first move: the machine starts where its format says (the writers add the
        // way from there), not at the page's corner.
        let Some(from) = self.last_move().map(|s| s.pos()) else {
            self.p.add(q, Cmd::Jump);
            return;
        };
        let d = from.distance(q);
        if d < 1e-9 {
            return;
        }
        let max = self.s.max_jump.max(1.0);
        let k = ((d / max).ceil() as usize).max(1);
        for m in 1..=k {
            self.p.add(from.lerp(q, m as f64 / k as f64), Cmd::Jump);
        }
    }

    /// Lock stitches at the start of a sewing run (towards `next`).
    fn tie_in(&mut self, at: Point, next: Option<Point>) {
        if !self.tie_in {
            return;
        }
        let u = next.map(|n| (n - at).normalized()).filter(|u| *u != Point::ZERO).unwrap_or(Point::new(1.0, 0.0));
        let t = self.s.tie_length.max(self.s.min_stitch + 0.05);
        for p in self.s.tie_style.points(at, u, t) {
            self.p.add(p, Cmd::Stitch);
        }
    }

    /// Lock stitches before cutting the thread.
    fn tie_off(&mut self) {
        if !self.live {
            return;
        }
        if self.tie_off {
            let moves: Vec<Point> = self.p.stitches.iter().rev().filter(|s| s.cmd == Cmd::Stitch).take(2).map(|s| s.pos()).collect();
            if moves.len() == 2 {
                let (end, prev) = (moves[0], moves[1]);
                let u = (prev - end).normalized();
                if u != Point::ZERO {
                    let t = self.s.tie_length.max(self.s.min_stitch + 0.05);
                    for p in self.s.tie_style.points(end, u, t) {
                        self.p.add(p, Cmd::Stitch);
                    }
                }
            }
        }
    }

    /// Running stitches from the needle through `way`, about [`TRAVEL_STITCH`] apart:
    /// the thread carried to the next object without a trim.
    fn travel(&mut self, way: &[Point]) {
        let Some(mut from) = self.last_move().map(|s| s.pos()) else { return };
        for &q in way {
            let k = ((from.distance(q) / TRAVEL_STITCH).ceil() as usize).max(1);
            for m in 1..=k {
                self.stitch(from.lerp(q, m as f64 / k as f64));
            }
            from = q;
        }
    }

    fn cut(&mut self) {
        if self.live {
            self.tie_off();
            self.p.trim();
            self.live = false;
        }
    }

    /// Appends an object's records.
    fn object(&mut self, st: &[Stitch]) {
        for (i, rec) in st.iter().enumerate() {
            match rec.cmd {
                Cmd::Stitch => {
                    let q = rec.pos();
                    if !self.live {
                        if self.last_move().is_none_or(|m| m.pos().distance(q) > 1e-9) {
                            self.jump(q);
                        }
                        self.p.add(q, Cmd::Stitch);
                        let next = st[i + 1..].iter().find(|s| s.cmd == Cmd::Stitch && s.pos().distance(q) > 1e-6).map(Stitch::pos);
                        self.tie_in(q, next);
                        self.live = true;
                    } else {
                        self.stitch(q);
                    }
                }
                Cmd::Jump => self.jump(rec.pos()),
                Cmd::Trim => self.cut(),
                Cmd::Stop => {
                    let p = self.p.current();
                    self.p.add(p, Cmd::Stop);
                }
                Cmd::ColorChange | Cmd::End => {}
            }
        }
    }
}

/// Length of the running stitches that carry the thread between branched objects (mm).
const TRAVEL_STITCH: f64 = 2.5;
/// Travel may leave the area that covers it for this far at each end (mm): it starts
/// at the last object's exit and ends at the next one's entry, on their edges.
const TRAVEL_SLACK: f64 = 1.0;

/// How much each object of a branch is grown (mm) to make the area that covers travel.
const SEAM: f64 = 0.05;

/// For each object of a branch but its first, what it and the rest of its branch
/// cover: thread carried into the object under that area is sewn over and hidden.
/// `None` outside branches.
fn branch_covers(design: &Design) -> Vec<Option<Vec<Polygon>>> {
    let objs = &design.objects;
    let mut out: Vec<Option<Vec<Polygon>>> = vec![None; objs.len()];
    let mut i = 0;
    while i < objs.len() {
        let Some(b) = objs[i].branch else {
            i += 1;
            continue;
        };
        let end = (i..objs.len()).find(|&k| objs[k].branch != Some(b)).unwrap_or(objs.len());
        // unions from the branch's last object back to its second
        let mut acc: Vec<Polygon> = Vec::new();
        for k in (i + 1..end).rev() {
            // what is not sewn covers nothing
            // grown a little, so pieces cut from one shape (which only touch) join up
            let mine: Vec<Polygon> = if objs[k].visible {
                poly::offset(&object_polygons(&objs[k]).into_iter().filter(|p| p.outer.len() >= 3).collect::<Vec<_>>(), SEAM)
            } else {
                Vec::new()
            };
            if !mine.is_empty() {
                acc = if acc.is_empty() { poly::union(&mine) } else { poly::union2(&acc, &mine) };
            }
            if !acc.is_empty() {
                out[k] = Some(acc.clone());
            }
        }
        i = end.max(i + 1);
    }
    out
}

/// The longest way (mm) the thread is carried under an object instead of a trim, and how much
/// longer than the straight way it may be.
const HIDDEN_TRAVEL_MAX: f64 = 40.0;
const HIDDEN_TRAVEL_DETOUR: f64 = 2.0;

/// A way from `from` to `first` (where `obj` starts) under `obj`'s own area, which it then sews
/// over: none for objects that cover nothing (runs, stitches) or when the way would leave the
/// area or wander far. `from` may lie up to [`TRAVEL_SLACK`] outside the area, so objects that
/// touch or overlap are reached; ones apart are trimmed between as before.
fn hidden_travel(obj: &EmbObject, from: Point, first: Point) -> Option<Vec<Point>> {
    if !matches!(obj.kind, ObjectKind::Fill(_) | ObjectKind::Satin(_) | ObjectKind::Lettering(_)) {
        return None;
    }
    let cover: Vec<Polygon> = object_polygons(obj).into_iter().filter(|p| p.outer.len() >= 3).collect();
    if cover.is_empty() {
        return None;
    }
    let way = crate::travel::route(from, first, &poly::union(&cover), TRAVEL_SLACK)?;
    let length: f64 = std::iter::once(from).chain(way.iter().copied()).collect::<Vec<_>>().windows(2).map(|w| w[0].distance(w[1])).sum();
    let direct = from.distance(first);
    (length.is_finite() && length <= HIDDEN_TRAVEL_MAX && length <= direct * HIDDEN_TRAVEL_DETOUR + 2.0).then_some(way)
}

/// Where a branched object without a stop bead should end so the thread can go on
/// under the objects after it: its point, under those, nearest to the next object.
/// `None` when the next object is not of the same branch and thread (the thread is
/// cut anyway) or nothing of this object is covered.
fn branch_exit(design: &Design, index: usize, covers: &[Option<Vec<Polygon>>]) -> Option<Point> {
    let objs = &design.objects;
    let obj = objs.get(index)?;
    let b = obj.branch?;
    if obj.exit.is_some() || obj.trim_after {
        return None;
    }
    let next = objs.get(index + 1).filter(|n| n.branch == Some(b) && n.visible && n.color == obj.color)?;
    let cover = covers.get(index + 1)?.as_ref()?;
    let mine = object_polygons(obj);
    if mine.is_empty() {
        return None;
    }
    let under: Vec<Vec<Point>> = poly::intersection(&mine, cover).iter().flat_map(|p| p.rings().cloned()).collect();
    // pieces that only touch what is still to come (a letter's strokes, cut apart) share no
    // area with it: their outline where it meets it
    let under = if under.iter().any(|r| !r.is_empty()) {
        under
    } else {
        let meets = |q: &Point| cover.iter().any(|c| c.contains(*q) || c.distance_to_boundary(*q) <= TRAVEL_SLACK * 0.5);
        vec![mine.iter().flat_map(|p| p.rings()).flatten().copied().filter(meets).collect()]
    };
    let target = object_polygons(next);
    let bbox = next.bbox();
    let dist = |p: Point| {
        if target.is_empty() {
            let dx = (bbox.min.x - p.x).max(p.x - bbox.max.x).max(0.0);
            let dy = (bbox.min.y - p.y).max(p.y - bbox.max.y).max(0.0);
            dx.hypot(dy)
        } else if target.iter().any(|t| t.contains(p)) {
            0.0
        } else {
            target.iter().map(|t| t.distance_to_boundary(p)).fold(f64::INFINITY, f64::min)
        }
    };
    under.iter().flatten().copied().min_by(|a, b| dist(*a).total_cmp(&dist(*b)))
}

/// Where an object should end so the next one, of the same thread, starts close by (then no
/// trim is needed, or the thread runs hidden under it: [`hidden_travel`]): its outline's point
/// nearest that object. `None` for branched objects (see [`branch_exit`]), objects with their own
/// stop bead or a trim after, the last object of a thread, or with [`DesignSettings::hide_travel`]
/// off.
fn near_next_exit(design: &Design, index: usize) -> Option<Point> {
    let objs = &design.objects;
    let obj = objs.get(index)?;
    if !design.settings.hide_travel || obj.branch.is_some() || obj.exit.is_some() || obj.trim_after {
        return None;
    }
    let next = objs.get(index + 1).filter(|n| n.visible && n.color == obj.color && n.branch.is_none() && !n.own_block)?;
    let mine = object_polygons(obj);
    let target = object_polygons(next);
    if mine.is_empty() {
        return None;
    }
    let bbox = next.bbox();
    let dist = |p: Point| {
        if target.is_empty() {
            let dx = (bbox.min.x - p.x).max(p.x - bbox.max.x).max(0.0);
            let dy = (bbox.min.y - p.y).max(p.y - bbox.max.y).max(0.0);
            dx.hypot(dy)
        } else if target.iter().any(|t| t.contains(p)) {
            0.0
        } else {
            target.iter().map(|t| t.distance_to_boundary(p)).fold(f64::INFINITY, f64::min)
        }
    };
    mine.iter().flat_map(|p| p.rings()).flatten().copied().min_by(|a, b| dist(*a).total_cmp(&dist(*b)))
}

/// `st` without its last needle point (when it has more than two).
fn without_last_stitch(st: &[Stitch]) -> Vec<Stitch> {
    let mut v = st.to_vec();
    if v.iter().filter(|s| s.cmd == Cmd::Stitch).count() > 2
        && let Some(i) = v.iter().rposition(|s| s.cmd == Cmd::Stitch)
    {
        v.remove(i);
    }
    v
}

/// Builds the full machine pattern for a design.
/// A continuous meander inside `polygon`, paths about `spacing` mm apart (randomised by
/// `random`, 0 to 1), as one line of points (longarm quilting); a shape in several pieces gives
/// them one after another.
pub fn meander_line(polygon: &Polygon, spacing: f64, random: f64) -> Vec<Point> {
    let mut out = Out::new();
    special::meander(polygon, spacing, spacing * 0.5, random, &mut out, None, f64::INFINITY);
    out.finish().iter().filter(|s| s.is_move()).map(Stitch::pos).collect()
}

pub fn build(design: &Design) -> Built {
    build_cached(design, &mut StitchCache::new())
}

/// Like [`build`] but reuses cached object stitches.
pub fn build_cached(design: &Design, cache: &mut StitchCache) -> Built {
    let settings = &design.settings;
    let mut em = Emitter { p: Pattern::new(), s: settings, live: false, tie_in: settings.tie_in, tie_off: settings.tie_off };
    em.p.name = design.name.clone();
    let mut spans = Vec::new();
    let mut current_color: Option<usize> = None;
    // the design's start and end points, on its box
    let bbox = design.bbox();
    let (start, end) = if bbox.is_empty() { (None, None) } else { (settings.start.map(|p| p.on(&bbox)), settings.end.map(|p| p.on(&bbox))) };
    // the first object entered closest to the start
    let mut pos: Option<Point> = start;
    let mut force_trim = false;
    let covers = branch_covers(design);
    // a basting run, sewn first in the first thread
    let mut basting = settings.basting.and_then(|b| b.stitches(design.bbox(), design.hoop.as_ref()));
    // a chenille design: a stop wherever the stitch (chain or moss) or the needle height changes,
    // the machine starting in chain at the first chenille object's height
    let mut chenille_stops: Vec<ChenilleStop> = Vec::new();
    let mut chenille: Option<crate::model::Chenille> = design
        .objects
        .iter()
        .find_map(|o| o.chenille.filter(|_| o.visible))
        .map(|c| crate::model::Chenille { stitch: crate::model::ChenilleStitch::Chain, ..c });
    for (index, obj) in design.objects.iter().enumerate() {
        if !obj.visible {
            continue;
        }
        let st = match branch_exit(design, index, &covers).or_else(|| near_next_exit(design, index)) {
            Some(exit) => cache.get(&EmbObject { exit: Some(exit), ..obj.clone() }, settings, pos),
            None => cache.get(obj, settings, pos),
        };
        // Omit last stitch: one needle point less at the object's end
        let st = if obj.omit_last { Arc::new(without_last_stitch(&st)) } else { st };
        let Some(first) = st.iter().find(|s| s.cmd == Cmd::Stitch).map(Stitch::pos) else { continue };
        // a new colour, or the same one as a block of its own
        if current_color != Some(obj.color) || (obj.own_block && current_color.is_some()) {
            if current_color.is_some() {
                em.cut();
                em.p.color_change();
                if chenille.is_some() {
                    let record = em.p.stitches.len().saturating_sub(1);
                    chenille_stops.push(ChenilleStop { record, change: ChenilleChange::Color(em.p.threads.len()) });
                }
            }
            em.p.threads.push(design.thread(obj.color));
            current_color = Some(obj.color);
            // picked out after: no ties
            if let Some(b) = basting.take() {
                (em.tie_in, em.tie_off) = (false, false);
                em.object(&b);
                em.cut();
            }
        } else if let (Some(p), Some(cover)) = (pos, covers.get(index).and_then(Option::as_ref))
            && em.live
            && !force_trim
            && p.distance(first) > 1e-6
            && let Some(way) = crate::travel::route(p, first, cover, TRAVEL_SLACK)
        {
            // branched: carry the thread across under what is still to be sewn
            em.travel(&way);
        } else if let Some(way) = pos
            .filter(|p| settings.hide_travel && em.live && !force_trim && obj.branch.is_none() && p.distance(first) > settings.trim_threshold)
            .and_then(|p| hidden_travel(obj, p, first))
        {
            // the same thread: carried under the object about to be sewn, which covers it
            em.travel(&way);
        } else if force_trim || pos.is_some_and(|p| p.distance(first) > settings.trim_threshold) {
            em.cut();
        }
        // Cuts above tie off the previous object; from here on ties are this object's.
        em.tie_in = obj.tie_in.unwrap_or(settings.tie_in);
        em.tie_off = obj.tie_off.unwrap_or(settings.tie_off);
        // chenille: a stop for each of the stitch and the needle height that changes here
        if let (Some(now), Some(cur)) = (obj.chenille, chenille.as_mut()) {
            let at = em.p.current();
            if now.stitch != cur.stitch {
                em.p.add(at, Cmd::Stop);
                chenille_stops.push(ChenilleStop { record: em.p.stitches.len().saturating_sub(1), change: ChenilleChange::Stitch(now.stitch) });
            }
            if now.height != cur.height {
                em.p.add(at, Cmd::Stop);
                chenille_stops.push(ChenilleStop { record: em.p.stitches.len().saturating_sub(1), change: ChenilleChange::Height(now.height) });
            }
            *cur = now;
        }
        let start = em.p.stitches.len();
        em.object(&st);
        if obj.stop_after {
            let p = em.p.current();
            em.p.add(p, Cmd::Stop);
        }
        spans.push(ObjectSpan { id: obj.id, index, range: start..em.p.stitches.len(), own: st.clone() });
        pos = em.last_move().map(|s| s.pos());
        force_trim = obj.trim_after;
    }
    em.cut();
    // the frame to the end point
    if let Some(q) = end
        && em.last_move().is_some()
    {
        em.jump(q);
    }
    if !em.p.stitches.is_empty() {
        em.p.ensure_end();
    }
    Built { pattern: em.p, spans, chenille_stops }
}

/// Convenience: outline polygon of an object for hit-testing and previews.
pub fn object_polygons(obj: &EmbObject) -> Vec<Polygon> {
    match &obj.kind {
        ObjectKind::Fill(f) => vec![f.shape.polygon(FLATTEN_TOLERANCE)],
        ObjectKind::Applique(a) => a.pieces().map(|s| s.polygon(FLATTEN_TOLERANCE)).collect(),
        ObjectKind::Satin(s) => vec![Polygon::from_ring(s.column.outline(FLATTEN_TOLERANCE))],
        ObjectKind::Lettering(l) => l.glyphs.iter().map(|g| g.shape.polygon(FLATTEN_TOLERANCE)).collect(),
        ObjectKind::Run(_) | ObjectKind::Stitches(_) | ObjectKind::Redwork(_) | ObjectKind::Decor(_) => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::{BBox, Column, Contour, Node, NodeKind, Rung};
    use crate::model::*;

    fn demo() -> Design {
        let mut d = Design::new();
        d.add(ObjectKind::Fill(FillObject::new(Shape::rect(Point::new(-20.0, -10.0), Point::new(20.0, 10.0)), FillParams::default())), 0);
        d.add(
            ObjectKind::Satin(SatinObject {
                column: Column::new(vec![
                    Rung::new(Point::new(-20.0, 12.0), Point::new(-20.0, 16.0), NodeKind::Corner),
                    Rung::new(Point::new(20.0, 12.0), Point::new(20.0, 16.0), NodeKind::Corner),
                ]),
                params: SatinParams::default(),
            }),
            1,
        );
        d.add(
            ObjectKind::Run(RunObject {
                path: Contour::new(
                    vec![Node::corner(Point::new(-20.0, 20.0)), Node::curve(Point::new(0.0, 25.0)), Node::corner(Point::new(20.0, 20.0))],
                    false,
                ),
                params: RunParams { kind: RunKind::Triple, ..Default::default() },
            }),
            1,
        );
        d.add(
            ObjectKind::Applique(AppliqueObject {
                shape: Shape::ellipse(Point::new(0.0, -30.0), 12.0, 8.0),
                params: AppliqueParams::default(),
                covered: Vec::new(),
                extra: Vec::new(),
            }),
            2,
        );
        d
    }

    #[test]
    fn build_demo_design() {
        let d = demo();
        let built = build(&d);
        let p = &built.pattern;
        assert_eq!(built.spans.len(), 4);
        assert_eq!(p.count(Cmd::ColorChange), 2);
        assert_eq!(p.threads.len(), 3);
        assert_eq!(p.count(Cmd::Stop), 2);
        assert!(p.count(Cmd::Trim) >= 3);
        assert_eq!(p.stitches.last().unwrap().cmd, Cmd::End);
        // machine limits respected
        let mut prev: Option<Point> = None;
        for s in &p.stitches {
            if s.is_move() {
                if let Some(q) = prev {
                    assert!(q.distance(s.pos()) <= d.settings.max_stitch.max(d.settings.max_jump) + 1e-6);
                }
                prev = Some(s.pos());
            }
        }
        // spans are ordered and object_at works
        let mid = built.spans[1].range.start + 3;
        assert_eq!(built.object_at(mid).unwrap().index, 1);
    }

    #[test]
    fn the_first_move_is_one_jump_to_the_first_stitch() {
        // far from the page's corner, which the machine knows nothing of
        let mut d = Design::new();
        d.add(ObjectKind::Fill(FillObject::new(Shape::rect(Point::new(200.0, 150.0), Point::new(240.0, 170.0)), FillParams::default())), 0);
        let p = build(&d).pattern;
        assert_eq!((p.stitches[0].cmd, p.stitches[1].cmd), (Cmd::Jump, Cmd::Stitch));
        assert_eq!(p.stitches[0].pos(), p.stitches[1].pos());
        assert_eq!(p.count(Cmd::Jump), 1);
        // so the frame goes nowhere outside what is sewn
        assert_eq!(p.extents(), p.bounds());
    }

    #[test]
    fn basting_goes_round_first_in_the_first_thread() {
        let mut d = demo();
        let plain = build(&d);
        let bb = d.bbox();
        d.settings.basting = Some(Basting::default());
        let built = build(&d);
        let p = &built.pattern;
        assert_eq!(p.threads, plain.pattern.threads);
        // its stitches, up to the first trim: a rectangle 5 mm round the design, ≤ 6 mm apart
        let end = p.stitches.iter().position(|s| s.cmd == Cmd::Trim).unwrap();
        let run: Vec<Point> = p.stitches[..end].iter().filter(|s| s.cmd == Cmd::Stitch).map(Stitch::pos).collect();
        let rb = run.iter().fold(BBox::EMPTY, |b, q| b.union(BBox::new(*q, *q)));
        assert!((rb.min.x - (bb.min.x - 5.0)).abs() < 1e-9 && (rb.max.y - (bb.max.y + 5.0)).abs() < 1e-9, "{rb:?} {bb:?}");
        assert!(run.windows(2).all(|w| w[0].distance(w[1]) <= 6.0 + 1e-9));
        assert!(run.first().unwrap().distance(*run.last().unwrap()) < 1e-9, "closed");
        // the objects follow, their spans past it
        assert!(built.spans[0].range.start > end);
        assert_eq!(built.spans.len(), plain.spans.len());
        // inside the hoop instead, centred on the design
        d.hoop = Some(crate::hoops::Hoop::new("H", 100.0, 80.0, crate::hoops::HoopShape::Rect));
        d.settings.basting = Some(Basting { hoop: true, margin: 4.0, ..Basting::default() });
        let p = build(&d).pattern;
        let end = p.stitches.iter().position(|s| s.cmd == Cmd::Trim).unwrap();
        let rb = p.stitches[..end].iter().filter(|s| s.cmd == Cmd::Stitch).fold(BBox::EMPTY, |b, s| b.union(BBox::new(s.pos(), s.pos())));
        assert!((rb.width() - 92.0).abs() < 1e-9 && (rb.height() - 72.0).abs() < 1e-9, "{rb:?}");
        // a hoop with no room: none
        d.settings.basting = Some(Basting { hoop: true, margin: 45.0, ..Basting::default() });
        assert_eq!(build(&d).pattern.stitch_count(), plain.pattern.stitch_count());
    }

    #[test]
    fn an_object_can_start_a_colour_block_of_its_own() {
        let mut d = demo();
        // all one colour: one block
        for o in &mut d.objects {
            o.color = 0;
        }
        assert_eq!(build(&d).pattern.color_blocks().len(), 1);
        // the second its own: two blocks of the same thread (the third sewn on in the second's)
        d.objects[1].own_block = true;
        let p = build(&d).pattern;
        assert_eq!(p.color_blocks().len(), 2);
        assert_eq!(p.stitches.iter().filter(|s| s.cmd == Cmd::ColorChange).count(), 1);
        // the third its own too: three
        d.objects[2].own_block = true;
        assert_eq!(build(&d).pattern.color_blocks().len(), 3);
        d.objects[2].own_block = false;
        assert!(p.threads.iter().all(|t| t.color == p.threads[0].color));
        // first in the design: nothing before it to split from
        d.objects[1].own_block = false;
        d.objects[0].own_block = true;
        assert_eq!(build(&d).pattern.color_blocks().len(), 1);
    }

    #[test]
    fn the_design_starts_near_its_start_point_and_ends_at_its_end_point() {
        let mut d = demo();
        d.objects.truncate(1);
        let first = |p: &Pattern| p.stitches.iter().find(|s| s.cmd == Cmd::Stitch).map(Stitch::pos).unwrap();
        let b = d.bbox();
        // a fill: started at the corner asked for
        for at in [DesignPoint::TopLeft, DesignPoint::BottomRight] {
            d.settings.start = Some(at);
            let p = build(&d).pattern;
            let corner = at.on(&b);
            let others = DesignPoint::ALL.iter().filter(|q| **q != at).map(|q| q.on(&b).distance(first(&p))).fold(f64::INFINITY, f64::min);
            assert!(first(&p).distance(corner) < others, "{at:?}: {:?}", first(&p));
        }
        // ended at the centre: a trim, then the frame there
        d.settings.end = Some(DesignPoint::Centre);
        let p = build(&d).pattern;
        let last = p.stitches.iter().rev().find(|s| s.is_move()).unwrap();
        assert_eq!(last.cmd, Cmd::Jump);
        assert!(last.pos().distance(DesignPoint::Centre.on(&b)) < 1e-9);
        assert!(p.stitches.iter().any(|s| s.cmd == Cmd::Trim));
        assert_eq!(DesignPoint::from_id("center"), Some(DesignPoint::Centre));
        assert!(DesignPoint::ALL.iter().all(|q| DesignPoint::from_id(q.id()) == Some(*q)));
    }

    #[test]
    fn a_fill_with_underlay_sews_its_top_from_the_start_point_to_the_stop_point() {
        // a 30 mm square, rows across, start at the bottom, stop at the top (the complaint: the top
        // stitching started where the underlay left off, at the top, then travelled back up)
        let shape = Shape::rect(Point::new(0.0, 0.0), Point::new(30.0, 30.0));
        let mut params = FillParams { angle: 0.0, ..FillParams::default() };
        assert!(params.underlay.tatami && params.underlay.edge_run);
        // underlay rows along the top rows: it sweeps from the start towards the stop
        params.underlay.angle_offset = 0.0;
        let job = FillJob { params: &params, splits: &[], flow: &[], extra_angle: 0.0 };
        let st = fill_shape(&shape, job, &[], Some(Point::new(15.0, 30.0)), Some(Point::new(15.0, 0.0)), (2.0, 12.1));
        let ys: Vec<f64> = st.iter().filter(|s| s.cmd == Cmd::Stitch).map(|s| s.y).collect();
        let at = |f: f64| ys[((ys.len() - 1) as f64 * f) as usize];
        // the top layer (the last part) climbs steadily and ends at the top
        assert!(at(0.70) > at(0.85) && at(0.85) > at(0.97), "{} {} {}", at(0.70), at(0.85), at(0.97));
        assert!(*ys.last().unwrap() < 2.0);
    }

    #[test]
    fn push_comp_keeps_fill_rows_short_of_the_edges_across_them() {
        let shape = Shape::rect(Point::new(0.0, 0.0), Point::new(30.0, 30.0));
        let bare =
            FillParams { angle: 0.0, underlay: FillUnderlay { edge_run: false, tatami: false, ..Default::default() }, ..FillParams::default() };
        let span = |p: &FillParams| {
            let job = FillJob { params: p, splits: &[], flow: &[], extra_angle: 0.0 };
            let ys: Vec<f64> = fill_shape(&shape, job, &[], None, None, (2.0, 12.1)).iter().filter(|s| s.cmd == Cmd::Stitch).map(|s| s.y).collect();
            (ys.iter().copied().fold(f64::INFINITY, f64::min), ys.iter().copied().fold(f64::NEG_INFINITY, f64::max))
        };
        let (a0, b0) = span(&bare);
        let (a1, b1) = span(&FillParams { push_comp: 0.6, ..bare.clone() });
        // rows run across (along x): the push takes the top and bottom rows in, not the row ends
        assert!(a0 < 0.1 && b0 > 29.5, "{a0} {b0}");
        assert!(a1 >= 0.59 && b1 <= 29.41, "{a1} {b1}");
    }

    #[test]
    fn objects_override_the_design_ties() {
        let mut d = demo();
        d.objects.truncate(1);
        let plain = build(&d).pattern;
        d.objects[0].tie_in = Some(false);
        d.objects[0].tie_off = Some(false);
        let untied = build(&d).pattern;
        // the default tie-in and tie-off are four stitches each
        assert_eq!(plain.stitch_count(), untied.stitch_count() + 8);
        d.settings.tie_in = false;
        d.settings.tie_off = false;
        d.objects[0].tie_in = Some(true);
        d.objects[0].tie_off = None;
        assert_eq!(build(&d).pattern.stitch_count(), untied.stitch_count() + 4, "tie in only");
    }

    #[test]
    fn cache_reuses_unchanged_objects() {
        let mut d = demo();
        let mut cache = StitchCache::new();
        let a = build_cached(&d, &mut cache);
        let b = build_cached(&d, &mut cache);
        assert_eq!(a, b);
        if let ObjectKind::Fill(f) = &mut d.objects[0].kind {
            f.params.angle = 0.0;
        }
        let c = build_cached(&d, &mut cache);
        assert_ne!(a.pattern.stitches.len(), 0);
        assert_ne!(a, c);
    }

    #[test]
    fn every_fill_kind_generates() {
        for kind in FillKind::ALL {
            let mut d = Design::new();
            let params = FillParams { kind, ..Default::default() };
            d.add(ObjectKind::Fill(FillObject::new(Shape::ellipse(Point::ZERO, 15.0, 10.0), params)), 0);
            let built = build(&d);
            assert!(built.pattern.stitch_count() > 50, "{kind:?}: {}", built.pattern.stitch_count());
        }
    }

    #[test]
    fn lettering_glyphs_are_filled() {
        let mut d = Design::new();
        let glyphs = vec![
            Glyph { ch: 'I', shape: Shape::rect(Point::new(0.0, 0.0), Point::new(2.0, 10.0)), angle: 0.0 },
            Glyph { ch: 'I', shape: Shape::rect(Point::new(6.0, 0.0), Point::new(8.0, 10.0)), angle: 0.0 },
        ];
        d.add(ObjectKind::Lettering(LetteringObject { glyphs, ..Default::default() }), 0);
        let built = build(&d);
        assert!(built.pattern.stitch_count() > 80);
    }
}
