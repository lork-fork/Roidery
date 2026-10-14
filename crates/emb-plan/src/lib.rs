//! Roidery stitch plan: turns the objects' stitches, in sewing order, into a machine [`Design`].
//!
//! The plan decides everything between and around the objects' own stitches:
//! - travel to each object (and each separate part inside one) as a jump, trimmed first when
//!   longer than [`PlanSettings::trim_at`], or, where fills sewn later will cover the way, as
//!   running stitches under them (no trim, no loose thread);
//! - tie-ins where the thread starts and tie-offs before it is cut, so stitches don't unravel;
//! - a colour change whenever the thread changes;
//! - stitches longer than a machine can sew split into pieces, and needle-wearing tiny stitches
//!   dropped;
//! - the whole design centred on the hoop's origin.
//!
//! [`review`] checks a planned design: its sew time and a map of thread density.

#![forbid(unsafe_code)]

pub mod review;

use kurbo::Point;
use roidery_formats::{Command, Design, MachineStitch, Thread};
use roidery_stitch::{Polyline, Stitch, StitchKind};
use serde::{Deserialize, Serialize};

/// Stitch plan settings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PlanSettings {
    /// Moves longer than this (mm) cut the thread first; shorter ones carry it over.
    pub trim_at: f64,
    /// Lock the thread where it starts.
    pub tie_in: bool,
    /// Lock the thread before it is cut.
    pub tie_off: bool,
    /// Length of each tie stitch, mm.
    pub tie_length: f64,
    /// Longest stitch the machine sews, mm; longer ones are split.
    pub max_stitch: f64,
    /// Stitches shorter than this (mm) are dropped.
    pub min_stitch: f64,
    /// Move the design so its centre is at the origin (where the machine centres the hoop).
    pub center: bool,
    /// Travel as running stitches where fills sewn later will hide them, instead of jumping.
    pub travel_under: bool,
}

impl Default for PlanSettings {
    fn default() -> Self {
        Self { trim_at: 3.0, tie_in: true, tie_off: true, tie_length: 0.5, max_stitch: 12.0, min_stitch: 0.3, center: true, travel_under: true }
    }
}

impl PlanSettings {
    fn sanitized(&self) -> Self {
        let f = |v: f64, def: f64, lo: f64, hi: f64| if v.is_finite() { v.clamp(lo, hi) } else { def };
        let d = Self::default();
        Self {
            trim_at: f(self.trim_at, d.trim_at, 0.0, 1000.0),
            tie_length: f(self.tie_length, d.tie_length, 0.2, 2.0),
            max_stitch: f(self.max_stitch, d.max_stitch, 1.0, 12.1),
            min_stitch: f(self.min_stitch, d.min_stitch, 0.0, 1.0),
            ..self.clone()
        }
    }
}

/// One object's stitches and the thread they are sewn with.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlanObject {
    pub stitches: Vec<Stitch>,
    pub thread: Thread,
    /// The area its stitches cover, as rings (even-odd), mm: travel sewn before it may run
    /// under it. Empty for objects that don't cover an area (runs, satins).
    pub cover: Vec<Polyline>,
}

/// Length of a travel stitch hidden under a fill, mm.
pub const TRAVEL_STITCH: f64 = 2.0;
/// Travel ends this close to where it starts or lands (mm) may lie outside the covering fills:
/// the stitches there sit on the objects' own edges.
const TRAVEL_END_SLACK: f64 = 0.6;
/// Covered travel is checked every this many mm.
const TRAVEL_SAMPLE: f64 = 0.5;

/// An object's cover area with its bounds, for quick rejection.
struct Cover<'a> {
    rings: &'a [Polyline],
    min: Point,
    max: Point,
}

impl<'a> Cover<'a> {
    fn new(rings: &'a [Polyline]) -> Option<Self> {
        let mut pts = rings.iter().flat_map(|r| r.points.iter()).filter(|p| p.x.is_finite() && p.y.is_finite());
        let first = *pts.next()?;
        let (min, max) = pts.fold((first, first), |(a, b), p| (Point::new(a.x.min(p.x), a.y.min(p.y)), Point::new(b.x.max(p.x), b.y.max(p.y))));
        Some(Self { rings, min, max })
    }

    /// Whether `p` is inside the area (even-odd over the rings).
    fn contains(&self, p: Point) -> bool {
        if p.x < self.min.x || p.x > self.max.x || p.y < self.min.y || p.y > self.max.y {
            return false;
        }
        let mut inside = false;
        for r in self.rings {
            for (a, b) in r.points.iter().zip(r.points.iter().cycle().skip(1)) {
                if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x) {
                    inside = !inside;
                }
            }
        }
        inside
    }
}

/// Hard cap on machine records, so the output stays writable.
pub const MAX_RECORDS: usize = 4_000_000;

/// Build the machine design from objects in sewing order.
pub fn plan(objects: &[PlanObject], settings: &PlanSettings, name: &str) -> Design {
    plan_kinds(objects, settings, name).0
}

/// [`plan`], with what each machine record sews: the kind of the object's stitch it came from
/// ([`StitchKind::Underlay`] for underlay, [`StitchKind::Travel`] for travel the plan adds, the
/// stitch's own kind for ties), one per record of the design.
pub fn plan_kinds(objects: &[PlanObject], settings: &PlanSettings, name: &str) -> (Design, Vec<StitchKind>) {
    let s = settings.sanitized();
    // Each object's cover, by index: travel before object `i` may hide under objects `i..`.
    let covers: Vec<Option<Cover>> = objects.iter().map(|o| if s.travel_under { Cover::new(&o.cover) } else { None }).collect();
    let mut p = Planner { s: &s, out: vec![], kinds: vec![], kind: StitchKind::Top, threads: vec![], pos: None, sewing: false, last_dir: None };
    for (oi, obj) in objects.iter().enumerate() {
        let pts: Vec<&Stitch> = obj.stitches.iter().filter(|st| st.x.is_finite() && st.y.is_finite()).collect();
        if pts.is_empty() {
            continue;
        }
        match p.threads.last() {
            None => p.threads.push(obj.thread.clone()),
            Some(t) if t.rgb != obj.thread.rgb => {
                p.cut(Command::ColorChange);
                p.threads.push(obj.thread.clone());
            }
            Some(_) => {}
        }
        for (i, st) in pts.iter().enumerate() {
            let to = st.point();
            if i == 0 || st.kind == StitchKind::Jump {
                // Into an object, it hides under that object and those after it; inside one,
                // only under those after it (this one's stitches there may be sewn already).
                let later = covers.get(if i == 0 { oi } else { oi + 1 }..).unwrap_or_default();
                p.kind = StitchKind::Travel;
                p.travel(to, later);
            } else {
                p.kind = st.kind;
                let next = pts.get(i + 1).filter(|n| n.kind != StitchKind::Jump).map(|n| n.point());
                p.sew(to, next);
            }
            if p.out.len() >= MAX_RECORDS {
                break;
            }
        }
    }
    p.tie_off();
    let pos = p.pos.unwrap_or(Point::ZERO);
    p.push(pos, Command::End);
    let kinds = p.kinds;
    let mut design = Design { stitches: p.out, threads: p.threads, name: name.to_string() };
    if s.center
        && let Some(b) = design.bounds()
    {
        let (cx, cy) = ((b.min_x + b.max_x) / 2.0, (b.min_y + b.max_y) / 2.0);
        for st in &mut design.stitches {
            st.x -= cx;
            st.y -= cy;
        }
    }
    (design, kinds)
}

struct Planner<'a> {
    s: &'a PlanSettings,
    out: Vec<MachineStitch>,
    /// What each record of `out` sews.
    kinds: Vec<StitchKind>,
    /// The kind of stitch being sewn now.
    kind: StitchKind,
    threads: Vec<Thread>,
    /// Where the needle is (`None` before the first move).
    pos: Option<Point>,
    /// The thread is anchored in the fabric (a tie-off is due before it is cut).
    sewing: bool,
    /// Unit direction of the last sewn stitch, for the tie-off.
    last_dir: Option<kurbo::Vec2>,
}

impl Planner<'_> {
    fn push(&mut self, p: Point, c: Command) {
        self.out.push(MachineStitch::new(p.x, p.y, c));
        self.kinds.push(self.kind);
        if c != Command::Trim && c != Command::ColorChange {
            self.pos = Some(p);
        }
    }

    /// Move to `to`: sewn as running stitches when the thread is anchored and fills in `later`
    /// will cover the way; otherwise a jump, with a trim first when it is long.
    fn travel(&mut self, to: Point, later: &[Option<Cover>]) {
        let Some(from) = self.pos else {
            self.push(to, Command::Jump);
            return;
        };
        let d = from.distance(to);
        if d < 1e-9 {
            return;
        }
        if self.sewing && d > self.s.min_stitch && covered(from, to, later) {
            let n = ((d / TRAVEL_STITCH).ceil() as usize).clamp(1, 10_000);
            for k in 1..=n {
                self.push(from.lerp(to, k as f64 / n as f64), Command::Stitch);
            }
            self.last_dir = unit(to - from);
            return;
        }
        if self.sewing && d > self.s.trim_at {
            self.cut(Command::Trim);
        }
        self.push(to, Command::Jump);
    }

    /// End the thread here: tie off, then trim or change colour.
    fn cut(&mut self, c: Command) {
        self.tie_off();
        if let Some(p) = self.pos {
            self.push(p, c);
        } else {
            self.push(Point::ZERO, c);
        }
        self.sewing = false;
    }

    fn tie_off(&mut self) {
        if !(self.sewing && self.s.tie_off) {
            return;
        }
        let (Some(p), Some(dir)) = (self.pos, self.last_dir) else { return };
        // Two small stitches back along the last stitch and forward again.
        let back = p - dir * self.s.tie_length;
        for q in [back, p, back, p] {
            self.push(q, Command::Stitch);
        }
    }

    /// Sew a stitch to `to`; `next` is the stitch after it, for the tie-in's direction.
    fn sew(&mut self, to: Point, next: Option<Point>) {
        let Some(from) = self.pos else {
            self.push(to, Command::Jump);
            return;
        };
        if !self.sewing {
            self.sewing = true;
            if self.s.tie_in {
                // Lock the thread with small stitches along the first stitch and back.
                let dir = unit(to - from).or_else(|| next.and_then(|n| unit(n - from))).unwrap_or(kurbo::Vec2::new(1.0, 0.0));
                let t = self.s.tie_length.min(from.distance(to) / 2.0).max(self.s.tie_length / 2.0);
                let fwd = from + dir * t;
                for q in [fwd, from, fwd, from] {
                    self.push(q, Command::Stitch);
                }
            }
        }
        let d = from.distance(to);
        if d < self.s.min_stitch {
            return;
        }
        let n = ((d / self.s.max_stitch).ceil() as usize).clamp(1, 1000);
        for k in 1..=n {
            self.push(from.lerp(to, k as f64 / n as f64), Command::Stitch);
        }
        self.last_dir = unit(to - from);
    }
}

/// Whether the way from `a` to `b` lies under the areas in `covers` (its ends may stick out a
/// little: they are on the objects' edges).
fn covered(a: Point, b: Point, covers: &[Option<Cover>]) -> bool {
    let covers: Vec<&Cover> = covers.iter().flatten().collect();
    if covers.is_empty() {
        return false;
    }
    let d = a.distance(b);
    let n = ((d / TRAVEL_SAMPLE).ceil() as usize).clamp(1, 100_000);
    (0..=n).map(|k| k as f64 * d / n as f64).filter(|t| *t >= TRAVEL_END_SLACK.min(d / 2.0) && *t <= d - TRAVEL_END_SLACK.min(d / 2.0)).all(|t| {
        let p = a.lerp(b, t / d);
        covers.iter().any(|c| c.contains(p))
    })
}

fn unit(v: kurbo::Vec2) -> Option<kurbo::Vec2> {
    let l = v.hypot();
    (l > 1e-9 && l.is_finite()).then(|| v / l)
}

#[cfg(test)]
mod tests {
    use super::*;
    use roidery_stitch::{Polyline, Shape, StitchSettings, StitchType, generate};

    fn thread(rgb: [u8; 3]) -> Thread {
        Thread { rgb, ..Default::default() }
    }

    fn line(x0: f64, x1: f64, y: f64) -> Vec<Stitch> {
        generate(&Shape::Paths(vec![Polyline::new(vec![Point::new(x0, y), Point::new(x1, y)], false)]), &StitchSettings::of(StitchType::Run))
    }

    fn no_center() -> PlanSettings {
        PlanSettings { center: false, ..Default::default() }
    }

    #[test]
    fn one_run_starts_with_a_jump_ties_and_ends() {
        let d = plan(&[PlanObject { stitches: line(0.0, 10.0, 0.0), thread: thread([255, 0, 0]), ..Default::default() }], &no_center(), "x");
        assert_eq!(d.stitches.first().map(|s| s.command), Some(Command::Jump));
        assert_eq!(d.stitches.last().map(|s| s.command), Some(Command::End));
        assert_eq!(d.threads.len(), 1);
        // 4 tie-in + 4 stitches of 2.5 mm + 4 tie-off.
        assert_eq!(d.stitch_count(), 12);
        assert_eq!(d.trims(), 0);
    }

    #[test]
    fn far_objects_trim_near_ones_jump_and_colours_change() {
        let red = thread([255, 0, 0]);
        let objs = [
            PlanObject { stitches: line(0.0, 10.0, 0.0), thread: red.clone(), ..Default::default() },
            PlanObject { stitches: line(11.0, 20.0, 0.0), thread: red.clone(), ..Default::default() },
            PlanObject { stitches: line(50.0, 60.0, 0.0), thread: red, ..Default::default() },
            PlanObject { stitches: line(60.0, 70.0, 10.0), thread: thread([0, 0, 255]), ..Default::default() },
        ];
        let d = plan(&objs, &no_center(), "x");
        assert_eq!(d.trims(), 1);
        assert_eq!(d.color_changes(), 1);
        assert_eq!(d.threads.len(), 2);
        // Every machine move survives the DST round trip.
        let back = roidery_formats::Format::Dst.read(&roidery_formats::Format::Dst.write(&d).unwrap()).unwrap();
        assert_eq!(back.color_changes(), 1);
        assert_eq!(back.trims(), 1);
    }

    #[test]
    fn long_stitches_split_and_tiny_ones_drop() {
        let st = vec![
            Stitch::new(Point::new(0.0, 0.0), StitchKind::Top),
            Stitch::new(Point::new(30.0, 0.0), StitchKind::Top),
            Stitch::new(Point::new(30.1, 0.0), StitchKind::Top),
        ];
        let s = PlanSettings { tie_in: false, tie_off: false, ..no_center() };
        let d = plan(&[PlanObject { stitches: st, thread: Thread::default(), ..Default::default() }], &s, "x");
        assert_eq!(d.stitch_count(), 3);
        for w in d.stitches.windows(2) {
            assert!(((w[1].x - w[0].x).powi(2) + (w[1].y - w[0].y).powi(2)).sqrt() <= 12.0 + 1e-9);
        }
    }

    #[test]
    fn centres_on_the_origin() {
        let d =
            plan(&[PlanObject { stitches: line(10.0, 30.0, 5.0), thread: Thread::default(), ..Default::default() }], &PlanSettings::default(), "x");
        let b = d.bounds().unwrap();
        assert!((b.min_x + b.max_x).abs() < 1e-9 && (b.min_y + b.max_y).abs() < 1e-9);
    }

    #[test]
    fn fill_sections_jump_inside_an_object() {
        let ring = |x0: f64, y0: f64, x1: f64, y1: f64| {
            Polyline::new(vec![Point::new(x0, y0), Point::new(x1, y0), Point::new(x1, y1), Point::new(x0, y1)], true)
        };
        let st = generate(&Shape::Area(vec![ring(0.0, 0.0, 30.0, 30.0), ring(10.0, 10.0, 20.0, 20.0)]), &StitchSettings::of(StitchType::Tatami));
        let d = plan(&[PlanObject { stitches: st, thread: Thread::default(), ..Default::default() }], &no_center(), "x");
        assert!(d.stitches.iter().filter(|s| s.command == Command::Jump).count() >= 2);
        assert!(roidery_formats::Format::Dst.write(&d).is_ok());
    }

    #[test]
    fn travel_runs_under_fills_sewn_later_instead_of_trimming() {
        let red = thread([255, 0, 0]);
        let square = Polyline::new(vec![Point::new(-5.0, -5.0), Point::new(65.0, -5.0), Point::new(65.0, 5.0), Point::new(-5.0, 5.0)], true);
        let fill = generate(&Shape::Area(vec![square.clone()]), &StitchSettings::of(StitchType::Tatami));
        let objs = [
            PlanObject { stitches: line(0.0, 10.0, 0.0), thread: red.clone(), ..Default::default() },
            PlanObject { stitches: line(50.0, 60.0, 0.0), thread: red.clone(), ..Default::default() },
            PlanObject { stitches: fill, thread: thread([0, 0, 255]), cover: vec![square] },
        ];
        // The 40 mm gap lies under the blue fill: sewn as short stitches, no trim.
        let d = plan(&objs, &no_center(), "x");
        assert_eq!(d.trims(), 0);
        let first_blue = d.stitches.iter().position(|s| s.command == Command::ColorChange).unwrap();
        let red_part = &d.stitches[..first_blue];
        assert!(!red_part.iter().skip(1).any(|s| s.command == Command::Jump));
        for w in red_part.windows(2) {
            assert!(((w[1].x - w[0].x).powi(2) + (w[1].y - w[0].y).powi(2)).sqrt() <= 2.5 + 1e-9);
        }
        // Without it, or when the fill sews first, the thread is cut.
        assert_eq!(plan(&objs, &PlanSettings { travel_under: false, ..no_center() }, "x").trims(), 1);
        let first = [objs[2].clone(), objs[0].clone(), objs[1].clone()];
        assert_eq!(plan(&first, &no_center(), "x").trims(), 1);
        // A way that leaves the fill is cut too.
        let off = [objs[0].clone(), PlanObject { stitches: line(50.0, 60.0, 20.0), ..objs[1].clone() }, objs[2].clone()];
        assert_eq!(plan(&off, &no_center(), "x").trims(), 1);
    }

    #[test]
    fn each_record_says_what_it_sews() {
        let mut stitches = line(0.0, 10.0, 0.0);
        stitches.iter_mut().for_each(|s| s.kind = StitchKind::Underlay);
        stitches.extend(line(0.0, 10.0, 2.0));
        let objs = [PlanObject { stitches, thread: thread([0, 0, 0]), cover: vec![] }];
        let (d, kinds) = plan_kinds(&objs, &no_center(), "x");
        assert_eq!(kinds.len(), d.stitches.len());
        let sewn = |k| d.stitches.iter().zip(&kinds).filter(|(s, kk)| s.command == Command::Stitch && **kk == k).count();
        assert!(sewn(StitchKind::Underlay) > 0 && sewn(StitchKind::Top) > 0);
        assert_eq!(kinds.first(), Some(&StitchKind::Travel), "the first jump in");
    }

    #[test]
    fn empty_input_is_just_the_end() {
        let d = plan(&[], &PlanSettings::default(), "x");
        assert_eq!(d.stitches.len(), 1);
        assert!(roidery_formats::Format::Dst.write(&d).is_ok());
    }
}
