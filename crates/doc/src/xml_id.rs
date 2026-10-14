//! Object names ↔ XML IDs (Preferences › Units › Identify Objects By).
//!
//! Illustrator's public documentation: with XML ID selected, the Layers panel shows and edits
//! names that conform to XML naming (letter, underscore or colon first; no spaces). Underscores
//! stand for spaces and `_xHH_` / `_xHHHH_` escape other characters, as in `.ai` `AI10_ArtUID`
//! values ([edit dynamic objects](https://helpx.adobe.com/illustrator/desktop/automate-visualize-data/automate-actions/edit-dynamic-objects.html)).

/// An object name from its XML id: `_` is a space and `_xHH_` / `_xHHHH_` a character; ids made
/// unique end with `_` and 20 or more digits (stripped).
pub fn to_object_name(id: &str) -> String {
    let id = strip_unique(id);
    let mut out = String::with_capacity(id.len());
    let mut rest = id;
    while let Some(c) = rest.chars().next() {
        if c == '_' {
            if let Some((ch, n)) = escaped(rest) {
                out.push(ch);
                rest = rest.get(n..).unwrap_or_default();
                continue;
            }
            out.push(' ');
        } else {
            out.push(c);
        }
        rest = rest.get(c.len_utf8()..).unwrap_or_default();
    }
    out
}

/// The XML id for an object name (the inverse of [`to_object_name`]).
pub fn from_object_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len().saturating_mul(2));
    for (i, c) in name.chars().enumerate() {
        if c == ' ' {
            out.push('_');
        } else if c == '_' {
            out.push_str("_x5F_");
        } else if i == 0 && c.is_ascii_digit() {
            out.push_str(&format!("_x{:X}_", u32::from(c)));
        } else if c.is_alphanumeric() || c == '.' || c == '-' || c == ':' {
            out.push(c);
        } else {
            out.push_str(&format!("_x{:X}_", u32::from(c)));
        }
    }
    if out.is_empty() || !starts_name(&out) {
        out.insert(0, '_');
    }
    out
}

/// Whether `id` is a usable XML ID for Identify Objects By › XML ID: non-empty, no whitespace,
/// starts with a letter, `_` or `:`, and otherwise letters, digits, `.`, `-`, `_` or `:`.
pub fn is_valid(id: &str) -> bool {
    let id = id.trim();
    if id.is_empty() || id.chars().any(char::is_whitespace) {
        return false;
    }
    let mut chars = id.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_alphabetic() || first == '_' || first == ':') {
        return false;
    }
    chars.all(|c| c.is_alphanumeric() || matches!(c, '_' | '.' | '-' | ':'))
}

fn starts_name(id: &str) -> bool {
    id.starts_with(|c: char| c.is_alphabetic() || c == '_' || c == ':')
}

/// `_xHHHH_` at the start of `s`: the character and the escape's length.
fn escaped(s: &str) -> Option<(char, usize)> {
    let body = s.strip_prefix("_x")?;
    let end = body.find('_')?;
    let hex = body.get(..end)?;
    if hex.is_empty() || hex.len() > 6 {
        return None;
    }
    let c = char::from_u32(u32::from_str_radix(hex, 16).ok()?)?;
    Some((c, end + 3))
}

/// `id` without the `_000…_` suffix that makes it unique.
fn strip_unique(id: &str) -> &str {
    let Some(body) = id.strip_suffix('_') else {
        return id;
    };
    let Some(at) = body.rfind('_') else {
        return id;
    };
    let digits = body.get(at + 1..).unwrap_or_default();
    if digits.len() >= 20 && digits.bytes().all(|b| b.is_ascii_digit()) { body.get(..at).unwrap_or(id) } else { id }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ai_style_ids() {
        for (name, id) in [
            ("Layer 1", "Layer_1"),
            ("My Rect #2", "My_Rect__x23_2"),
            ("1abc (paren)", "_x31_abc__x28_paren_x29_"),
            ("Ünï 日本", "Ünï_日本"),
            ("a_b", "a_x5F_b"),
        ] {
            assert_eq!(from_object_name(name), id, "encode {name}");
            assert_eq!(to_object_name(id), name, "decode {id}");
        }
        assert_eq!(to_object_name("S_00000092429200007574234970000005974767392385616558_"), "S");
        assert!(is_valid("Layer_1") && is_valid("_x31_abc") && is_valid(":root"));
        assert!(!is_valid("My Rect") && !is_valid("1abc") && !is_valid(""));
    }
}
