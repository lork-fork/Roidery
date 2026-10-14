//! How deep an SVG's elements nest, found in the XML before usvg runs: usvg converts the tree by
//! recursion, a few stack frames an element, so a file nested deeply enough overflows the stack
//! (an abort, not an error). Such a file is refused: its nesting can't be merged before usvg has
//! read it, and usvg is what can't read it.
//!
//! What counts is the tree usvg walks, not the XML alone: a `<use>` holds a copy of what it
//! refers to, and a clip path, mask, pattern, marker or filter is converted where it is used
//! (`clip-path: url(#a)` on an element of a clip path makes a chain as deep as there are clip
//! paths). So each element's depth is the deepest of what it holds and what it refers to.

use std::collections::HashMap;
use std::collections::hash_map::Entry;

use usvg::roxmltree;

use super::css::{Styles, XNode, parse_decls};

/// The deepest an SVG's elements may nest, references followed. Far beyond what drawing apps
/// write, and deeper than a document's objects may nest ([`vectorcraft_doc::MAX_DEPTH`]: wrappers
/// that hold one object aren't kept, and what is still deeper is merged into the group around
/// it). usvg uses about 15 kB of stack for each level: the importer runs on a thread with 16 MB
/// ([`vectorcraft_doc::depth::with_deep_stack`]), except on the web, where there is the one
/// stack: [`WEB_DEPTH`] there.
pub const MAX_ELEMENT_DEPTH: usize = if cfg!(target_arch = "wasm32") { WEB_DEPTH } else { 256 };

/// The stack of the web build: rustc links `wasm32-unknown-unknown` with
/// `-z stack-size=1048576` (`rustc --print link-args` shows it), and the web app gives the linker
/// no arguments of its own (`apps/vectorcraft-web`: `Trunk.toml`, `index.html`, no rustflags).
const WEB_STACK: usize = 1 << 20;

/// The stack usvg uses for each level of elements: about 70 levels overflowed the 1 MB main
/// thread of Windows. Measured natively, never in a browser.
const LEVEL_STACK: usize = 15 << 10;

/// The deepest elements may nest on the web: half its stack at usvg's cost a level, the other
/// half left for what is on the stack when a file is opened and for the cost being a native
/// measure. 1,048,576 / (2 × 15,360) = 34.
const WEB_DEPTH: usize = WEB_STACK / (2 * LEVEL_STACK);

/// Properties whose value an element's descendants use too: the paint server they name is
/// converted at each descendant, under everything between.
const INHERITED: [&str; 6] = ["fill", "stroke", "marker", "marker-start", "marker-mid", "marker-end"];

/// The ids of the `url(#id)` references in a property value.
fn urls(value: &str) -> impl Iterator<Item = &str> {
    value.split("url(").skip(1).filter_map(|rest| {
        let id = rest.trim_start().trim_start_matches(['"', '\'']).strip_prefix('#')?;
        id.split([')', '"', '\'']).next().map(str::trim).filter(|id| !id.is_empty())
    })
}

/// The elements `n` refers to, each with whether `n`'s descendants use it too.
fn references<'a, 'i>(n: XNode<'a, 'i>, ids: &HashMap<&str, XNode<'a, 'i>>, css: &Styles, out: &mut Vec<(XNode<'a, 'i>, bool)>) {
    let add = |out: &mut Vec<(XNode<'a, 'i>, bool)>, name: &str, value: &str| {
        let inherited = INHERITED.contains(&name);
        out.extend(urls(value).filter_map(|id| ids.get(id)).map(|t| (*t, inherited)));
    };
    for a in n.attributes() {
        match a.name() {
            "href" => out.extend(a.value().trim().strip_prefix('#').and_then(|id| ids.get(id)).map(|t| (*t, false))),
            // Read with the style sheets' rules below.
            "style" if css.has_rules() => {}
            "style" => parse_decls(a.value()).iter().for_each(|(k, v, _)| add(out, k, v)),
            name => add(out, name, a.value()),
        }
    }
    if css.has_rules() {
        css.declared(n).iter().for_each(|(k, v)| add(out, k, v));
    }
}

/// Do the elements of the XML text `svg` nest deeper than [`MAX_ELEMENT_DEPTH`]? Read from the
/// text, before any XML reader: roxmltree (ours and usvg's) reads elements by recursion too. Tags
/// are counted as the text has them; markup in the values of a document type's entities counts
/// ten times (how deep entities may refer to one another), on top of the deepest element.
pub(crate) fn xml_too_deep(svg: &str) -> bool {
    let b = svg.as_bytes();
    let find =
        |from: usize, what: &[u8]| b.get(from..).and_then(|rest| rest.windows(what.len()).position(|w| w == what)).map(|at| from + at + what.len());
    // The end of the tag starting at `from` (past its `>`), and whether it closes itself.
    let tag_end = |from: usize| {
        let mut quote = None;
        for (i, c) in b.iter().enumerate().skip(from) {
            match (quote, c) {
                (None, b'"' | b'\'') => quote = Some(*c),
                (Some(q), c) if q == *c => quote = None,
                (None, b'>') => return (i + 1, b.get(i.wrapping_sub(1)) == Some(&b'/')),
                _ => {}
            }
        }
        (b.len(), false)
    };
    // (now, deepest) of the elements, and of the markup inside the document type.
    let (mut elements, mut entities) = ((0usize, 0usize), (0usize, 0usize));
    let mut doctype_end = 0;
    let mut i = 0;
    while let Some(at) = b.get(i..).and_then(|rest| rest.iter().position(|c| *c == b'<')).map(|at| i + at) {
        let rest = b.get(at..).unwrap_or_default();
        let depth = if at < doctype_end { &mut entities } else { &mut elements };
        i = if rest.starts_with(b"<!--") {
            find(at, b"-->").unwrap_or(b.len())
        } else if rest.starts_with(b"<![CDATA[") {
            find(at, b"]]>").unwrap_or(b.len())
        } else if rest.starts_with(b"<?") {
            find(at, b"?>").unwrap_or(b.len())
        } else if rest.starts_with(b"<!DOCTYPE") {
            // Up to the `>` that ends it: after its internal subset (`[` … `]`) when it has one.
            let plain = tag_end(at).0;
            let subset = b.get(at..plain).is_some_and(|head| head.contains(&b'['));
            doctype_end = if subset { find(at, b"]>").unwrap_or(b.len()) } else { plain };
            at + 2
        } else if rest.starts_with(b"<!") {
            // A declaration of the document type: the markup in its value is counted as it comes.
            at + 2
        } else if rest.starts_with(b"</") {
            depth.0 = depth.0.saturating_sub(1);
            tag_end(at).0
        } else if at < doctype_end {
            // In an entity's value, itself in quotes: up to the tag's `>`.
            let end = find(at, b">").unwrap_or(b.len());
            depth.1 = depth.1.max(depth.0 + 1);
            depth.0 += usize::from(b.get(end.wrapping_sub(2)) != Some(&b'/'));
            end
        } else {
            let (end, empty) = tag_end(at);
            depth.1 = depth.1.max(depth.0 + 1);
            depth.0 += usize::from(!empty);
            end
        };
        if elements.1 + 10 * entities.1 > MAX_ELEMENT_DEPTH {
            return true;
        }
    }
    false
}

/// Does anything in `xml` nest deeper than [`MAX_ELEMENT_DEPTH`], references followed? Without
/// recursion.
pub(super) fn too_deep(xml: &roxmltree::Document) -> bool {
    let elements = || xml.descendants().filter(|n| n.is_element());
    let ids: HashMap<&str, XNode> = elements().filter_map(|n| Some((n.attribute("id")?, n))).collect();
    let css = Styles::new(xml);
    // How deep each element's own tree goes, itself included; 0 while it is being worked out: a
    // reference back to it then is a loop, which usvg leaves out.
    let mut heights: HashMap<roxmltree::NodeId, usize> = HashMap::new();
    enum Step<'a, 'i> {
        Enter(XNode<'a, 'i>),
        Leave(XNode<'a, 'i>, Vec<(XNode<'a, 'i>, bool)>),
    }
    let mut stack = vec![Step::Enter(xml.root_element())];
    while let Some(step) = stack.pop() {
        match step {
            Step::Enter(n) => {
                match heights.entry(n.id()) {
                    Entry::Occupied(_) => continue,
                    Entry::Vacant(v) => v.insert(0),
                };
                let mut refs = vec![];
                references(n, &ids, &css, &mut refs);
                let targets: Vec<XNode> = refs.iter().map(|(t, _)| *t).collect();
                stack.push(Step::Leave(n, refs));
                stack.extend(n.children().filter(|c| c.is_element()).chain(targets).map(Step::Enter));
            }
            Step::Leave(n, refs) => {
                let of = |n: XNode| heights.get(&n.id()).copied().unwrap_or(0);
                let held = n.children().filter(|c| c.is_element()).map(of).max().unwrap_or(0);
                let used = |inherited: bool| refs.iter().filter(|(_, i)| *i == inherited).map(|(t, _)| of(*t)).max().unwrap_or(0);
                let height = 1 + held.max(used(false)) + used(true);
                if height > MAX_ELEMENT_DEPTH {
                    return true;
                }
                heights.insert(n.id(), height);
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deep(svg: &str) -> bool {
        too_deep(&roxmltree::Document::parse(svg).unwrap())
    }

    fn svg(body: &str) -> String {
        format!(r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="10" height="10">{body}</svg>"#)
    }

    /// `n` groups in one another around a rectangle.
    fn groups(n: usize) -> String {
        format!("{}<rect width='1' height='1'/>{}", "<g>".repeat(n), "</g>".repeat(n))
    }

    #[test]
    fn elements_nest_up_to_the_cap() {
        // The root, the groups and the rectangle.
        let (at, past) = (svg(&groups(MAX_ELEMENT_DEPTH - 2)), svg(&groups(MAX_ELEMENT_DEPTH - 1)));
        assert!(!deep(&at) && !xml_too_deep(&at));
        assert!(deep(&past) && xml_too_deep(&past));
        assert!(xml_too_deep(&svg(&groups(200_000))), "any depth is measured");
    }

    #[test]
    fn the_text_is_measured_as_xml_reads_it() {
        let many = r#"<g a='>' b="<g>"><rect/></g>"#.repeat(500);
        assert!(!xml_too_deep(&svg(&many)), "side by side, with `>` and tags in attribute values");
        let hidden = format!("<!-- {0} --><![CDATA[{0}]]><?pi {0}?>", "<g>".repeat(500));
        assert!(!xml_too_deep(&svg(&hidden)), "comments, character data and instructions hold no elements");
        assert!(xml_too_deep(&format!("<svg>{}", "<g>".repeat(MAX_ELEMENT_DEPTH))), "never closed");
        assert!(!xml_too_deep(&format!("{}<svg/>", "</g>".repeat(2 * MAX_ELEMENT_DEPTH))), "ends without beginnings");
        // Entities: namespaces as drawing apps declare them, and markup that would be read ten deep.
        let names = r#"<!DOCTYPE svg PUBLIC "-//W3C//DTD SVG 1.1//EN" "http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd" [ <!ENTITY ns_svg "http://www.w3.org/2000/svg"> ]>"#;
        assert!(!xml_too_deep(&format!("{names}{}", svg(&groups(30)))));
        let tenth = MAX_ELEMENT_DEPTH / 10 + 1;
        let bomb = format!("<!DOCTYPE svg [ <!ENTITY a \"{}&a;{}\"> ]>", "<g>".repeat(tenth), "</g>".repeat(tenth));
        assert!(xml_too_deep(&format!("{bomb}<svg>&a;</svg>")));
        assert!(!xml_too_deep(""));
    }

    #[test]
    fn a_chain_of_uses_is_as_deep_as_what_it_copies() {
        let chain = |n: usize| {
            let defs: String = (0..n).map(|i| format!("<g id='g{i}'><use xlink:href='#g{}'/></g>", i + 1)).collect();
            svg(&format!("<defs>{defs}<rect id='g{n}' width='1' height='1'/></defs><use href='#g0'/>"))
        };
        assert!(!deep(&chain(10)));
        assert!(deep(&chain(MAX_ELEMENT_DEPTH / 2)), "two elements a link");
        assert!(deep(&chain(5000)));
    }

    #[test]
    fn chains_of_clips_masks_and_patterns_count_by_attribute_style_and_style_sheet() {
        let by_attribute = |n: usize| -> String {
            (0..n).map(|i| format!("<clipPath id='c{i}' clip-path='url(#c{})'><rect width='1' height='1'/></clipPath>", i + 1)).collect()
        };
        let by_style = |n: usize| -> String {
            (0..n).map(|i| format!("<mask id='c{i}' style='mask: url( \"#c{}\" )'><rect width='1' height='1'/></mask>", i + 1)).collect()
        };
        let by_sheet = |n: usize| -> String {
            let rules: String = (0..n).map(|i| format!(".p{i} {{ fill: url(#c{}) }}", i + 1)).collect();
            let patterns: String = (0..n)
                .map(|i| format!("<pattern id='c{i}' width='1' height='1'><g class='p{i}'><rect width='1' height='1'/></g></pattern>"))
                .collect();
            format!("<style>{rules}</style>{patterns}")
        };
        for chain in [&by_attribute as &dyn Fn(usize) -> String, &by_style, &by_sheet] {
            let file =
                |n| svg(&format!("<defs>{}</defs><rect width='5' height='5' fill='url(#c0)' clip-path='url(#c0)' mask='url(#c0)'/>", chain(n)));
            assert!(!deep(&file(5)));
            assert!(deep(&file(MAX_ELEMENT_DEPTH)));
        }
    }

    #[test]
    fn many_paint_servers_used_side_by_side_are_not_a_chain() {
        // As drawing apps write gradients: a class each, used by objects that aren't inside one another.
        let rules: String = (0..300).map(|i| format!(".c{i}{{fill:url(#g{i})}}")).collect();
        let defs: String = (0..300).map(|i| format!("<linearGradient id='g{i}'><stop class='c{i}' offset='0'/></linearGradient>")).collect();
        let art: String = (0..300).map(|i| format!("<g class='c{i}'><rect class='c{i}' width='1' height='1'/></g>")).collect();
        assert!(!deep(&svg(&format!("<style>{rules}</style><defs>{defs}</defs>{art}"))));
    }

    #[test]
    fn a_loop_of_references_is_not_deep() {
        assert!(!deep(&svg("<g id='a'><use href='#b'/></g><g id='b'><use href='#a'/></g><clipPath id='c' clip-path='url(#c)'/>")));
    }

    #[test]
    fn an_inherited_paint_is_converted_under_everything_between() {
        // Neither the pattern nor the art is too deep; the pattern under the art's last object is.
        let half = MAX_ELEMENT_DEPTH / 2;
        let pattern = format!("<pattern id='p' width='1' height='1'>{}</pattern>", groups(half));
        assert!(!deep(&svg(&format!("<defs>{pattern}</defs><g fill='url(#p)'>{}</g>", groups(half / 2)))));
        assert!(deep(&svg(&format!("<defs>{pattern}</defs><g fill='url(#p)'>{}</g>", groups(half)))));
    }
}
