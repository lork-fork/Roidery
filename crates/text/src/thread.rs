//! Threaded text: one story flowing through several area-type frames in order.
//!
//! Each frame keeps its own slice of the story as ordinary runs, so rendering, export and hit
//! testing need nothing special; after an edit the engine joins the slices and re-distributes them
//! with [`distribute`].

use vectorcraft_doc::{TextObject, TextRun};

use crate::edit::{normalize, runs_len, slice_runs, style_at};
use crate::{FontDb, layout};

/// The story: every frame's runs joined in thread order.
pub fn story(frames: &[&TextObject]) -> Vec<TextRun> {
    let mut runs: Vec<TextRun> = frames.iter().flat_map(|f| f.runs.iter().cloned()).collect();
    normalize(&mut runs);
    runs
}

/// How many bytes of `runs` fit in `frame` (whole lines; a following break space or paragraph
/// break stays with the line that ends there).
fn fit(db: &FontDb, frame: &TextObject, runs: &[TextRun]) -> usize {
    let mut probe = frame.clone();
    probe.runs = runs.to_vec();
    let lay = layout(db, &probe);
    let text: String = runs.iter().map(|r| r.text.as_str()).collect();
    if !lay.overflow {
        return text.len();
    }
    let mut end = lay.lines.last().map_or(0, |l| l.end);
    let bytes = text.as_bytes();
    while end < bytes.len() && bytes[end] == b' ' {
        end += 1;
    }
    if end < bytes.len() && bytes[end] == b'\n' {
        end += 1;
    }
    end
}

/// The story `runs` split across `frames` in order (the last frame takes any overflow). Empty
/// slices keep one empty run carrying the style at the split, so typing there has a style.
pub fn distribute(db: &FontDb, frames: &[&TextObject], runs: &[TextRun]) -> Vec<Vec<TextRun>> {
    let mut rest = runs.to_vec();
    let mut out = Vec::with_capacity(frames.len());
    for (i, f) in frames.iter().enumerate() {
        let len = runs_len(&rest);
        let n = if i + 1 == frames.len() { len } else { fit(db, f, &rest) };
        let mut head = slice_runs(&rest, 0, n);
        if head.is_empty() {
            head.push(TextRun { text: String::new(), style: style_at(&rest, n) });
        }
        let tail = slice_runs(&rest, n, len);
        rest = if tail.is_empty() { vec![TextRun { text: String::new(), style: style_at(&rest, len) }] } else { tail };
        out.push(head);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use vectorcraft_doc::{CharStyle, TextKind};
    use vectorcraft_geom::{Affine, Rect, shapes};

    fn frame(x: f64) -> TextObject {
        let mut t = TextObject::point(vectorcraft_geom::Point::ZERO, "", CharStyle::default());
        t.kind = TextKind::Area { frame: shapes::rectangle(Rect::new(x, 0.0, x + 120.0, 60.0)) };
        t.xf = Affine::IDENTITY;
        t
    }

    #[test]
    fn story_flows_across_frames_and_rejoins() {
        let db = FontDb::global();
        let (a, b, c) = (frame(0.0), frame(200.0), frame(400.0));
        let text = "The quick brown fox jumps over the lazy dog. ".repeat(6) + "\nSecond paragraph here.";
        let story = vec![TextRun { text: text.clone(), style: CharStyle::default() }];
        let parts = distribute(db, &[&a, &b, &c], &story);
        let lens: Vec<usize> = parts.iter().map(|p| runs_len(p)).collect();
        assert!(lens[0] > 0 && lens[1] > 0, "{lens:?}");
        let joined: String = parts.iter().flatten().map(|r| r.text.as_str()).collect();
        assert_eq!(joined, text, "no character lost or duplicated");
        // Each non-last slice fits its frame.
        for (f, p) in [&a, &b].iter().zip(&parts) {
            let mut t = (*f).clone();
            t.runs = p.clone();
            assert!(!layout(db, &t).overflow);
        }
        // Lines never start with the break space.
        assert!(!parts[1][0].text.starts_with(' '));
    }

    #[test]
    fn short_story_leaves_later_frames_empty_with_a_style() {
        let db = FontDb::global();
        let (a, b) = (frame(0.0), frame(200.0));
        let st = CharStyle { size: 20.0, ..Default::default() };
        let parts = distribute(db, &[&a, &b], &[TextRun { text: "Hi".into(), style: st }]);
        assert_eq!(parts[0][0].text, "Hi");
        assert_eq!((parts[1][0].text.as_str(), parts[1][0].style.size), ("", 20.0));
    }
}
