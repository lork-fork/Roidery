use vectorcraft_geom::{BezPath, FillRule, PathData, Point, Rect};

use super::*;

fn square(x: f64, y: f64, s: f64) -> BezPath {
    let mut b = BezPath::new();
    b.move_to((x, y));
    b.line_to((x + s, y));
    b.line_to((x + s, y + s));
    b.line_to((x, y + s));
    b.close_path();
    b
}

fn shape(b: &BezPath) -> (PathData, Rect) {
    let p = PathData::from_bezpath(b);
    let r = p.bounds().unwrap();
    (p, r)
}

/// Straight on, without perspective.
fn front() -> View {
    View { rotation_x: 0.0, rotation_y: 0.0, rotation_z: 0.0, perspective: 0.0, ..View::default() }
}

fn extent(f: &[Facet]) -> Rect {
    let pts: Vec<Point> = f.iter().flat_map(|f| f.contours.iter().flatten().copied()).collect();
    pts.iter().fold(Rect::from_points(pts[0], pts[0]), |r, p| r.union_pt(*p))
}

/// The area the facets cover: each facet's contours counted with their winding (holes
/// subtract), whichever way round the facet runs.
fn area(f: &[Facet]) -> f64 {
    f.iter().map(|f| f.contours.iter().map(|c| signed_area(c)).sum::<f64>().abs()).sum()
}

fn close(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

#[test]
fn seen_straight_on_an_extrusion_is_its_front_face_exactly_where_the_art_is() {
    let (p, b) = shape(&square(10.0, 20.0, 100.0));
    let f = extrude(&p, FillRule::EvenOdd, b, Extrude::default(), front()).unwrap();
    assert_eq!(f.len(), 1, "the walls and back face away: only the front cap shows");
    assert_eq!(f[0].contours.len(), 1);
    assert_eq!(extent(&f), Rect::new(10.0, 20.0, 110.0, 120.0));
}

#[test]
fn turned_about_y_the_front_foreshortens_and_one_side_wall_shows() {
    let (p, b) = shape(&square(0.0, 0.0, 100.0));
    let o = Extrude { depth: 40.0, ..Extrude::default() };
    let f = extrude(&p, FillRule::EvenOdd, b, o, View { rotation_y: 30.0, ..front() }).unwrap();
    // Orthographic: the silhouette is the front's width times cos 30° plus the depth times sin 30°.
    let w = extent(&f).width();
    assert!(close(w, 100.0 * 30f64.to_radians().cos() + 40.0 * 30f64.to_radians().sin(), 1e-6), "{w}");
    assert_eq!(f.len(), 2, "the front cap and one side wall");
    // Lit from the upper left by default: the cap and the wall are shaded differently.
    assert!(!close(f[0].brightness, f[1].brightness, 1e-6));
    // The nearest face is drawn last.
    assert!(f.windows(2).all(|w| w[0].depth <= w[1].depth));
}

#[test]
fn no_depth_is_the_flat_shape_and_no_cap_shows_the_inside() {
    let (p, b) = shape(&square(0.0, 0.0, 100.0));
    let v = View { rotation_x: 20.0, rotation_y: 30.0, ..front() };
    let flat = extrude(&p, FillRule::EvenOdd, b, Extrude { depth: 0.0, ..Extrude::default() }, v).unwrap();
    assert_eq!(flat, plane(&p, FillRule::EvenOdd, b, v).unwrap());
    let solid = extrude(&p, FillRule::EvenOdd, b, Extrude::default(), v).unwrap();
    let hollow = extrude(&p, FillRule::EvenOdd, b, Extrude { cap: false, ..Extrude::default() }, v).unwrap();
    // Solid: front, right and top faces. Hollow: all four walls (two seen from inside), no caps.
    assert_eq!(solid.len(), 3);
    assert_eq!(hollow.len(), 4);
    assert!(hollow.iter().all(|f| f.contours.len() == 1 && f.contours[0].len() == 4));
}

#[test]
fn taper_shrinks_the_back_and_twist_turns_it() {
    let (p, b) = shape(&square(0.0, 0.0, 100.0));
    let side = View { rotation_y: 90.0, ..front() };
    let o = Extrude { depth: 60.0, cap: false, ..Extrude::default() };
    let straight = extent(&extrude(&p, FillRule::EvenOdd, b, o, side).unwrap());
    assert!(close(straight.height(), 100.0, 1e-6) && close(straight.width(), 60.0, 1e-6));
    // Seen from the side, a 50 % taper keeps the front's full height and halves the back's.
    let tapered = extrude(&p, FillRule::EvenOdd, b, Extrude { taper: 0.5, ..o }, side).unwrap();
    let back: Vec<Point> = tapered.iter().flat_map(|f| f.contours.iter().flatten().copied()).filter(|q| close(q.x, straight.x0, 1e-6)).collect();
    let (lo, hi) = back.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), q| (lo.min(q.y), hi.max(q.y)));
    let at_front: Vec<Point> = tapered.iter().flat_map(|f| f.contours.iter().flatten().copied()).filter(|q| close(q.x, straight.x1, 1e-6)).collect();
    let (flo, fhi) = at_front.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), q| (lo.min(q.y), hi.max(q.y)));
    let (back_h, front_h) = (hi - lo, fhi - flo);
    assert!(close(front_h, 100.0, 1e-6) && close(back_h, 50.0, 1e-6), "front {front_h}, back {back_h}");
    // Seen from behind, a 45° twist shows the back cap turned into a diamond.
    let back = View { rotation_y: 180.0, ..front() };
    let twisted = extrude(&p, FillRule::EvenOdd, b, Extrude { twist: 45.0, cap: true, ..o }, back).unwrap();
    assert_eq!(twisted.len(), 1, "only the back cap faces the viewer");
    assert!(close(extent(&twisted).width(), 100.0 * 2f64.sqrt(), 1e-6), "{:?}", extent(&twisted));
    // Its walls are cut into slices that follow the turn.
    let open = extrude(&p, FillRule::EvenOdd, b, Extrude { twist: 45.0, ..o }, View { rotation_x: -20.0, rotation_y: 30.0, ..front() }).unwrap();
    assert!(open.len() > 2 * 8, "{}", open.len());
}

#[test]
fn a_bevel_inside_shrinks_the_cap_and_outside_grows_the_walls() {
    let (p, b) = shape(&square(0.0, 0.0, 100.0));
    let bevel = Bevel { width: 10.0, height: 10.0, ..Bevel::default() };
    let inside = extrude(&p, FillRule::EvenOdd, b, Extrude { bevel: Some(Bevel { inside: true, ..bevel }), ..Extrude::default() }, front()).unwrap();
    // Straight on: the cap inset by the width, and the chamfer around it filling the outline.
    let cap = inside.iter().find(|f| close(signed_area(&f.contours[0]).abs(), 80.0 * 80.0, 1e-6));
    assert!(cap.is_some(), "an 80 × 80 cap");
    assert_eq!(extent(&inside), Rect::new(0.0, 0.0, 100.0, 100.0));
    assert!(close(area(&inside), 100.0 * 100.0, 1e-6));
    let outside = extrude(&p, FillRule::EvenOdd, b, Extrude { bevel: Some(bevel), ..Extrude::default() }, front()).unwrap();
    assert_eq!(extent(&outside), Rect::new(-10.0, -10.0, 110.0, 110.0), "the walls stand 10 pt outside the art");
    assert!(outside.iter().any(|f| close(signed_area(&f.contours[0]).abs(), 100.0 * 100.0, 1e-6)), "the cap is the art");
    // The chamfer's faces lean towards the light differently from the cap.
    assert!(inside.iter().any(|f| !close(f.brightness, inside[inside.len() - 1].brightness, 1e-3)));
}

#[test]
fn bevel_shapes_repeats_and_both_sides_change_the_surface() {
    let (p, b) = shape(&square(0.0, 0.0, 100.0));
    let v = View { rotation_x: -20.0, rotation_y: 30.0, ..front() };
    let base = Bevel { width: 12.0, height: 12.0, ..Bevel::default() };
    let classic = extrude(&p, FillRule::EvenOdd, b, Extrude { bevel: Some(base), ..Extrude::default() }, v).unwrap();
    for shape in BevelShape::ALL.into_iter().skip(1) {
        assert_ne!(
            classic,
            extrude(&p, FillRule::EvenOdd, b, Extrude { bevel: Some(Bevel { shape, ..base }), ..Extrude::default() }, v).unwrap(),
            "{shape:?}"
        );
    }
    for bevel in [Bevel { repeat: 3, ..base }, Bevel { repeat: 3, space: 0.3, ..base }, Bevel { both_sides: true, ..base }] {
        assert_ne!(classic, extrude(&p, FillRule::EvenOdd, b, Extrude { bevel: Some(bevel), ..Extrude::default() }, v).unwrap(), "{bevel:?}");
    }
    // The step shape keeps its corners: half the width at half the height.
    assert_eq!(BevelShape::Step.at(0.5), (0.5, 0.5));
    assert_eq!(BevelShape::Step.at(0.25), (0.0, 0.5));
    for shape in BevelShape::ALL {
        assert_eq!(shape.at(0.0), (0.0, 0.0), "{shape:?}");
        let (x, y) = shape.at(1.0);
        assert!(close(x, 1.0, 1e-12) && close(y, 1.0, 1e-12), "{shape:?}");
        assert_eq!(BevelShape::parse(shape.id()), Some(shape));
    }
}

#[test]
fn holes_stay_holes_in_the_caps_and_get_walls_of_their_own() {
    let mut b = square(0.0, 0.0, 100.0);
    // The hole drawn the same way round as the outline: even-odd nesting still makes it a hole.
    b.extend(square(30.0, 30.0, 40.0));
    let (p, r) = shape(&b);
    let f = extrude(&p, FillRule::EvenOdd, r, Extrude::default(), front()).unwrap();
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].contours.len(), 2);
    assert!(close(area(&f), 100.0 * 100.0 - 40.0 * 40.0, 1e-6));
    let turned = extrude(&p, FillRule::EvenOdd, r, Extrude::default(), View { rotation_y: 40.0, ..front() }).unwrap();
    // Front cap, the outline's right wall and the hole's left wall (seen through the hole).
    assert_eq!(turned.len(), 3);
}

#[test]
fn a_plane_turns_about_the_given_centre() {
    let (p, b) = shape(&square(0.0, 0.0, 100.0));
    let f = plane(&p, FillRule::EvenOdd, b, View { rotation_y: 60.0, ..front() }).unwrap();
    assert_eq!(f.len(), 1);
    let e = extent(&f);
    assert!(close(e.width(), 50.0, 1e-6) && close(e.center().x, 50.0, 1e-9));
    // Turned about the centre of a larger box, the square moves as well as narrowing.
    let wide = plane(&p, FillRule::EvenOdd, Rect::new(0.0, 0.0, 300.0, 100.0), View { rotation_y: 60.0, ..front() }).unwrap();
    assert!(close(extent(&wide).x0, 150.0 - 150.0 * 0.5, 1e-6));
    // Its back shows when turned past 90°: a plane is two-sided.
    assert_eq!(plane(&p, FillRule::EvenOdd, b, View { rotation_y: 150.0, ..front() }).unwrap().len(), 1);
}

#[test]
fn inflate_covers_the_shape_and_puffs_by_the_volume() {
    let mut c = BezPath::new();
    c.extend(vectorcraft_geom::kurbo::Shape::path_elements(&vectorcraft_geom::kurbo::Circle::new((100.0, 100.0), 50.0), 0.01));
    let (p, b) = shape(&c);
    let f = inflate(&p, FillRule::EvenOdd, b, Inflate::default(), front()).unwrap();
    // Straight on, the cushion covers the circle (sampled as a polygon) and nothing else.
    assert!(close(area(&f), std::f64::consts::PI * 50.0 * 50.0, 60.0), "{}", area(&f));
    assert!(extent(&f).x0 >= 50.0 - 1e-6 && extent(&f).x1 <= 150.0 + 1e-6);
    // Its slopes catch the light differently.
    let (lo, hi) = f.iter().fold((1.0f64, 0.0f64), |(lo, hi), f| (lo.min(f.brightness), hi.max(f.brightness)));
    assert!(hi - lo > 0.3, "{lo}..{hi}");
    // From the side: 50 % volume rounds a disc into a half sphere on a flat back.
    let side = View { rotation_y: 90.0, ..front() };
    let thick = |o: Inflate| extent(&inflate(&p, FillRule::EvenOdd, b, o, side).unwrap()).width();
    assert!(close(thick(Inflate::default()), 50.0, 2.0), "{}", thick(Inflate::default()));
    assert!(close(thick(Inflate { both_sides: true, ..Inflate::default() }), 100.0, 4.0));
    assert!(close(thick(Inflate { depth: 30.0, ..Inflate::default() }), 80.0, 2.0));
    assert!(close(thick(Inflate { volume: 25.0, ..Inflate::default() }), 25.0, 2.0));
    assert!(thick(Inflate { volume: 0.0, depth: 10.0, ..Inflate::default() }) <= 10.0 + 1e-6);
}

#[test]
fn inflate_leaves_holes_open() {
    let mut b = square(0.0, 0.0, 100.0);
    b.extend(square(30.0, 30.0, 40.0));
    let (p, r) = shape(&b);
    let f = inflate(&p, FillRule::EvenOdd, r, Inflate::default(), front()).unwrap();
    assert!(close(area(&f), 100.0 * 100.0 - 40.0 * 40.0, 1e-3), "{}", area(&f));
}

#[test]
fn malformed_and_degenerate_input_is_bounded() {
    let (p, b) = shape(&square(0.0, 0.0, 100.0));
    for r in [Rect::new(f64::NAN, 0.0, 1.0, 1.0), Rect::new(0.0, 0.0, 1e9, 1.0)] {
        assert!(extrude(&p, FillRule::EvenOdd, r, Extrude::default(), front()).is_err());
        assert!(inflate(&p, FillRule::EvenOdd, r, Inflate::default(), front()).is_err());
        assert!(plane(&p, FillRule::EvenOdd, r, front()).is_err());
    }
    let wild = Extrude {
        depth: f64::INFINITY,
        twist: f64::NAN,
        taper: -3.0,
        cap: true,
        bevel: Some(Bevel { width: f64::NAN, height: 1e12, repeat: usize::MAX, space: 7.0, ..Bevel::default() }),
    };
    let f = extrude(&p, FillRule::EvenOdd, b, wild, View { rotation_x: f64::NAN, perspective: 900.0, ..front() }).unwrap();
    assert!(f.iter().flat_map(|f| f.contours.iter().flatten()).all(|q| q.x.is_finite() && q.y.is_finite()));
    assert!(
        inflate(
            &p,
            FillRule::EvenOdd,
            b,
            Inflate { depth: -5.0, volume: f64::NAN, both_sides: true, twist: f64::INFINITY, taper: f64::NAN, cap: true },
            front()
        )
        .is_ok()
    );
    assert!(extrude(&PathData::default(), FillRule::EvenOdd, b, Extrude::default(), front()).unwrap().is_empty());
    // A line has no area to extrude.
    let mut line = BezPath::new();
    line.move_to((0.0, 0.0));
    line.line_to((100.0, 0.0));
    assert!(extrude(&PathData::from_bezpath(&line), FillRule::EvenOdd, b, Extrude::default(), front()).unwrap().is_empty());
    // Too many anchors: refused, not truncated.
    let mut many = BezPath::new();
    many.move_to((0.0, 0.0));
    for i in 0..3000 {
        many.line_to((f64::from(i), f64::from(i % 7)));
    }
    many.close_path();
    assert!(extrude(&PathData::from_bezpath(&many), FillRule::EvenOdd, b, Extrude::default(), front()).is_err());
}

#[test]
fn perspective_makes_the_near_face_larger() {
    let (p, b) = shape(&square(0.0, 0.0, 100.0));
    let o = Extrude { depth: 100.0, ..Extrude::default() };
    let flat = extent(&extrude(&p, FillRule::EvenOdd, b, o, front()).unwrap());
    let deep = extent(&extrude(&p, FillRule::EvenOdd, b, o, View { perspective: 30.0, ..front() }).unwrap());
    assert!(deep.width() > flat.width() + 1.0);
    assert!(close(deep.center().x, flat.center().x, 1e-6));
}

/// The wider the Perspective, the nearer the eye: seen straight on, the front of a cube is drawn
/// larger and larger, up to 160°, where the eye is still outside the cube.
#[test]
fn a_wider_perspective_brings_the_eye_nearer() {
    let (p, b) = shape(&square(0.0, 0.0, 100.0));
    let o = Extrude { depth: 100.0, ..Extrude::default() };
    let mut last = 100.0;
    for angle in [20.0, 60.0, 110.0, 160.0] {
        // The front face alone shows: its width is the cube's on the page.
        let width = extent(&extrude(&p, FillRule::EvenOdd, b, o, View { perspective: angle, ..front() }).unwrap()).width();
        assert!(width > last, "{angle}°: {width} after {last}");
        last = width;
    }
    // At 160° the eye is about 87.9 pt in front of the cube's centre (√3 × 50 / sin 80°), so the
    // front face, 50 pt nearer than the centre, is drawn about 2.3 times its size.
    let eye = 3f64.sqrt() * 50.0 / 80f64.to_radians().sin();
    assert!(close(last, 100.0 * eye / (eye - 50.0), 1e-6), "{last}");
}

#[test]
fn the_inflate_membrane_matches_known_solutions() {
    // ∇²φ = −1, zero on the outline: a disc of radius r peaks at r²/4, a square of side l at
    // about 0.07367 l².
    let mut c = BezPath::new();
    c.extend(vectorcraft_geom::kurbo::Shape::path_elements(&vectorcraft_geom::kurbo::Circle::new((100.0, 100.0), 50.0), 0.01));
    let (p, b) = shape(&c);
    let disc = Membrane::solve(&rings(&p, FillRule::EvenOdd, 100.0, MAX_SOLID_POINTS).unwrap(), b.origin(), b.width() / 48.0, 49, 49);
    assert!(close(disc.peak, 50.0 * 50.0 / 4.0, 625.0 * 0.02), "{}", disc.peak);
    let (p, b) = shape(&square(0.0, 0.0, 100.0));
    let sq = Membrane::solve(&rings(&p, FillRule::EvenOdd, 100.0, MAX_SOLID_POINTS).unwrap(), b.origin(), 100.0 / 48.0, 49, 49);
    assert!(close(sq.peak, 0.07367 * 100.0 * 100.0, 736.7 * 0.02), "{}", sq.peak);
    // Smooth across the middle: no crease where a distance-to-the-edge height would have one
    // (the diagonals). The second difference along a diagonal stays small and even.
    let along: Vec<f64> = (10..=90).step_by(5).map(|t| sq.at(Point::new(f64::from(t), f64::from(t)))).collect();
    // (Near the corners φ grows like r², so the bend changes sign there: that is the solution,
    // not a crease. A crease would be one bend far larger than its neighbours.)
    let bends: Vec<f64> = along.windows(3).map(|w| w[0] - 2.0 * w[1] + w[2]).collect();
    let largest = bends.iter().fold(0.0f64, |m, b| m.max(b.abs()));
    assert!(bends.windows(2).all(|w| (w[1] - w[0]).abs() < 0.4 * largest), "no kink: {bends:?}");
    // Zero (or just below, extrapolated) on the outline.
    assert!(sq.at(Point::new(0.0, 50.0)).abs() < 1.0 && sq.at(Point::new(50.0, 0.0)).abs() < 1.0);
}

/// Grid cells across a hole's edge are cut to the shape itself: a hole's piece cancelling part of
/// the outline's only works flat, and lifted onto the surface it left notches along the edge.
#[test]
fn inflate_cells_across_a_hole_edge_are_cut_to_the_shape() {
    let mut b = square(80.0, 330.0, 220.0);
    b.move_to((140.0, 390.0));
    b.line_to((140.0, 480.0));
    b.line_to((240.0, 480.0));
    b.line_to((240.0, 390.0));
    b.close_path();
    let (p, r) = shape(&b);
    let v = View { rotation_x: -21.88, rotation_y: -23.4, rotation_z: 9.06, ..front() };
    let f = inflate(&p, FillRule::EvenOdd, r, Inflate::default(), v).unwrap();
    assert!(f.iter().all(|f| f.contours.len() == 1), "every facet is one piece of the surface");
    // Straight on, the facets still cover exactly the shape.
    let flat = inflate(&p, FillRule::EvenOdd, r, Inflate::default(), front()).unwrap();
    assert!(close(area(&flat), 220.0 * 220.0 - 100.0 * 90.0, 1e-3), "{}", area(&flat));
}

/// A row of `count` small bumps, each a curve, closed along the bottom: like an outlined word,
/// many short curves.
fn bumps(count: usize) -> (PathData, Rect) {
    let mut b = BezPath::new();
    b.move_to((0.0, 20.0));
    for i in 0..count {
        let x = i as f64 * 4.0;
        b.curve_to((x + 1.0, 14.0), (x + 3.0, 14.0), (x + 4.0, 20.0));
    }
    b.line_to((count as f64 * 4.0, 30.0));
    b.line_to((0.0, 30.0));
    b.close_path();
    shape(&b)
}

/// Curves are followed within a tolerance rather than by sixteen points each: small curves take a
/// few points, so shapes of many curves (outlined type) fit; Rotate, which only turns the outline,
/// takes far more.
#[test]
fn small_curves_take_few_points_and_rotate_takes_large_outlines() {
    // 300 curves were 4,800 points at sixteen each, over the old limit of 1,024.
    let (p, r) = bumps(300);
    assert!(extrude(&p, FillRule::EvenOdd, r, Extrude::default(), front()).is_ok());
    assert!(inflate(&p, FillRule::EvenOdd, r, Inflate { volume: 20.0, ..Inflate::default() }, front()).is_ok());
    assert!(plane(&p, FillRule::EvenOdd, r, front()).is_ok());
    // 4,000 curves: too many for a solid, not for Rotate.
    let (p, r) = bumps(4000);
    assert!(extrude(&p, FillRule::EvenOdd, r, Extrude::default(), front()).unwrap_err().contains("too complex"));
    assert_eq!(plane(&p, FillRule::EvenOdd, r, front()).unwrap().len(), 1);
    // Each piece stays within the tolerance of the curve: a circle's chords sag at most 0.05 pt
    // here (1/2000 of its 100 pt size is less).
    let mut c = BezPath::new();
    c.extend(vectorcraft_geom::kurbo::Shape::path_elements(&vectorcraft_geom::kurbo::Circle::new((50.0, 50.0), 50.0), 0.01));
    let (p, r) = shape(&c);
    let ring = &rings(&p, FillRule::EvenOdd, 100.0, MAX_SOLID_POINTS).unwrap()[0];
    for (a, b) in ring.iter().zip(ring.iter().cycle().skip(1)) {
        let mid = a.midpoint(*b);
        assert!(50.0 - mid.distance(Point::new(50.0, 50.0)) <= 0.05 + 1e-3, "{a:?} {b:?}");
    }
    assert!(r.width() > 0.0);
    // A small bump takes a few points; a large curve more than sixteen where it needs them.
    let (p, r) = bumps(300);
    let small = rings(&p, FillRule::EvenOdd, r.width().max(r.height()), MAX_SOLID_POINTS).unwrap()[0].len();
    assert!(small <= 300 * 4 + 3, "{small} points for 300 bumps");
    assert!(ring.len() > 4 * 16, "a 100 pt circle within 0.05 pt: {}", ring.len());
}

/// The bucketed nearest-edge search agrees with checking every edge, inside, outside and far
/// beyond the shape.
#[test]
fn the_edge_index_finds_the_nearest_edge() {
    let (p, r) = bumps(200);
    let rings = rings(&p, FillRule::EvenOdd, r.width(), MAX_SOLID_POINTS).unwrap();
    let edges = Edges::new(&rings);
    for k in 0..400 {
        let t = f64::from(k);
        let q = Point::new(-300.0 + (t * 37.0) % 1400.0, -50.0 + (t * 13.0) % 130.0);
        let (fast, slow) = (edges.distance(q), edge_distance(&rings, q));
        assert!((fast - slow).abs() < 1e-9, "{q:?}: {fast} against {slow}");
    }
}

/// From a point outside the grid, an edge further along the grid's side can be nearer than one
/// straight ahead: the search doesn't stop before it has looked there.
#[test]
fn the_edge_index_looks_along_the_side_from_outside() {
    let ring = |b: BezPath| rings(&shape(&b).0, FillRule::EvenOdd, 64.0, MAX_SOLID_POINTS).unwrap().remove(0);
    // Straight ahead of the point, a little further up its side, and the far corner (a 64 pt grid).
    let rings = vec![ring(square(2.5, 0.0, 1.0)), ring(square(0.0, 4.2, 0.8)), ring(square(63.0, 63.0, 1.0))];
    let edges = Edges::new(&rings);
    let q = Point::new(-10.0, 0.5);
    let (fast, slow) = (edges.distance(q), edge_distance(&rings, q));
    assert!((fast - slow).abs() < 1e-9, "{fast} against {slow}");
}

/// Twist runs 0° to 360°, as documented: beyond either end it holds at that end.
#[test]
fn twist_holds_between_0_and_360_degrees() {
    let (p, b) = shape(&square(0.0, 0.0, 100.0));
    let v = View { rotation_x: -20.0, rotation_y: 30.0, ..front() };
    let at = |twist: f64| extrude(&p, FillRule::EvenOdd, b, Extrude { twist, ..Extrude::default() }, v).unwrap();
    assert_eq!(at(-45.0), at(0.0));
    assert_eq!(at(500.0), at(360.0));
    assert_ne!(at(45.0), at(0.0));
}

/// A twisted extrusion shows every wall facing the viewer: a twist bends each strip between two
/// slices into a saddle, and its normal from one corner's triangle leaned so far back that from
/// 60° on every wall was dropped. Checked against each strip's average normal, oriented away from
/// the square's axis.
#[test]
fn twisted_walls_face_the_way_they_turn() {
    let (p, b) = shape(&square(0.0, 0.0, 100.0));
    let c = b.center();
    for twist in [60.0, 120.0, 180.0, 300.0] {
        for (x, y) in [(-20.0, 30.0), (0.0, 90.0), (30.0, -60.0), (-45.0, 150.0)] {
            let v = View { rotation_x: x, rotation_y: y, ..front() };
            let o = Extrude { twist, ..Extrude::default() };
            let got = extrude(&p, FillRule::EvenOdd, b, o, v).unwrap();
            // Expected: walls (and caps) whose outward normal faces the viewer.
            let o = o.sanitized();
            let camera = Camera::new(v, b, o.depth);
            let ring = &rings(&p, FillRule::EvenOdd, b.width().max(b.height()), MAX_SOLID_POINTS).unwrap()[0];
            let slices = ((twist / 6.0).ceil() as usize).clamp(8, 64);
            let z = |t: f64| o.depth / 2.0 - o.depth * t;
            let mut expected = 0;
            for k in 0..slices {
                let (t0, t1) = (k as f64 / slices as f64, (k + 1) as f64 / slices as f64);
                let (r0, r1) = (section(ring, b, &o, t0), section(ring, b, &o, t1));
                for i in 0..ring.len() {
                    let j = (i + 1) % ring.len();
                    let q = [camera.place(r0[i], z(t0)), camera.place(r0[j], z(t0)), camera.place(r1[j], z(t1)), camera.place(r1[i], z(t1))];
                    let mut n = newell(&[q.to_vec()]).unit();
                    let mid = r0[i].midpoint(r1[j]);
                    let away = camera.rotate(V3(mid.x - c.x, mid.y - c.y, 0.0));
                    if n.dot(away) < 0.0 {
                        n = V3(-n.0, -n.1, -n.2);
                    }
                    let centre = V3((q[0].0 + q[2].0) / 2.0, (q[0].1 + q[2].1) / 2.0, (q[0].2 + q[2].2) / 2.0);
                    // Kept as the builder keeps them: facing, or seen almost edge-on, with an area.
                    let projected: Vec<Point> = q.iter().map(|v| camera.project(*v)).collect();
                    if camera.facing(centre, n) > -0.02 && signed_area(&projected).abs() >= 1e-10 {
                        expected += 1;
                    }
                }
            }
            for (n, t) in [(1.0, 0.0), (-1.0, 1.0)] {
                let cap: Vec<Point> = section(ring, b, &o, t).iter().map(|p| camera.project(camera.place(*p, z(t)))).collect();
                if camera.facing(V3::default(), camera.rotate(V3(0.0, 0.0, n))) > -0.02 && signed_area(&cap).abs() >= 1e-10 {
                    expected += 1;
                }
            }
            assert_eq!(got.len(), expected, "twist {twist}, view ({x}, {y})");
        }
    }
}

/// Inflate takes Twist, Taper and Cap like the other types: the back turned and scaled (its puff
/// too), the sides following, and without a cap no back at all.
#[test]
fn inflate_twists_tapers_and_opens_its_back() {
    let (p, b) = shape(&square(0.0, 0.0, 100.0));
    let side = View { rotation_y: 90.0, ..front() };
    let o = Inflate { depth: 60.0, volume: 0.0, ..Inflate::default() };
    // From the side, a flat (0 % volume) cushion is the extrusion: front 100 high, the back 50
    // at a 50 % taper.
    let tapered = inflate(&p, FillRule::EvenOdd, b, Inflate { taper: 0.5, ..o }, side).unwrap();
    let e = extent(&tapered);
    assert!(close(e.width(), 60.0, 1e-6) && close(e.height(), 100.0, 1e-6), "{e:?}");
    let back: Vec<Point> = tapered.iter().flat_map(|f| f.contours.iter().flatten().copied()).filter(|q| close(q.x, e.x0, 1e-6)).collect();
    let (lo, hi) = back.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), q| (lo.min(q.y), hi.max(q.y)));
    assert!(close(hi - lo, 50.0, 1e-6), "back {}", hi - lo);
    // From behind, a 45° twist shows the flat back as a diamond.
    let behind = View { rotation_y: 180.0, ..front() };
    let twisted = inflate(&p, FillRule::EvenOdd, b, Inflate { twist: 45.0, ..o }, behind).unwrap();
    assert!(close(extent(&twisted).width(), 100.0 * 2f64.sqrt(), 1e-6), "{:?}", extent(&twisted));
    // Hollow: no back; from behind the inside of the inflated front shows instead of a flat face.
    let solid = inflate(&p, FillRule::EvenOdd, b, Inflate { volume: 50.0, ..o }, behind).unwrap();
    assert_eq!(solid.len(), 1, "the flat back alone");
    let hollow = inflate(&p, FillRule::EvenOdd, b, Inflate { volume: 50.0, cap: false, ..o }, behind).unwrap();
    assert!(hollow.len() > 100 && hollow.iter().all(|f| f.contours.len() == 1 && f.contours[0].len() < 12));
    // With both sides puffed, the back puff is scaled with the taper: lower and smaller.
    let both =
        |taper: f64| extent(&inflate(&p, FillRule::EvenOdd, b, Inflate { both_sides: true, volume: 50.0, taper, depth: 0.0, ..o }, side).unwrap());
    assert!(both(0.5).width() < both(1.0).width() && both(0.5).height() <= both(1.0).height() + 1e-6);
    // Out of range values hold at the documented ends.
    let at = |i: Inflate| inflate(&p, FillRule::EvenOdd, b, i, View { rotation_x: -20.0, rotation_y: 30.0, ..front() }).unwrap();
    assert_eq!(at(Inflate { twist: -30.0, ..o }), at(o));
    assert_eq!(at(Inflate { taper: 0.0, ..o }), at(Inflate { taper: 0.01, ..o }));
}

/// At 0 % volume an inflated shape is the extruded one: twisted and tapered, their outlines
/// agree from every side (no wall goes missing in either).
#[test]
fn a_flat_twisted_inflate_matches_the_extrusion() {
    let (p, b) = shape(&square(0.0, 0.0, 100.0));
    for (x, y) in [(-20.0, 30.0), (0.0, 90.0), (30.0, -60.0), (-45.0, 150.0), (60.0, 200.0)] {
        let v = View { rotation_x: x, rotation_y: y, ..front() };
        let e = extent(&extrude(&p, FillRule::EvenOdd, b, Extrude { depth: 60.0, twist: 120.0, taper: 0.6, ..Extrude::default() }, v).unwrap());
        let i = extent(
            &inflate(&p, FillRule::EvenOdd, b, Inflate { depth: 60.0, twist: 120.0, taper: 0.6, volume: 0.0, ..Inflate::default() }, v).unwrap(),
        );
        for (a, c) in [(e.x0, i.x0), (e.y0, i.y0), (e.x1, i.x1), (e.y1, i.y1)] {
            assert!(close(a, c, 1e-6), "view ({x}, {y}): extrusion {e:?}, inflate {i:?}");
        }
    }
}

/// #1724: under the nonzero rule a contour inside another that runs the same way cuts no hole
/// (the middle is filled, as the art is drawn); one running the other way does, and so does
/// either under the even-odd rule.
#[test]
fn nonzero_shapes_keep_the_middle_a_same_way_contour_encloses() {
    let rect = |r: Rect, clockwise: bool| {
        let mut pts = vec![Point::new(r.x0, r.y0), Point::new(r.x1, r.y0), Point::new(r.x1, r.y1), Point::new(r.x0, r.y1)];
        if clockwise {
            pts.reverse();
        }
        vectorcraft_geom::SubPath::polyline(&pts, true)
    };
    let (outer, inner) = (Rect::new(0.0, 0.0, 100.0, 100.0), Rect::new(25.0, 25.0, 75.0, 75.0));
    let nested = |inner_clockwise: bool| PathData::new(vec![rect(outer, false), rect(inner, inner_clockwise)]);
    let flat = |p: &PathData, rule: FillRule| area(&plane(p, rule, outer, front()).unwrap());
    let (solid, holed) = (100.0 * 100.0, 100.0 * 100.0 - 50.0 * 50.0);
    assert!(close(flat(&nested(false), FillRule::NonZero), solid, 1e-6), "the same way round: filled");
    assert!(close(flat(&nested(true), FillRule::NonZero), holed, 1e-6), "the other way round: a hole");
    assert!(close(flat(&nested(false), FillRule::EvenOdd), holed, 1e-6));
    assert!(close(flat(&nested(true), FillRule::EvenOdd), holed, 1e-6));
    // A third contour inside the hole is an island again, whichever way it runs.
    let island = |clockwise: bool| PathData::new(vec![rect(outer, false), rect(inner, true), rect(Rect::new(40.0, 40.0, 60.0, 60.0), clockwise)]);
    for clockwise in [false, true] {
        assert!(close(flat(&island(clockwise), FillRule::NonZero), holed + 20.0 * 20.0, 1e-6), "{clockwise}");
    }
    // Extrude's caps and Inflate follow the same outline.
    let caps = |p: &PathData| area(&extrude(p, FillRule::NonZero, outer, Extrude::default(), front()).unwrap());
    assert!(caps(&nested(false)) > caps(&nested(true)) + 1000.0);
    assert!(inflate(&nested(false), FillRule::NonZero, outer, Inflate::default(), front()).is_ok());
}
