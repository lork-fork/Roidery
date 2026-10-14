//! Pattern-free English hyphenation: break long words between syllables at vowel–consonant
//! boundaries (V-CV: "ty-po", and VC-CV: "hap-pen"), never leaving fewer than [`MIN_BEFORE`]
//! letters before or [`MIN_AFTER`] after the hyphen.

/// Only words at least this long are hyphenated (Illustrator's default: longer than 5 letters).
pub const MIN_WORD: usize = 6;
pub const MIN_BEFORE: usize = 3;
pub const MIN_AFTER: usize = 3;

fn is_vowel(c: char) -> bool {
    matches!(c.to_ascii_lowercase(), 'a' | 'e' | 'i' | 'o' | 'u' | 'y') || (c.is_alphabetic() && !c.is_ascii())
}

/// Consonant pairs that stay together at the start of a syllable ("gra-phy", "tea-cher").
fn onset_pair(a: char, b: char) -> bool {
    let (a, b) = (a.to_ascii_lowercase(), b.to_ascii_lowercase());
    matches!(
        (a, b),
        ('c' | 's' | 't' | 'p' | 'w' | 'g', 'h')
            | ('b' | 'c' | 'd' | 'f' | 'g' | 'k' | 'p' | 't', 'r')
            | ('b' | 'c' | 'f' | 'g' | 'k' | 'p' | 's', 'l')
            | ('q', 'u')
    )
}

/// Allowed hyphenation points in `word`, as char indices (a hyphen goes *before* the char).
/// Words containing non-letters (other than a trailing apostrophe) are not hyphenated.
pub fn hyphen_points(word: &str) -> Vec<usize> {
    let c: Vec<char> = word.chars().collect();
    let n = c.len();
    if n < MIN_WORD || !c.iter().all(|c| c.is_alphabetic() || matches!(c, '\'' | '’')) {
        return vec![];
    }
    // A capitalised run like "NASA" or a proper name stays whole only when all caps.
    if c.iter().all(|c| c.is_uppercase()) {
        return vec![];
    }
    let v: Vec<bool> = c.iter().map(|&x| is_vowel(x)).collect();
    let mut out = vec![];
    for i in MIN_BEFORE..=n.saturating_sub(MIN_AFTER) {
        if !c[i].is_alphabetic() || !c[i - 1].is_alphabetic() {
            continue;
        }
        let ok = if v[i - 1] && !v[i] {
            // V-CV: a single consonant (or an onset pair) followed by a vowel starts the syllable.
            (i + 1 < n && v[i + 1]) || (i + 2 < n && onset_pair(c[i], c[i + 1]) && v[i + 2])
        } else if !v[i - 1] && !v[i] {
            // VC-CV: split a consonant cluster after the first consonant unless it's an onset pair.
            i >= 2 && v[i - 2] && i + 1 < n && v[i + 1] && !onset_pair(c[i - 1], c[i])
        } else {
            false
        };
        if ok {
            out.push(i);
        }
    }
    out
}

/// `word` with `-` at every hyphenation point (for tests and diagnostics).
pub fn hyphenate_word(word: &str) -> String {
    let pts = hyphen_points(word);
    let mut s = String::with_capacity(word.len() + pts.len());
    for (i, ch) in word.chars().enumerate() {
        if pts.contains(&i) {
            s.push('-');
        }
        s.push(ch);
    }
    s
}
