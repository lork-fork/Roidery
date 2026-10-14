//! PDF import names type by the installed fonts in a new session (#130). One test in its own
//! process, so the process-wide font database starts fresh, as it does when the app starts.
// Integration tests: unwrapping and panicking on failure is fine here, unlike in shipped code (AGENTS.md › Robustness).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use vectorcraft_doc::NodeKind;
use vectorcraft_pdf::{ImportOptions, import_with_report};
use vectorcraft_testkit::pdf::{PdfPage, pdf_with};
use vectorcraft_text::{FontDb, system_font_dirs};

#[test]
fn a_fresh_session_resolves_pdf_fonts_to_installed_families() {
    let bundled = FontDb::with_font_dirs(vec![]).families();
    let probe = FontDb::with_font_dirs(system_font_dirs());
    probe.load_system_fonts();
    let installed = probe.families();
    // A family whose PDF name is its name without the spaces (as "TimesNewRoman").
    let Some(family) = installed.iter().find(|f| !bundled.contains(f) && f.contains(' ') && f.chars().all(|c| c.is_ascii_alphanumeric() || c == ' '))
    else {
        eprintln!("no system fonts installed: nothing to check");
        return;
    };
    let resources = format!("/Font << /F1 << /Type /Font /Subtype /TrueType /BaseFont /{} >> >>", family.replace(' ', ""));
    let page = PdfPage { resources, ..PdfPage::new(100.0, 100.0, "BT /F1 12 Tf 10 30 Td (Hi) Tj ET") };
    let r = import_with_report(&pdf_with(&[page], &[], None), &ImportOptions::default()).unwrap();
    let mut families = vec![];
    r.document.walk(|n| {
        if let NodeKind::Text(t) = &n.kind {
            families.extend(t.runs.iter().map(|r| r.style.font_family.clone()));
        }
    });
    assert_eq!(families, std::slice::from_ref(family));
    assert!(!r.warnings.iter().any(|w| w.contains(family.as_str())), "{:?}", r.warnings);
}
