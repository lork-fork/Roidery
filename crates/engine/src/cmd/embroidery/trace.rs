//! Roidery: Image Trace for embroidery (the Embroidery bar's Image Trace button). The image is
//! traced with one of four of VectorCraft's presets and expanded into paths, white left out so
//! the fabric shows there (the colour presets then trace one colour more, so 3 Color sews three
//! threads), and the paths sew as tatami fills.

use serde_json::{Value, json};

use crate::Session;
use crate::cmd::{Result, bad, str_param};

/// The presets the Embroidery bar offers: (Roidery's name, VectorCraft's preset, the thread
/// colours it sews when it counts them: VectorCraft's colour presets count the white left out).
pub const PRESETS: [(&str, &str, Option<u64>); 4] =
    [("2 Color", "Default", None), ("3 Color", "3 Colors", Some(3)), ("6 Color", "6 Colors", Some(6)), ("Detailed", "Flat Logo", None)];

/// The preset a click on Image Trace uses.
pub const DEFAULT_PRESET: &str = "3 Color";

/// `stitch.traceImage`.
pub(super) fn trace_image(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "stitch.traceImage";
    let name = str_param(p, "preset").unwrap_or(DEFAULT_PRESET);
    let Some((name, preset, threads)) = PRESETS.iter().find(|(n, ..)| n.eq_ignore_ascii_case(name.trim())) else {
        return Err(bad(C, format!("unknown preset `{name}`: use 2 Color, 3 Color, 6 Color or Detailed")));
    };
    let mut q = json!({ "preset": preset, "params": { "ignoreWhite": true } });
    for key in ["id", "ids"] {
        if let Some(v) = p.get(key) {
            q[key] = v.clone();
        }
    }
    in_one_step(s, |s| {
        let mut r = s.execute("imageTrace.make", &q)?;
        // White was one of the preset's colours and is left out: traced again with one more, so
        // "3 Color" sews three threads.
        if let Some(n) = threads
            && r["colors"].as_u64().is_some_and(|c| c < *n)
        {
            let again = json!({ "id": r["id"], "preset": preset, "params": { "ignoreWhite": true, "colors": n + 1 } });
            r = s.execute("imageTrace.make", &again)?;
        }
        s.execute("imageTrace.expand", &json!({ "id": r["id"] }))?;
        // The traced paths sew as tatami.
        s.execute("select.set", &json!({ "ids": [r["id"]] }))?;
        s.execute("stitch.set", &json!({ "type": "tatami" }))?;
        Ok(json!({ "id": r["id"], "paths": r["paths"], "colors": r["colors"], "preset": name }))
    })
}

/// Run `f`'s commands as one undo step (undone together when it fails), unless an undo group is
/// already open (its own step then holds them).
fn in_one_step<T>(s: &mut Session, f: impl FnOnce(&mut Session) -> Result<T>) -> Result<T> {
    let open = s.active().is_some_and(|st| st.undo_group.is_some());
    if !open {
        s.begin_named_undo_group(Some("Image Trace".into()));
    }
    let r = f(s);
    if !open {
        s.end_undo_group(r.is_err());
    }
    r
}
