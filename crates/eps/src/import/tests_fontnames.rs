//! Type names the available family as the font spells it, not as the PostScript name splits into
//! words (#130).

use vectorcraft_doc::NodeKind;
use vectorcraft_text::FontDb;

use crate::import::import;

/// The bundled Source Sans 3 renamed `family` (as long as the original name).
fn renamed(family: &str) -> Vec<u8> {
    let mut data = include_bytes!("../../../../assets/fonts/SourceSans3-Regular.ttf").to_vec();
    let utf16 = |s: &str| s.encode_utf16().flat_map(u16::to_be_bytes).collect::<Vec<u8>>();
    for (from, to) in [(utf16("Source Sans 3"), utf16(family)), (b"Source Sans 3".to_vec(), family.as_bytes().to_vec())] {
        assert_eq!(from.len(), to.len());
        let mut i = 0;
        while let Some(at) = data[i..].windows(from.len()).position(|w| w == from) {
            data[i + at..i + at + to.len()].copy_from_slice(&to);
            i += at + to.len();
        }
    }
    data
}

#[test]
fn postscript_names_resolve_to_the_available_family() {
    // Split into words, its PostScript name would read "My Face Ya Hei3".
    assert!(FontDb::global().add_font(renamed("MyFaceYaHei 3")) > 0);
    let body = "/MyFaceYaHei3-Bold findfont 12 scalefont setfont 10 20 moveto (Hi) show";
    let eps = format!("%!PS-Adobe-3.0 EPSF-3.0\n%%BoundingBox: 0 0 100 100\n%%EndComments\n{body}\nshowpage\n%%EOF\n");
    let r = import(eps.as_bytes()).unwrap();
    let mut styles = vec![];
    r.document.walk(|n| {
        if let NodeKind::Text(t) = &n.kind {
            styles.extend(t.runs.iter().map(|r| (r.style.font_family.clone(), r.style.font_style.clone())));
        }
    });
    assert_eq!(styles, [("MyFaceYaHei 3".to_string(), "Bold".to_string())]);
}
