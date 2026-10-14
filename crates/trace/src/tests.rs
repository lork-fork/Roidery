use super::*;

const BLACK: [u8; 4] = [0, 0, 0, 255];
const WHITE: [u8; 4] = [255, 255, 255, 255];

fn bw() -> TraceParams {
    TraceParams { ignore_white: true, ..TraceParams::default() }
}

fn disc(size: u32, cx: f64, cy: f64, r: f64) -> Raster {
    Raster::from_fn(size, size, |x, y| {
        let (dx, dy) = (x as f64 + 0.5 - cx, y as f64 + 0.5 - cy);
        if dx * dx + dy * dy <= r * r { BLACK } else { WHITE }
    })
}

/// Net filled area (outer contours positive, holes negative).
fn net_area(p: &vectorcraft_geom::PathData) -> f64 {
    p.subpaths.iter().map(|s| s.area()).sum()
}

#[test]
fn black_disc_is_one_closed_path_with_disc_area() {
    let r = 80.0;
    let res = trace(&disc(200, 100.0, 100.0, r), &bw());
    assert_eq!(res.paths.len(), 1);
    let p = &res.paths[0];
    assert_eq!(p.path.subpaths.len(), 1);
    assert!(p.path.subpaths[0].closed);
    assert_eq!(p.color, [0, 0, 0]);
    let want = std::f64::consts::PI * r * r;
    let got = net_area(&p.path);
    assert!((got - want).abs() / want < 0.03, "area {got} vs {want}");
    // Curves, not a pixel staircase.
    assert!(p.path.anchor_count() < 40, "anchors {}", p.path.anchor_count());
}

#[test]
fn small_disc_area_within_tolerance() {
    let r = 20.0;
    let res = trace(&disc(64, 32.0, 32.0, r), &bw());
    assert_eq!(res.paths.len(), 1);
    let want = std::f64::consts::PI * r * r;
    let got = net_area(&res.paths[0].path);
    assert!((got - want).abs() / want < 0.03, "area {got} vs {want}");
}

#[test]
fn ring_has_outer_and_hole() {
    let img = Raster::from_fn(200, 200, |x, y| {
        let d2 = (x as f64 + 0.5 - 100.0).powi(2) + (y as f64 + 0.5 - 100.0).powi(2);
        if (40.0 * 40.0..=80.0 * 80.0).contains(&d2) { BLACK } else { WHITE }
    });
    let res = trace(&img, &bw());
    assert_eq!(res.paths.len(), 1);
    assert_eq!(res.paths[0].path.subpaths.len(), 2);
    let want = std::f64::consts::PI * (80.0f64.powi(2) - 40.0f64.powi(2));
    let got = net_area(&res.paths[0].path);
    assert!((got - want).abs() / want < 0.03, "area {got} vs {want}");
    // Opposite orientations → fills the same under non-zero.
    let a: Vec<f64> = res.paths[0].path.subpaths.iter().map(|s| s.area()).collect();
    assert!(a[0] * a[1] < 0.0);
}

#[test]
fn stripes_give_one_path_per_stripe() {
    // 5 black stripes, 10 px wide, separated by 10 px of white.
    let img = Raster::from_fn(100, 60, |x, _| if (x / 10) % 2 == 0 { BLACK } else { WHITE });
    let res = trace(&img, &bw());
    assert_eq!(res.paths.len(), 5);
    for p in &res.paths {
        let a = net_area(&p.path);
        assert!((a - 600.0).abs() < 1.0, "stripe area {a}");
        assert_eq!(p.path.anchor_count(), 4, "rectangles keep their 4 corners");
    }
}

#[test]
fn two_colour_stripes_without_ignore_white_trace_both_colours() {
    let img = Raster::from_fn(100, 60, |x, _| if (x / 10) % 2 == 0 { BLACK } else { WHITE });
    let res = trace(&img, &TraceParams::default());
    assert_eq!(res.paths.len(), 10);
    assert_eq!(res.palette.len(), 2);
    let total: f64 = res.paths.iter().map(|p| net_area(&p.path)).sum();
    assert!((total - 6000.0).abs() < 2.0);
}

#[test]
fn noise_removal_drops_speckles() {
    let img = Raster::from_fn(100, 100, |x, y| {
        let square = (20..60).contains(&x) && (20..60).contains(&y);
        let speck = (x % 13 == 3 && y % 11 == 5) && !(15..65).contains(&x);
        if square || speck { BLACK } else { WHITE }
    });
    let noisy = trace(&img, &TraceParams { noise: 0, ..bw() });
    assert!(noisy.paths.len() > 10, "{} paths", noisy.paths.len());
    let clean = trace(&img, &TraceParams { noise: 10, ..bw() });
    assert_eq!(clean.paths.len(), 1);
    assert!((net_area(&clean.paths[0].path) - 1600.0).abs() < 1.0);
}

#[test]
fn noise_removal_fills_pinholes() {
    let img = Raster::from_fn(60, 60, |x, y| {
        let square = (10..50).contains(&x) && (10..50).contains(&y);
        let hole = x == 30 && y == 30;
        if square && !hole { BLACK } else { WHITE }
    });
    let res = trace(&img, &TraceParams { noise: 4, ..bw() });
    assert_eq!(res.paths.len(), 1);
    assert_eq!(res.paths[0].path.subpaths.len(), 1);
}

#[test]
fn diagonal_pixels_stay_separate() {
    let img = Raster::from_fn(4, 4, |x, y| if (x, y) == (1, 1) || (x, y) == (2, 2) { BLACK } else { WHITE });
    let res = trace(&img, &TraceParams { noise: 0, ..bw() });
    assert_eq!(res.paths.len(), 2);
}

#[test]
fn contour_loops_are_consistent() {
    let mask = vec![true, true, true, true, false, true, true, true, true];
    let comps = trace_mask(&mask, 3, 3);
    assert_eq!(comps.len(), 1);
    assert_eq!(comps[0].pixels, 8);
    assert_eq!(comps[0].outer.area2, 18);
    assert_eq!(comps[0].holes.len(), 1);
    assert_eq!(comps[0].holes[0].area2, -2);
}

#[test]
fn color_mode_palette_size_matches_request() {
    let quad = |x: u32, y: u32| match (x < 50, y < 50) {
        (true, true) => [220, 30, 30, 255],
        (false, true) => [30, 200, 40, 255],
        (true, false) => [30, 40, 210, 255],
        (false, false) => [240, 220, 20, 255],
    };
    let img = Raster::from_fn(100, 100, quad);
    let p4 = trace(&img, &TraceParams { mode: Mode::Color, colors: 4, ..TraceParams::default() });
    assert_eq!(p4.palette.len(), 4);
    assert_eq!(p4.paths.len(), 4);
    let p2 = trace(&img, &TraceParams { mode: Mode::Color, colors: 2, ..TraceParams::default() });
    assert_eq!(p2.palette.len(), 2);
    // Palette colours are close to the originals.
    assert!(p4.palette.iter().any(|c| c[0] > 200 && c[1] < 60 && c[2] < 60));
}

#[test]
fn color_mode_gradient_uses_at_most_n_colors() {
    let img = Raster::from_fn(128, 32, |x, _| [(x * 2) as u8, 255 - (x * 2) as u8, 128, 255]);
    for n in [3, 6, 16] {
        let q = quantize(&img, &TraceParams { mode: Mode::Color, colors: n, ..TraceParams::default() });
        assert!(q.palette.len() <= n as usize && q.palette.len() >= 2, "n={n}: {}", q.palette.len());
    }
}

#[test]
fn grayscale_levels() {
    let img = Raster::from_fn(256, 8, |x, _| [x as u8, x as u8, x as u8, 255]);
    let q = quantize(&img, &TraceParams { mode: Mode::Grayscale, colors: 4, ..TraceParams::default() });
    assert_eq!(q.palette.len(), 4);
    assert!(q.palette.iter().all(|c| c[0] == c[1] && c[1] == c[2]));
    let res = trace(&img, &TraceParams { mode: Mode::Grayscale, colors: 4, noise: 0, ..TraceParams::default() });
    assert_eq!(res.paths.len(), 4);
}

#[test]
fn overlapping_layers_stack() {
    // White background with a black square: overlapping → bottom layer covers everything.
    let img = Raster::from_fn(50, 50, |x, y| if (10..30).contains(&x) && (10..30).contains(&y) { BLACK } else { WHITE });
    let ab = trace(&img, &TraceParams::default());
    let ov = trace(&img, &TraceParams { method: Method::Overlapping, ..TraceParams::default() });
    assert_eq!(ab.paths.len(), 2);
    assert_eq!(ov.paths.len(), 2);
    // Abutting: white has a hole; overlapping: white is a full rectangle underneath.
    assert_eq!(ab.paths[0].path.subpaths.len(), 2);
    assert_eq!(ov.paths[0].path.subpaths.len(), 1);
    assert!((net_area(&ov.paths[0].path) - 2500.0).abs() < 1.0);
    assert_eq!(ov.paths[1].color, [0, 0, 0]);
}

#[test]
fn transparent_pixels_are_not_traced() {
    let img = Raster::from_fn(40, 40, |x, _| if x < 20 { [0, 0, 0, 0] } else { BLACK });
    let res = trace(&img, &TraceParams::default());
    assert_eq!(res.paths.len(), 1);
    assert!((net_area(&res.paths[0].path) - 800.0).abs() < 1.0);
}

#[test]
fn threshold_controls_black_and_white() {
    let img = Raster::from_fn(40, 40, |x, _| if x < 20 { [100, 100, 100, 255] } else { WHITE });
    assert_eq!(trace(&img, &TraceParams { threshold: 128, ..bw() }).paths.len(), 1);
    assert_eq!(trace(&img, &TraceParams { threshold: 90, ..bw() }).paths.len(), 0);
}

#[test]
fn fidelity_changes_anchor_count() {
    let img = Raster::from_fn(200, 200, |x, y| {
        let t = (y as f64 / 12.0).sin() * 20.0 + 100.0;
        if (x as f64) < t { BLACK } else { WHITE }
    });
    let lo = trace(&img, &TraceParams { paths: 0.0, ..bw() }).anchor_count();
    let hi = trace(&img, &TraceParams { paths: 100.0, ..bw() }).anchor_count();
    assert!(hi >= lo, "hi {hi} lo {lo}");
}

#[test]
fn snap_curves_to_lines_straightens_near_lines() {
    // A slightly jagged, almost straight edge.
    let img = Raster::from_fn(200, 100, |x, y| if y as f64 > 30.0 + (x as f64 * 0.05) + if x % 37 < 2 { 1.0 } else { 0.0 } { BLACK } else { WHITE });
    let res = trace(&img, &TraceParams { snap_curves_to_lines: true, ..bw() });
    let sp = &res.paths[0].path.subpaths[0];
    let curves = (0..sp.segment_count()).filter(|&i| !sp.segment_is_line(i)).count();
    assert_eq!(curves, 0, "{sp:?}");
}

#[test]
fn presets_resolve() {
    assert_eq!(presets().len(), 12);
    for n in PRESET_NAMES {
        assert!(preset(n).is_some(), "{n}");
    }
    assert!(preset("[Default]").is_some());
    assert_eq!(preset("16 colors").unwrap().colors, 16);
    assert_eq!(preset("Shades of Gray").unwrap().mode, Mode::Grayscale);
    assert!(preset("nope").is_none());
}

#[test]
fn params_json_roundtrip_and_partial() {
    let p = preset("6 Colors").unwrap();
    let v = serde_json::to_value(&p).unwrap();
    assert_eq!(v["mode"], "color");
    let back: TraceParams = serde_json::from_value(v).unwrap();
    assert_eq!(back, p);
    let partial: TraceParams = serde_json::from_value(serde_json::json!({"threshold": 90, "ignoreWhite": true})).unwrap();
    assert_eq!(partial.threshold, 90);
    assert!(partial.ignore_white);
    assert_eq!(partial.mode, Mode::BlackAndWhite);
}

#[test]
fn decode_png() {
    let img = disc(32, 16.0, 16.0, 10.0);
    let bytes = img.encode_png();
    let r = Raster::decode(&bytes).unwrap();
    assert_eq!(r, img);
    assert!(Raster::decode(b"not an image").is_err());
}

#[test]
fn all_presets_trace_a_colour_image() {
    let img = Raster::from_fn(64, 64, |x, y| [(x * 4) as u8, (y * 4) as u8, ((x + y) * 2) as u8, 255]);
    for (name, p) in presets() {
        let r = trace(&img, &p);
        assert!(!r.paths.is_empty() || p.ignore_white, "{name}");
        for tp in &r.paths {
            assert!(tp.path.subpaths.iter().all(|s| s.closed && s.anchors.iter().all(|a| a.p.x.is_finite())));
        }
    }
}

#[test]
#[ignore = "timing; run with --release --ignored"]
fn timing_1000x1000_bw_under_300ms() {
    let img = Raster::from_fn(1000, 1000, |x, y| {
        let (fx, fy) = (x as f64, y as f64);
        let v = (fx / 37.0).sin() * (fy / 23.0).cos() + ((fx + fy) / 91.0).sin() * 0.5;
        if v > 0.2 { BLACK } else { WHITE }
    });
    let t = std::time::Instant::now();
    let res = trace(&img, &TraceParams { ignore_white: true, ..TraceParams::default() });
    let ms = t.elapsed().as_secs_f64() * 1000.0;
    eprintln!("1000x1000 B&W: {} paths, {} anchors in {ms:.1} ms", res.paths.len(), res.anchor_count());
    assert!(!res.paths.is_empty());
    assert!(ms < 300.0, "{ms} ms");
}
