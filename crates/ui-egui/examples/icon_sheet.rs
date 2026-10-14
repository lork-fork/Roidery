//! The icon sheet: every interface icon (`assets/icons`) on its own artboard, labelled with what
//! it does and its file name, for editing the icons in Roidery and writing them back.
//!
//! ```text
//! cargo run -p vectorcraft-ui-egui --example icon_sheet -- make roidery-icons.svg
//! cargo run -p vectorcraft-ui-egui --example icon_sheet -- apply roidery-icons.svg [--dry-run]
//! ```
//!
//! `make` writes an SVG that opens in Roidery as a document with one 96 × 96 pt artboard per icon
//! (4× the icons' 24-unit grid, so the usual 2-unit stroke is 8 pt), named after the icon's file.
//! The labels sit on the locked Labels layer and are never written back. The SVG carries the
//! native document (Preserve Editing), so other apps show the whole sheet and Roidery reopens it
//! with its artboards; a `.vectorcraft` output path writes the native file instead.
//!
//! `apply` reads the edited sheet (SVG saved by Roidery, `.vectorcraft`, or an SVG edited in
//! another app, matched to the artboards by position), takes the art on each artboard, scales it
//! back to 24 units and writes `assets/icons/<artboard name>.svg` for every icon that changed.
//! Black paint becomes `currentColor` (the app tints icons with the theme), stroke weights scale
//! with the art, and a Lucide icon keeps its licence header. An artboard named after no existing
//! icon makes a new icon file (add it to `crates/ui-egui/src/icon_data.rs` and `ASSETS.md`).
// A developer tool run by hand: a failure stops it with a message.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::type_complexity)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use vectorcraft_doc::{Artboard, Document, Node, NodeKind, Scaling};
use vectorcraft_geom::{Affine, Point, Rect, Vec2};

const LABELS: &str = include_str!("icon_sheet/labels.tsv");
/// Sheet points per icon unit.
const SCALE: f64 = 4.0;
/// Icon grid (units) and artboard size (points).
const GRID: f64 = 24.0;
const BOARD: f64 = GRID * SCALE;
const COLS: usize = 10;
const CELL_W: f64 = 210.0;
const LABEL_H: f64 = 70.0;
const CELL_H: f64 = LABEL_H + BOARD + 34.0;
const HEADING_H: f64 = 56.0;
const MARGIN: f64 = 40.0;
const LABEL_LAYER: &str = "Labels";
const ICON_LAYER: &str = "Icons";
const GROUPS: [&str; 6] = ["", "Toolbar tools", "Panels", "Buttons in panels", "App window and dialogs", "Not used in the app yet"];

struct Icon {
    name: String,
    group: usize,
    label: String,
}

fn icons_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/icons")
}

/// The icons in sheet order: labelled ones in the table's order, then any the table misses.
fn icons() -> Vec<Icon> {
    let mut files: Vec<String> = std::fs::read_dir(icons_dir())
        .expect("assets/icons")
        .filter_map(|e| e.ok()?.file_name().to_str()?.strip_suffix(".svg").map(str::to_string))
        .collect();
    files.sort();
    let mut out = Vec::new();
    for line in LABELS.lines().filter(|l| !l.starts_with('#') && !l.trim().is_empty()) {
        let mut f = line.splitn(3, '\t');
        let (Some(g), Some(n), Some(l)) = (f.next(), f.next(), f.next()) else { continue };
        if files.iter().any(|x| x == n) {
            out.push(Icon { name: n.into(), group: g.parse().unwrap_or(5), label: l.into() });
        }
    }
    for n in files {
        if !out.iter().any(|i| i.name == n) {
            out.push(Icon { name: n, group: 5, label: "No label yet".into() });
        }
    }
    out.sort_by_key(|i| i.group);
    out
}

/// Each icon's artboard, and the section headings' positions.
fn layout(icons: &[Icon]) -> (Vec<(String, Rect)>, Vec<(usize, f64)>, Rect) {
    let (mut boards, mut heads) = (Vec::new(), Vec::new());
    let mut y = MARGIN;
    let mut i = 0;
    while i < icons.len() {
        let g = icons[i].group;
        heads.push((g, y + 30.0));
        y += HEADING_H;
        let mut col = 0;
        while i < icons.len() && icons[i].group == g {
            if col == COLS {
                col = 0;
                y += CELL_H;
            }
            let x = MARGIN + col as f64 * CELL_W + (CELL_W - BOARD) / 2.0;
            boards.push((icons[i].name.clone(), Rect::new(x, y + LABEL_H, x + BOARD, y + LABEL_H + BOARD)));
            col += 1;
            i += 1;
        }
        y += CELL_H;
    }
    (boards, heads, Rect::new(0.0, 0.0, 2.0 * MARGIN + COLS as f64 * CELL_W, y + MARGIN))
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// `s` broken into lines of at most `w` characters (at spaces).
fn wrap(s: &str, w: usize) -> Vec<String> {
    let mut lines: Vec<String> = vec![];
    for word in s.split(' ') {
        match lines.last_mut() {
            Some(l) if l.chars().count() + 1 + word.chars().count() <= w => {
                l.push(' ');
                l.push_str(word);
            }
            _ => lines.push(word.to_string()),
        }
    }
    lines
}

/// An icon's ink (`currentColor`, which the app draws white before tinting) as black for the sheet.
fn ink_black(s: &str) -> String {
    ["currentColor", "white", "#ffffff", "#fff", "#FFFFFF", "#FFF"].iter().fold(s.to_string(), |s, c| s.replace(&format!("\"{c}\""), "\"#000000\""))
}

/// The `<svg>` element's content of an icon file, painted black for the sheet.
fn icon_body(svg: &str) -> String {
    let start = svg.find("<svg").and_then(|s| svg[s..].find('>').map(|e| s + e + 1)).expect("icon <svg>");
    let end = svg.rfind("</svg>").expect("icon </svg>");
    ink_black(&svg[start..end])
}

/// The root attributes icons paint with (Lucide's): inherited by the icon's elements.
fn icon_root_paint(svg: &str) -> String {
    let start = svg.find("<svg").unwrap_or(0);
    let head = &svg[start..svg[start..].find('>').map_or(svg.len(), |e| start + e)];
    let mut out = String::new();
    for a in ["fill", "stroke", "stroke-width", "stroke-linecap", "stroke-linejoin"] {
        let key = format!("{a}=\"");
        if let Some(p) = head.find(&format!(" {key}")).or_else(|| head.find(&format!("\n{key}"))) {
            let v = &head[p + 1 + key.len()..];
            let v = &v[..v.find('"').unwrap_or(0)];
            out += &ink_black(&format!(" {a}=\"{v}\""));
        }
    }
    out
}

/// The sheet as plain SVG (before Roidery's import turns it into a document).
fn sheet_svg(icons: &[Icon]) -> String {
    let (boards, heads, page) = layout(icons);
    let mut labels = String::new();
    for (g, y) in &heads {
        labels += &format!(
            "<text x=\"{MARGIN}\" y=\"{y}\" font-family=\"Source Sans 3\" font-weight=\"600\" font-size=\"22\" fill=\"#1a1a1a\">{}</text>\n",
            esc(GROUPS[*g])
        );
    }
    let mut art = String::new();
    for (icon, (_, r)) in icons.iter().zip(&boards) {
        let x = r.x0 - (CELL_W - BOARD) / 2.0 + 8.0;
        let lines = wrap(&icon.label, 38);
        let mut y = r.y0 - 14.0 - 12.0 * lines.len().min(4) as f64;
        for l in lines.iter().take(4) {
            labels += &format!("<text x=\"{x}\" y=\"{y}\" font-family=\"Source Sans 3\" font-size=\"10.5\" fill=\"#1a1a1a\">{}</text>\n", esc(l));
            y += 12.0;
        }
        labels += &format!(
            "<text x=\"{x}\" y=\"{}\" font-family=\"JetBrains Mono\" font-size=\"8.5\" fill=\"#8a6d00\">{}.svg</text>\n",
            r.y0 - 6.0,
            esc(&icon.name)
        );
        let src = std::fs::read_to_string(icons_dir().join(format!("{}.svg", icon.name))).expect("icon file");
        art += &format!(
            "<g id=\"icon-{}\" transform=\"translate({} {}) scale({SCALE})\"{}>{}</g>\n",
            esc(&icon.name),
            r.x0,
            r.y0,
            icon_root_paint(&src),
            icon_body(&src)
        );
    }
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\">\n<g id=\"{LABEL_LAYER}\">\n{labels}</g>\n<g id=\"{ICON_LAYER}\">\n{art}</g>\n</svg>\n",
        w = page.width(),
        h = page.height()
    )
}

/// The sheet document: the SVG imported, one artboard per icon, labels locked.
fn sheet_doc(icons: &[Icon]) -> Document {
    let (mut doc, warnings) = vectorcraft_svg::import_with_report(&sheet_svg(icons)).expect("sheet SVG");
    for w in warnings {
        eprintln!("import: {w}");
    }
    let (boards, _, _) = layout(icons);
    doc.artboards = boards
        .into_iter()
        .enumerate()
        .map(|(i, (name, rect))| Artboard { id: i as u32 + 1, name, rect, show_center_mark: false, show_cross_hairs: false })
        .collect();
    for l in &mut doc.layers {
        if l.name.as_deref() == Some(LABEL_LAYER) {
            Arc::make_mut(l).locked = true;
        }
    }
    doc.title = "Roidery icons".into();
    doc
}

/// The art on artboard `r` (objects of the layers other than Labels whose bounds overlap it; a
/// group reaching well past it, such as a layer that came in as a group, is looked into), moved
/// and scaled to the icon's 24-unit grid.
fn art_on(doc: &Document, r: Rect) -> Vec<Arc<Node>> {
    let to_icon = Affine::scale(1.0 / SCALE) * Affine::translate(-Vec2::new(r.x0, r.y0));
    let near = r.inflate(BOARD / 2.0, BOARD / 2.0);
    let mut out = Vec::new();
    let mut stack: Vec<&Arc<Node>> = doc.layers.iter().rev().collect();
    while let Some(n) = stack.pop() {
        if !n.visible || n.name.as_deref() == Some(LABEL_LAYER) || matches!(n.kind, NodeKind::Text(_)) {
            continue;
        }
        let Some(b) = n.visual_bounds() else { continue };
        let i = b.intersect(r);
        if !(i.width() > 0.0 && i.height() > 0.0 || (b.width() == 0.0 || b.height() == 0.0) && r.contains(b.center())) {
            continue;
        }
        let whole = !n.is_layer() && near.contains(Point::new(b.x0, b.y0)) && near.contains(Point::new(b.x1, b.y1));
        let looked_into = !whole && (n.is_layer() || matches!(n.kind, NodeKind::Group { clip: false, .. }));
        if looked_into {
            stack.extend(n.children().into_iter().flatten().rev());
        } else {
            let mut m = (**n).clone();
            m.transform(to_icon, Scaling { strokes: true, ..Scaling::default() });
            out.push(Arc::new(m));
        }
    }
    out
}

/// Icon `art` as an icon file: 24 × 24, black as `currentColor`, no document chrome.
fn icon_file(doc: &Document, art: Vec<Arc<Node>>) -> String {
    let mut d = doc.clone();
    let mut layer = Node::layer(d.alloc_id(), ICON_LAYER, vectorcraft_doc::LayerColor::Preset(0));
    if let NodeKind::Layer { children, .. } = &mut layer.kind {
        *children = art;
    }
    d.layers = vec![Arc::new(layer)];
    d.artboards =
        vec![Artboard { id: 1, name: "icon".into(), rect: Rect::new(0.0, 0.0, GRID, GRID), show_center_mark: false, show_cross_hairs: false }];
    let opts = vectorcraft_svg::ExportOptions { object_ids: vectorcraft_svg::ObjectIds::Minimal, ..Default::default() };
    normalize(&vectorcraft_svg::export(&d, &opts))
}

/// Roidery's SVG export of an icon in the icon files' shape.
fn normalize(svg: &str) -> String {
    let body_start = svg.find("<svg").and_then(|s| svg[s..].find('>').map(|e| s + e + 1)).unwrap_or(0);
    let body_end = svg.rfind("</svg>").unwrap_or(svg.len());
    let mut body = svg[body_start..body_end].to_string();
    for (from, to) in [("#000000", "currentColor"), ("#000", "currentColor"), ("rgb(0,0,0)", "currentColor"), ("black", "currentColor")] {
        body = body.replace(&format!("\"{from}\""), "\"currentColor\"").replace(&format!(":{from}"), &format!(":{to}"));
    }
    // The layer group Roidery writes around the art.
    let body = body.trim();
    let body = match (body.strip_prefix(&format!("<g id=\"{ICON_LAYER}\">")), body.ends_with("</g>")) {
        (Some(b), true) => b[..b.len() - 4].trim().to_string(),
        _ => body.to_string(),
    };
    // Groups without attributes (the layer, the icon's group) are dropped; so are the title and
    // metadata.
    let mut kept = Vec::new();
    let mut open = Vec::new();
    for l in body.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if l.starts_with("<title") || l.starts_with("<desc") || l.starts_with("<metadata") {
            continue;
        }
        if l == "<g>" {
            open.push(false);
            continue;
        }
        if l.starts_with("<g ") && !l.ends_with("/>") {
            open.push(true);
        }
        if l == "</g>" && !open.pop().unwrap_or(true) {
            continue;
        }
        kept.push(l);
    }
    let lines = kept;
    format!(
        "<svg\n  xmlns=\"http://www.w3.org/2000/svg\"\n  width=\"24\"\n  height=\"24\"\n  viewBox=\"0 0 24 24\"\n>\n  {}\n</svg>\n",
        lines.join("\n  ")
    )
}

/// The numbers and the rest of `s`, for comparing two exports up to rounding.
fn same_up_to_rounding(a: &str, b: &str) -> bool {
    fn tokens(s: &str) -> Vec<Result<f64, String>> {
        let mut out = Vec::new();
        let mut cur = String::new();
        let mut num = false;
        for c in s.chars() {
            let is_num = c.is_ascii_digit() || c == '.' || (c == '-' && !num);
            if is_num != num || (c == '-' && num) {
                if !cur.is_empty() {
                    out.push(if num { cur.parse::<f64>().map_err(|_| cur.clone()) } else { Err(cur.clone()) });
                }
                cur.clear();
                num = is_num;
            }
            if !c.is_whitespace() {
                cur.push(c);
            }
        }
        if !cur.is_empty() {
            out.push(if num { cur.parse::<f64>().map_err(|_| cur.clone()) } else { Err(cur) });
        }
        out
    }
    let (ta, tb) = (tokens(a), tokens(b));
    ta.len() == tb.len()
        && ta.iter().zip(&tb).all(|(x, y)| match (x, y) {
            (Ok(p), Ok(q)) => (p - q).abs() < 0.01,
            (p, q) => p == q,
        })
}

/// The edited sheet as a document with the icons' artboards.
fn read_sheet(path: &Path, icons: &[Icon]) -> Document {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    if vectorcraft_format::sniff(&bytes) {
        return vectorcraft_format::load(&bytes).expect("Roidery document");
    }
    let text = vectorcraft_svg::text_of(&bytes).expect("SVG");
    if let Some(e) = vectorcraft_svg::editing(&text) {
        if e.intact {
            let native = vectorcraft_format::base64_decode(&e.data).expect("editing data");
            return vectorcraft_format::load(&native).expect("Roidery document in the SVG");
        }
        eprintln!("The SVG was edited in another app since Roidery saved it: reading its art, artboards by position.");
    }
    // Plain SVG (or edited elsewhere): its art, on the sheet's artboards. `make` wrote the art
    // with the top left of its bounds at 0, 0.
    let (mut doc, _) = vectorcraft_svg::import_with_report(&text).expect("SVG");
    doc.artboards = sheet_doc(icons).artboards;
    let origin = sheet_doc(icons).art_bounds().map_or(Vec2::ZERO, |b| Vec2::new(b.x0, b.y0));
    for a in &mut doc.artboards {
        a.rect = a.rect - origin;
    }
    doc
}

fn apply(path: &Path, dry_run: bool) {
    let icons = icons();
    let edited = read_sheet(path, &icons);
    let original = sheet_doc(&icons);
    let before: BTreeMap<&str, Rect> = original.artboards.iter().map(|a| (a.name.as_str(), a.rect)).collect();
    let (mut changed, mut added, mut empty) = (vec![], vec![], vec![]);
    for ab in &edited.artboards {
        let name = ab.name.trim();
        if name.is_empty() || name.contains(['/', '\\']) {
            continue;
        }
        let art = art_on(&edited, ab.rect);
        if art.is_empty() {
            empty.push(name.to_string());
            continue;
        }
        let new = icon_file(&edited, art);
        let file = icons_dir().join(format!("{name}.svg"));
        let header = match before.get(name) {
            Some(r) => {
                let was = icon_file(&original, art_on(&original, *r));
                if same_up_to_rounding(&was, &new) {
                    continue;
                }
                if std::env::var("ICON_SHEET_DEBUG").is_ok_and(|v| v == name) {
                    eprintln!("--- before\n{was}--- after\n{new}");
                }
                changed.push(name.to_string());
                let old = std::fs::read_to_string(&file).unwrap_or_default();
                old.lines().next().filter(|l| l.starts_with("<!--")).map(|l| format!("{l}\n")).unwrap_or_default()
            }
            None => {
                added.push(name.to_string());
                String::new()
            }
        };
        if !dry_run {
            std::fs::write(&file, header + &new).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
        }
    }
    let verb = if dry_run { "would update" } else { "updated" };
    println!("{verb} {} icon(s): {}", changed.len(), changed.join(", "));
    if !added.is_empty() {
        println!(
            "new icon(s) {}: {} (add them to crates/ui-egui/src/icon_data.rs and ASSETS.md)",
            if dry_run { "found" } else { "written" },
            added.join(", ")
        );
    }
    if !empty.is_empty() {
        println!("left alone, nothing on the artboard: {}", empty.join(", "));
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["make", out] => {
            let icons = icons();
            let doc = sheet_doc(&icons);
            let native = vectorcraft_format::save(&doc, false);
            let bytes = if out.ends_with(".vectorcraft") {
                native
            } else {
                let opts = vectorcraft_svg::ExportOptions { artboard: None, preserve_editing: true, ..Default::default() };
                vectorcraft_svg::export_full(&doc, &opts, Some(&native)).svg.into_bytes()
            };
            std::fs::write(out, bytes).expect("write the sheet");
            println!("{} icons on {} artboards → {out}", icons.len(), doc.artboards.len());
        }
        ["apply", file] => apply(Path::new(file), false),
        ["apply", file, "--dry-run"] => apply(Path::new(file), true),
        _ => {
            eprintln!("usage: icon_sheet make <out.svg|out.vectorcraft> | apply <edited sheet> [--dry-run]");
            std::process::exit(2);
        }
    }
}
