//! Every-line composer: a Knuth–Plass style total-fit line breaker for justified paragraphs.
//!
//! Boxes are the paragraph's glyphs, glue is its spaces (word spacing may stretch to 150% and
//! shrink to 80%), and penalties are break opportunities after hyphens/dashes/CJK and at
//! hyphenation points. The breaks minimise the sum of squared (badness + penalty) demerits over
//! the whole paragraph, so loose lines are traded for evenly spaced ones.

use crate::shape::SGlyph;

const STRETCH: f64 = 0.5;
const SHRINK: f64 = 0.2;
const HYPHEN_PENALTY: f64 = 50.0;
const DOUBLE_HYPHEN_DEMERITS: f64 = 3000.0;
const LINE_PENALTY: f64 = 10.0;

/// A place a line may end: glyph index `end` (exclusive), plus the width of the hyphen that is
/// added when breaking there (0 = no hyphen).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Breakpoint {
    pub end: usize,
    pub hyphen: f64,
}

struct Node {
    pos: usize,
    line: usize,
    demerits: f64,
    hyphenated: bool,
    prev: Option<usize>,
}

/// Compose the paragraph `g` into lines. `width(k)` is the available width of line `k`;
/// `cands` are the break opportunities in increasing order (the paragraph end is added).
/// `justify_last`: the last line is justified too (Justify All). Returns `(end, hyphen)` per line,
/// or `None` if no set of breaks fits within `tolerance` (maximum stretch ratio).
pub(crate) fn compose(
    g: &[SGlyph],
    width: &dyn Fn(usize) -> f64,
    cands: &[Breakpoint],
    justify_last: bool,
    tolerance: f64,
) -> Option<Vec<(usize, bool)>> {
    let n = g.len();
    if n == 0 {
        return Some(vec![]);
    }
    // Prefix sums of advances and of space advances.
    let mut w = vec![0.0; n + 1];
    let mut sp = vec![0.0; n + 1];
    for (i, gl) in g.iter().enumerate() {
        w[i + 1] = w[i] + gl.adv;
        sp[i + 1] = sp[i] + if gl.is_space() { gl.adv } else { 0.0 };
    }
    // Trailing spaces are dropped at a break.
    let trim = |mut b: usize, a: usize| {
        while b > a && g[b - 1].is_space() {
            b -= 1;
        }
        b
    };
    let mut all: Vec<Breakpoint> = cands.iter().copied().filter(|c| c.end > 0 && c.end < n).collect();
    all.push(Breakpoint { end: n, hyphen: 0.0 });
    let mut nodes = vec![Node { pos: 0, line: 0, demerits: 0.0, hyphenated: false, prev: None }];
    let mut active: Vec<usize> = vec![0];
    for bp in &all {
        let last = bp.end == n;
        let mut best: Vec<(usize, f64, usize)> = vec![]; // (line, demerits, from)
        let mut keep = Vec::with_capacity(active.len());
        for &ai in &active {
            let a = &nodes[ai];
            let t = trim(bp.end, a.pos);
            let natural = w[t] - w[a.pos] + bp.hyphen;
            let spaces = sp[t] - sp[a.pos];
            let lw = width(a.line);
            let ratio = if natural < lw {
                if last && !justify_last {
                    0.0
                } else if spaces > 0.0 {
                    (lw - natural) / (spaces * STRETCH)
                } else {
                    f64::INFINITY
                }
            } else if natural > lw {
                if spaces > 0.0 { (lw - natural) / (spaces * SHRINK) } else { f64::NEG_INFINITY }
            } else {
                0.0
            };
            if ratio < -1.0 {
                // Too long already; longer lines from this node only get worse.
                continue;
            }
            keep.push(ai);
            if ratio > tolerance && !(last && ratio.is_infinite() && justify_last) {
                continue;
            }
            let badness = if ratio.is_finite() { (100.0 * ratio.abs().powi(3)).min(10_000.0) } else { 10_000.0 };
            let penalty = if bp.hyphen > 0.0 { HYPHEN_PENALTY } else { 0.0 };
            let mut d = (LINE_PENALTY + badness).powi(2) + penalty * penalty;
            if bp.hyphen > 0.0 && a.hyphenated {
                d += DOUBLE_HYPHEN_DEMERITS;
            }
            let total = a.demerits + d;
            let line = a.line + 1;
            match best.iter_mut().find(|b| b.0 == line) {
                Some(b) if b.1 <= total => {}
                Some(b) => *b = (line, total, ai),
                None => best.push((line, total, ai)),
            }
        }
        active = keep;
        for (line, demerits, from) in best {
            nodes.push(Node { pos: bp.end, line, demerits, hyphenated: bp.hyphen > 0.0, prev: Some(from) });
            active.push(nodes.len() - 1);
        }
        if active.is_empty() {
            return None;
        }
    }
    let end = nodes.iter().enumerate().filter(|(_, nd)| nd.pos == n && nd.prev.is_some()).min_by(|a, b| a.1.demerits.total_cmp(&b.1.demerits))?.0;
    let mut out = vec![];
    let mut cur = Some(end);
    while let Some(i) = cur {
        if nodes[i].prev.is_some() {
            out.push((nodes[i].pos, nodes[i].hyphenated));
        }
        cur = nodes[i].prev;
    }
    out.reverse();
    Some(out)
}
