//! Typography tests: rich-text editing, caret navigation, selection, area flow, composer,
//! hyphenation, OpenType features, area type options.

use super::*;
use kurbo::{Affine, Circle, Shape};
use vectorcraft_doc::{CharStyle, Justify, TextKind, TextRun};
use vectorcraft_geom::PathData;

fn db() -> &'static FontDb {
    FontDb::global()
}

fn style(size: f64) -> CharStyle {
    CharStyle { size, ..CharStyle::default() }
}

fn run(text: &str, size: f64) -> TextRun {
    TextRun { text: text.into(), style: style(size) }
}

fn area_path(text: &str, st: CharStyle, frame: &BezPath, justify: Justify) -> TextObject {
    let mut t = TextObject::point(Point::ZERO, text, st);
    t.kind = TextKind::Area { frame: PathData::from_bezpath(frame) };
    t.xf = Affine::IDENTITY;
    t.para.justify = justify;
    t
}

fn area(text: &str, st: CharStyle, frame: Rect, justify: Justify) -> TextObject {
    area_path(text, st, &frame.to_path(0.1), justify)
}

const COPY: &str = "Typography is the craft of arranging type to make written language legible, readable and appealing when displayed. \
The arrangement of type involves selecting typefaces, point sizes, line lengths, line spacing and letter spacing, and adjusting the space \
between pairs of letters. Designers balance hyphenation against justification to produce even texture across the paragraph.";

// ---------- run editing ----------

#[test]
fn range_style_splits_runs() {
    let mut runs = vec![run("Hello brave world", 12.0)];
    edit::style_range(&mut runs, 6, 11, |s| s.size = 24.0);
    assert_eq!(runs.len(), 3);
    assert_eq!((runs[0].text.as_str(), runs[1].text.as_str(), runs[2].text.as_str()), ("Hello ", "brave", " world"));
    assert_eq!((runs[0].style.size, runs[1].style.size, runs[2].style.size), (12.0, 24.0, 12.0));
    // Restyling back merges the runs again.
    edit::style_range(&mut runs, 6, 11, |s| s.size = 12.0);
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].text, "Hello brave world");
}

#[test]
fn range_style_across_run_boundaries() {
    let mut runs = vec![run("aaa", 10.0), run("bbb", 20.0), run("ccc", 30.0)];
    edit::style_range(&mut runs, 2, 7, |s| s.tracking = 50.0);
    let sizes: Vec<(String, f64, f64)> = runs.iter().map(|r| (r.text.clone(), r.style.size, r.style.tracking)).collect();
    assert_eq!(
        sizes,
        vec![("aa".into(), 10.0, 0.0), ("a".into(), 10.0, 50.0), ("bbb".into(), 20.0, 50.0), ("c".into(), 30.0, 50.0), ("cc".into(), 30.0, 0.0)]
    );
    // Reversed and out-of-range offsets are clamped.
    edit::style_range(&mut runs, 100, 0, |s| s.tracking = 0.0);
    assert!(runs.iter().all(|r| r.style.tracking == 0.0));
    assert_eq!(runs.len(), 3);
}

#[test]
fn replace_range_takes_replaced_style_and_normalizes() {
    let mut runs = vec![run("Hello ", 12.0), run("brave", 24.0), run(" world", 12.0)];
    // Typing over the big word keeps its size.
    let caret = edit::replace_range(&mut runs, 6, 11, "bold");
    assert_eq!(caret, 10);
    assert_eq!(runs[1].text, "bold");
    assert_eq!(runs[1].style.size, 24.0);
    // Inserting at a run end continues the preceding run.
    let caret = edit::replace_range(&mut runs, 10, 10, "er");
    assert_eq!((caret, runs[1].text.as_str()), (12, "bolder"));
    // Deleting a whole run merges its neighbours.
    edit::replace_range(&mut runs, 6, 12, "");
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].text, "Hello  world");
    // Deleting everything keeps one empty run with the style.
    edit::replace_range(&mut runs, 0, 100, "");
    assert_eq!(runs.len(), 1);
    assert!(runs[0].text.is_empty());
    assert_eq!(runs[0].style.size, 12.0);
}

#[test]
fn replace_respects_char_boundaries_and_slices() {
    let mut runs = vec![run("héllo", 12.0)];
    // Offset 2 is inside 'é' (2 bytes): clamped back to the boundary.
    edit::replace_range(&mut runs, 2, 3, "E");
    assert_eq!(runs[0].text, "hEllo");
    let mut runs = vec![run("ab", 10.0), run("cd", 20.0)];
    let s = edit::slice_runs(&runs, 1, 3);
    assert_eq!(s.len(), 2);
    assert_eq!((s[0].text.as_str(), s[1].text.as_str()), ("b", "c"));
    let caret = edit::replace_range_styled(&mut runs, 4, 4, &s);
    assert_eq!(caret, 6);
    assert_eq!(runs.iter().map(|r| r.text.as_str()).collect::<String>(), "abcdbc");
}

#[test]
fn word_and_paragraph_navigation() {
    let s = "Hello, big world\nNext line";
    assert_eq!(edit::next_word(s, 0), 5);
    assert_eq!(edit::next_word(s, 5), 10);
    assert_eq!(edit::prev_word(s, 10), 7);
    assert_eq!(edit::prev_word(s, 7), 0);
    assert_eq!(edit::word_at(s, 8), 7..10);
    assert_eq!(edit::word_at(s, 10), 7..10, "at a word end");
    assert_eq!(edit::word_at(s, 5), 0..5, "word end wins over punctuation");
    assert_eq!(edit::word_at("x ,y", 2), 2..3, "punctuation alone");
    assert_eq!(edit::word_at(s, 6), 6..7, "spaces");
    assert_eq!(edit::paragraph_at(s, 3), 0..16);
    assert_eq!(edit::paragraph_at(s, 20), 17..26);
    assert_eq!(edit::next_char(s, 0), 1);
    assert_eq!(edit::prev_char("é", 2), 0);
    assert_eq!(edit::next_word(s, s.len()), s.len());
}

// ---------- caret navigation / selection ----------

#[test]
fn caret_up_down_keeps_column() {
    let t = area(COPY, style(12.0), Rect::new(0.0, 0.0, 200.0, 600.0), Justify::Left);
    let l = layout(db(), &t);
    assert!(l.lines.len() > 5);
    let b = l.lines[2].start + 6;
    let (top, _) = caret_position(&l, b);
    let down = caret_vertical(&l, b, 1, top.x);
    assert_eq!(l.line_of(down), 3);
    let (t2, _) = caret_position(&l, down);
    assert!((t2.x - top.x).abs() < 12.0, "{} vs {}", t2.x, top.x);
    let up = caret_vertical(&l, down, -1, top.x);
    assert_eq!(up, b);
    assert_eq!(caret_vertical(&l, b, -10, top.x), 0);
    assert_eq!(caret_vertical(&l, b, 100, top.x), COPY.len());
}

#[test]
fn home_end_on_wrapped_lines() {
    let t = area(COPY, style(12.0), Rect::new(0.0, 0.0, 200.0, 600.0), Justify::Left);
    let l = layout(db(), &t);
    let b = l.lines[1].start + 3;
    assert_eq!(line_home(&l, b), l.lines[1].start);
    let e = line_end_of(&l, b);
    // Before the trailing space: the caret stays on line 1.
    assert_eq!(l.line_of(e), 1);
    assert_eq!(&COPY[e..e + 1], " ");
}

#[test]
fn selection_quads_cover_lines() {
    let t = TextObject::point(Point::ZERO, "One two\nThree four", style(20.0));
    let l = layout(db(), &t);
    let q = selection_quads(&l, 4, 7);
    assert_eq!(q.len(), 1);
    let (a, _) = caret_position(&l, 4);
    let (b, _) = caret_position(&l, 7);
    assert!((q[0][0].x - a.x).abs() < 1e-6 && (q[0][1].x - b.x).abs() < 1e-6);
    // Across the paragraph break: two quads.
    let q = selection_quads(&l, 4, 12);
    assert_eq!(q.len(), 2);
    assert!(q[1][0].y > q[0][0].y);
    assert!(selection_quads(&l, 3, 3).is_empty());
}

#[test]
fn hit_testing_picks_the_right_column() {
    let text: String = COPY.repeat(3);
    let opts = LayoutOptions { columns: 2, gutter: 20.0, ..Default::default() };
    let t = area(&text, style(12.0), Rect::new(0.0, 0.0, 420.0, 200.0), Justify::Left);
    let l = layout_with(db(), &t, &opts);
    assert_eq!(l.frames.len(), 2);
    let right: Vec<&LineInfo> = l.lines.iter().filter(|li| li.x0 >= 220.0 - 1e-6).collect();
    assert!(!right.is_empty(), "text flowed into the second column");
    let r0 = right[0];
    let b = hit_byte(&l, Point::new(r0.x0 + 1.0, r0.baseline));
    assert!(b >= r0.start && b <= r0.end, "{b} not in {}..{}", r0.start, r0.end);
    for li in &l.lines {
        assert!((li.x0 >= -1e-6 && li.x1 <= 200.0 + 1e-6) || (li.x0 >= 220.0 - 1e-6 && li.x1 <= 420.0 + 1e-6), "{li:?}");
    }
}

// ---------- area flow ----------

#[test]
fn area_flow_inside_circle_keeps_glyphs_inside() {
    let circle = Circle::new((150.0, 150.0), 120.0).to_path(0.1);
    let t = area_path(COPY, style(11.0), &circle, Justify::Left);
    let l = layout(db(), &t);
    assert!(l.lines.len() > 8);
    let inside = |p: Point| (p - Point::new(150.0, 150.0)).hypot() <= 120.0 + 0.5;
    for g in &l.glyphs {
        let li = &l.lines[g.line];
        for p in [
            Point::new(g.origin.x, li.baseline - li.ascent),
            Point::new(g.origin.x + g.advance, li.baseline - li.ascent),
            Point::new(g.origin.x, li.baseline + li.descent),
            Point::new(g.origin.x + g.advance, li.baseline + li.descent),
        ] {
            if g.outline.elements().is_empty() {
                continue;
            }
            assert!(inside(p), "glyph box corner {p:?} outside the circle (line {})", g.line);
        }
    }
    // Lines near the middle are wider than lines near the top.
    let w = |i: usize| l.lines[i].avail.1 - l.lines[i].avail.0;
    assert!(w(l.lines.len() / 2) > w(0));
}

#[test]
fn justify_all_line_widths_equal() {
    let t = area(COPY, style(12.0), Rect::new(0.0, 0.0, 240.0, 800.0), Justify::JustifyAll);
    let l = layout(db(), &t);
    assert!(l.lines.len() > 4);
    for li in &l.lines {
        assert!((li.x1 - li.x0 - 240.0).abs() < 1e-6, "{li:?}");
    }
}

#[test]
fn composer_lines_never_exceed_frame() {
    for (hy, composer) in [(false, Composer::EveryLine), (true, Composer::EveryLine), (false, Composer::SingleLine), (true, Composer::SingleLine)] {
        let mut t = area(&COPY.repeat(2), style(12.0), Rect::new(0.0, 0.0, 180.0, 2000.0), Justify::JustifyLeft);
        t.para.hyphenate = hy;
        let l = layout_with(db(), &t, &LayoutOptions { composer, ..Default::default() });
        assert!(l.lines.len() > 10);
        for (i, li) in l.lines.iter().enumerate() {
            assert!(li.x1 - li.x0 <= 180.0 + 1e-6, "{composer:?} hy={hy} line {i} too wide: {li:?}");
            assert!(li.x0 >= -1e-6);
            if i + 1 < l.lines.len() {
                assert!((li.x1 - li.x0 - 180.0).abs() < 1e-6, "{composer:?} hy={hy} justified line {i}: {li:?}");
            }
        }
        let clusters: std::collections::BTreeSet<(usize, usize)> = l.glyphs.iter().filter(|g| g.len > 0).map(|g| (g.byte, g.len)).collect();
        assert_eq!(clusters.iter().map(|c| c.1).sum::<usize>(), COPY.len() * 2, "every character placed");
    }
}

/// Every-line composition spreads the spacing more evenly than greedy breaking.
#[test]
fn every_line_composer_is_more_even() {
    let t = area(&COPY.repeat(2), style(12.0), Rect::new(0.0, 0.0, 190.0, 2000.0), Justify::JustifyLeft);
    let spread = |c: Composer| {
        let l = layout_with(db(), &t, &LayoutOptions { composer: c, ..Default::default() });
        // Worst word-space stretch (space glyph advance) over all justified lines.
        let mut worst: f64 = 0.0;
        for li in &l.lines[..l.lines.len() - 1] {
            for g in &l.glyphs[li.glyph_start..li.glyph_end] {
                if COPY.as_bytes().get(g.byte % COPY.len()) == Some(&b' ') {
                    worst = worst.max(g.advance);
                }
            }
        }
        worst
    };
    let (kp, greedy) = (spread(Composer::EveryLine), spread(Composer::SingleLine));
    assert!(kp <= greedy + 1e-6, "every-line worst space {kp} vs greedy {greedy}");
}

#[test]
fn hyphenation_rules() {
    assert_eq!(hyphen::hyphenate_word("typography"), "typo-gra-phy");
    assert_eq!(hyphen::hyphenate_word("happen"), "hap-pen");
    assert_eq!(hyphen::hyphenate_word("hyphenation"), "hyphe-na-tion");
    assert!(hyphen::hyphen_points("short").is_empty());
    assert!(hyphen::hyphen_points("NASAJPL").is_empty(), "all caps stay whole");
    assert!(hyphen::hyphen_points("abc123def").is_empty());
    for w in ["justification", "arrangement", "paragraph", "legible"] {
        let h = hyphen::hyphenate_word(w);
        for part in h.split('-') {
            assert!(part.chars().count() >= 2, "{h}");
        }
        let pts = hyphen::hyphen_points(w);
        assert!(pts.iter().all(|&p| p >= hyphen::MIN_BEFORE && p <= w.chars().count() - hyphen::MIN_AFTER), "{w}: {pts:?}");
    }
}

#[test]
fn hyphenated_layout_adds_hyphens() {
    let text = "Extraordinarily comprehensive typographical considerations notwithstanding everything.";
    let mut t = area(text, style(14.0), Rect::new(0.0, 0.0, 120.0, 600.0), Justify::Left);
    let plain = layout(db(), &t);
    t.para.hyphenate = true;
    let hy = layout(db(), &t);
    let hyphens = |l: &TextLayout| l.glyphs.iter().filter(|g| g.len == 0).count();
    assert_eq!(hyphens(&plain), 0);
    assert!(hyphens(&hy) > 0, "some words hyphenated");
    for li in &hy.lines {
        assert!(li.x1 - li.x0 <= 120.0 + 1e-6, "{li:?}");
    }
    // Hyphenation fills lines better: no more lines than without it.
    assert!(hy.lines.len() <= plain.lines.len());
    // The hyphen glyph is the last glyph of its line and has an outline.
    for g in hy.glyphs.iter().filter(|g| g.len == 0) {
        let li = &hy.lines[g.line];
        assert_eq!(li.glyph_end - 1, hy.glyphs.iter().position(|x| std::ptr::eq(x, g)).unwrap());
        assert!(!g.outline.elements().is_empty());
    }
}

#[test]
fn soft_hyphen_is_invisible_mid_line() {
    let l = layout(db(), &TextObject::point(Point::ZERO, "co\u{00AD}operate", style(20.0)));
    let sh = l.glyphs.iter().find(|g| g.byte == 2).unwrap();
    assert!(sh.outline.elements().is_empty());
    assert_eq!(sh.advance, 0.0);
}

// ---------- area type options ----------

#[test]
fn inset_and_first_baseline_options() {
    let frame = Rect::new(0.0, 0.0, 300.0, 300.0);
    let t = area(COPY, style(12.0), frame, Justify::Left);
    let base = layout(db(), &t);
    let inset = layout_with(db(), &t, &LayoutOptions { inset: 10.0, ..Default::default() });
    for li in &inset.lines {
        assert!(li.x0 >= 10.0 - 1e-6 && li.x1 <= 290.0 + 1e-6);
    }
    assert!((inset.lines[0].baseline - base.lines[0].baseline - 10.0).abs() < 1e-6);
    let lead = layout_with(db(), &t, &LayoutOptions { first_baseline: FirstBaseline::Leading, ..Default::default() });
    assert!((lead.lines[0].baseline - 14.4).abs() < 1e-6, "{}", lead.lines[0].baseline);
    let fixed = layout_with(db(), &t, &LayoutOptions { first_baseline: FirstBaseline::Fixed, first_baseline_min: 30.0, ..Default::default() });
    assert!((fixed.lines[0].baseline - 30.0).abs() < 1e-6);
    let cap = layout_with(db(), &t, &LayoutOptions { first_baseline: FirstBaseline::CapHeight, ..Default::default() });
    assert!(cap.lines[0].baseline < base.lines[0].baseline && cap.lines[0].baseline > 5.0);
}

#[test]
fn rows_and_columns_flow_in_order() {
    let text = COPY.repeat(2);
    let t = area(&text, style(12.0), Rect::new(0.0, 0.0, 400.0, 300.0), Justify::Left);
    let l = layout_with(db(), &t, &LayoutOptions { rows: 2, columns: 2, gutter: 10.0, ..Default::default() });
    assert_eq!(l.frames.len(), 4);
    // Text order: column 1 (top then bottom cell), then column 2.
    let first_col2 = l.lines.iter().position(|li| li.x0 > 200.0).expect("reaches column 2");
    assert!(l.lines[..first_col2].iter().any(|li| li.baseline > 155.0), "fills the lower cell of column 1 first");
    for w in l.lines.windows(2) {
        assert!(w[1].start >= w[0].start);
    }
}

// ---------- OpenType ----------

#[test]
fn opentype_ligature_switches() {
    let serif = CharStyle { font_family: "Source Serif 4".into(), ..style(20.0) };
    let t = TextObject::point(Point::ZERO, "fi", serif.clone());
    assert_eq!(layout(db(), &t).glyphs.len(), 1, "fi ligature by default");
    let off = LayoutOptions { features: OtFeatures { ligatures: false, ..Default::default() }, ..Default::default() };
    assert_eq!(layout_with(db(), &t, &off).glyphs.len(), 2);
    // Tracking suppresses ligatures (as letterspaced type should).
    let tracked = TextObject::point(Point::ZERO, "fi", CharStyle { tracking: 100.0, ..serif });
    assert_eq!(layout(db(), &tracked).glyphs.len(), 2);
    let f = OtFeatures::from_tags(["dlig", "-liga", "smcp", "onum", "bogus"]);
    assert!(f.discretionary_ligatures && !f.ligatures && f.small_caps && f.oldstyle_figures && !f.fractions);
}

#[test]
fn face_charmap_for_glyphs_panel() {
    let f = db().face("Source Sans 3", "Regular").unwrap();
    let chars = f.chars();
    assert!(chars.len() > 200, "{}", chars.len());
    assert!(chars.windows(2).all(|w| w[0].0 < w[1].0));
    let a = f.glyph_for('A');
    assert!(a != 0 && chars.iter().any(|&(c, g)| c == 'A' && g == a));
    assert!(f.advance(a) > 0.0);
    assert!(!db().outline(&f, a).elements().is_empty());
    assert_eq!(f.units_per_em(), 1000.0);
}

#[test]
fn uncovered_characters_do_not_break_layout() {
    // Private-use code point: no font covers it (system fallback is tried at most once and
    // remembered); layout still places a .notdef-width glyph.
    let l = layout(db(), &TextObject::point(Point::ZERO, "a\u{F8FF}\u{E000}b", style(12.0)));
    assert_eq!(l.lines.len(), 1);
    assert!(l.glyphs.len() >= 3);
}

// ---------- performance ----------

#[test]
fn layout_10k_area_text_is_fast() {
    let text: String = COPY.chars().cycle().take(10_000).collect();
    let mut t = area(&text, style(10.0), Rect::new(0.0, 0.0, 400.0, 20_000.0), Justify::JustifyLeft);
    t.para.hyphenate = true;
    let _ = layout(db(), &t);
    let n = 5;
    let start = std::time::Instant::now();
    for _ in 0..n {
        let l = layout(db(), &t);
        assert!(!l.overflow);
    }
    let per = start.elapsed().as_secs_f64() * 1000.0 / n as f64;
    eprintln!("layout of 10k chars (justified, hyphenated): {per:.3} ms");
    let budget = if cfg!(debug_assertions) { 400.0 } else { 10.0 };
    assert!(per < budget, "{per} ms");
}
