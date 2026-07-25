//! Byte-string operations behind the `String` method intrinsics
//! (stdlib.md §8, Q21).
//!
//! Every measure is a **byte** measure over the language's immutable
//! UTF-8 strings: ASCII programs behave exactly as JS; on non-ASCII
//! text the values diverge from JS's UTF-16 units (recorded in Q21,
//! not hidden). Case mapping uses Unicode Default Case Conversion, and
//! trimming uses ECMA's explicit WhiteSpace + LineTerminator set.
//!
//! These functions are pure and total — no panics, no traps. The Q21
//! argument errors (`repeat(-1)`, `split("")`, `replaceAll("", …)`,
//! empty-pad padding) trap in [`crate::ffi`] *before* these are
//! called; where a guard would still be violated, each function
//! documents a harmless total fallback instead of a panic (CLAUDE.md
//! core principle 5).

/// True for exactly ECMA's WhiteSpace + LineTerminator code points
/// (Q21). This intentionally includes U+FEFF and excludes U+0085,
/// unlike Rust's [`char::is_whitespace`].
#[must_use]
pub fn is_ecma_whitespace(ch: char) -> bool {
    matches!(
        ch,
        '\u{0009}'
            | '\u{000A}'
            | '\u{000B}'
            | '\u{000C}'
            | '\u{000D}'
            | '\u{0020}'
            | '\u{00A0}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200A}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202F}'
            | '\u{205F}'
            | '\u{3000}'
            | '\u{FEFF}'
    )
}

/// First byte index of `needle` in `hay` at or after byte `at`;
/// `None` when absent. An empty needle matches at `at` when
/// `at <= hay.len()`. Naive scan (no dependency; needles are short).
#[must_use]
fn find_from(hay: &[u8], needle: &[u8], at: usize) -> Option<usize> {
    if at > hay.len() {
        return None;
    }
    if needle.is_empty() {
        return Some(at);
    }
    if hay.len() - at < needle.len() {
        return None;
    }
    (at..=hay.len() - needle.len()).find(|&i| &hay[i..i + needle.len()] == needle)
}

/// `indexOf(needle, from)`: first byte index or −1. `from` is clamped
/// to `[0, hay.len()]` (negative → 0, beyond length → length); an
/// empty needle returns the clamped `from` (ECMA-262).
#[must_use]
pub fn index_of(hay: &[u8], needle: &[u8], from: i32) -> i32 {
    let at = usize::try_from(from.max(0)).unwrap_or(0).min(hay.len());
    match find_from(hay, needle, at) {
        Some(i) => i as i32,
        None => -1,
    }
}

/// `lastIndexOf(needle)`: last byte index or −1; an empty needle
/// returns the length (ECMA-262).
#[must_use]
pub fn last_index_of(hay: &[u8], needle: &[u8]) -> i32 {
    if needle.is_empty() {
        return hay.len() as i32;
    }
    if needle.len() > hay.len() {
        return -1;
    }
    match (0..=hay.len() - needle.len())
        .rev()
        .find(|&i| &hay[i..i + needle.len()] == needle)
    {
        Some(i) => i as i32,
        None => -1,
    }
}

/// `split(sep)`: the byte pieces between non-overlapping left-to-right
/// matches of `sep` — no match → `[hay]`; adjacent/leading/trailing
/// separators produce empty pieces (JS semantics). `sep` must be
/// non-empty (the caller traps on `split("")`); an empty `sep` falls
/// back to `[hay]` rather than panicking.
#[must_use]
pub fn split<'a>(hay: &'a [u8], sep: &[u8]) -> Vec<&'a [u8]> {
    if sep.is_empty() {
        return vec![hay];
    }
    let mut out = Vec::new();
    let mut at = 0usize;
    while let Some(i) = find_from(hay, sep, at) {
        out.push(&hay[at..i]);
        at = i + sep.len();
    }
    out.push(&hay[at..]);
    out
}

/// `trimStart()`: strips leading ECMA WhiteSpace + LineTerminator
/// code points (Q21). Invalid UTF-8 is returned unchanged as a total
/// fallback; language strings are always valid UTF-8.
#[must_use]
pub fn trim_start(s: &[u8]) -> &[u8] {
    match std::str::from_utf8(s) {
        Ok(text) => text.trim_start_matches(is_ecma_whitespace).as_bytes(),
        Err(_) => s,
    }
}

/// `trimEnd()`: strips trailing ECMA WhiteSpace + LineTerminator code
/// points (Q21). Invalid UTF-8 is returned unchanged as a total
/// fallback; language strings are always valid UTF-8.
#[must_use]
pub fn trim_end(s: &[u8]) -> &[u8] {
    match std::str::from_utf8(s) {
        Ok(text) => text.trim_end_matches(is_ecma_whitespace).as_bytes(),
        Err(_) => s,
    }
}

/// `trim()`: strips both ends; an all-whitespace string becomes `""`.
#[must_use]
pub fn trim(s: &[u8]) -> &[u8] {
    trim_end(trim_start(s))
}

/// `repeat(n)`: `n` copies; `repeat(0)` is empty. `n` must be
/// non-negative (the caller traps on `repeat(-1)`); a negative `n`
/// falls back to empty rather than panicking.
#[must_use]
pub fn repeat(s: &[u8], n: i32) -> Vec<u8> {
    s.repeat(usize::try_from(n.max(0)).unwrap_or(0))
}

/// `padStart`/`padEnd` (Q21 byte lengths): pads with cyclic copies of
/// `pad`, the final repeat truncated so the result is exactly `target`
/// bytes ("ab".padStart(5, "xy") → "xyxab"). A receiver already at
/// least `target` bytes long — or a `target` ≤ 0 — returns the
/// receiver's bytes unchanged (the caller allocates a fresh copy). An
/// empty `pad` that would need to fill returns the receiver unchanged
/// here; the caller traps on that case before calling.
#[must_use]
pub fn pad(s: &[u8], target: i32, pad: &[u8], at_start: bool) -> Vec<u8> {
    let target = usize::try_from(target.max(0)).unwrap_or(0);
    if target <= s.len() || pad.is_empty() {
        return s.to_vec();
    }
    let fill = target - s.len();
    let filler = pad.iter().copied().cycle().take(fill);
    let mut out = Vec::with_capacity(target);
    if at_start {
        out.extend(filler);
        out.extend_from_slice(s);
    } else {
        out.extend_from_slice(s);
        out.extend(filler);
    }
    out
}

/// `toUpperCase()`: Unicode Default Case Conversion (Q21). Invalid
/// UTF-8 is returned unchanged as a total fallback; language strings
/// are always valid UTF-8.
#[must_use]
pub fn to_upper(s: &[u8]) -> Vec<u8> {
    match std::str::from_utf8(s) {
        Ok(text) => text.to_uppercase().into_bytes(),
        Err(_) => s.to_vec(),
    }
}

/// `toLowerCase()`: Unicode Default Case Conversion (Q21). Invalid
/// UTF-8 is returned unchanged as a total fallback; language strings
/// are always valid UTF-8.
#[must_use]
pub fn to_lower(s: &[u8]) -> Vec<u8> {
    match std::str::from_utf8(s) {
        Ok(text) => text.to_lowercase().into_bytes(),
        Err(_) => s.to_vec(),
    }
}

/// `replace(pat, repl)`: replaces the first occurrence, literally —
/// `$` in `repl` is not interpreted (Q21). No match returns the bytes
/// unchanged; an empty `pat` matches at index 0 (ECMA-262: the result
/// is `repl + s`).
#[must_use]
pub fn replace_first(s: &[u8], pat: &[u8], repl: &[u8]) -> Vec<u8> {
    match find_from(s, pat, 0) {
        Some(i) => {
            let mut out = Vec::with_capacity(s.len() - pat.len() + repl.len());
            out.extend_from_slice(&s[..i]);
            out.extend_from_slice(repl);
            out.extend_from_slice(&s[i + pat.len()..]);
            out
        }
        None => s.to_vec(),
    }
}

/// `replaceAll(pat, repl)`: replaces every occurrence in one
/// left-to-right pass over the original — a `pat` that reappears
/// inside a replacement is **not** rescanned (JS semantics:
/// `"aa".replaceAll("a", "aa")` is `"aaaa"`). `$` is literal (Q21).
/// `pat` must be non-empty (the caller traps on `replaceAll("", …)`);
/// an empty `pat` falls back to the unchanged bytes rather than
/// looping.
#[must_use]
pub fn replace_all(s: &[u8], pat: &[u8], repl: &[u8]) -> Vec<u8> {
    if pat.is_empty() {
        return s.to_vec();
    }
    let mut out = Vec::new();
    let mut at = 0usize;
    while let Some(i) = find_from(s, pat, at) {
        out.extend_from_slice(&s[at..i]);
        out.extend_from_slice(repl);
        at = i + pat.len();
    }
    out.extend_from_slice(&s[at..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_of_hits_misses_and_clamps() {
        let s = b"hello world";
        assert_eq!(index_of(s, b"o", 0), 4);
        assert_eq!(index_of(s, b"o", 5), 7);
        assert_eq!(index_of(s, b"z", 0), -1);
        // Negative `from` clamps to 0; beyond-length clamps to length.
        assert_eq!(index_of(s, b"o", -3), 4);
        assert_eq!(index_of(s, b"o", 99), -1);
        // Empty needle: the clamped `from` (ECMA-262).
        assert_eq!(index_of(s, b"", 0), 0);
        assert_eq!(index_of(s, b"", 4), 4);
        assert_eq!(index_of(s, b"", 99), 11);
        assert_eq!(index_of(b"", b"", 0), 0);
        // Needle longer than the rest of the string.
        assert_eq!(index_of(s, b"worlds", 0), -1);
        assert_eq!(index_of(s, b"world", 7), -1);
        assert_eq!(index_of(b"ab", b"abc", 0), -1);
    }

    #[test]
    fn last_index_of_scans_from_the_end() {
        let s = b"hello world";
        assert_eq!(last_index_of(s, b"o"), 7);
        assert_eq!(last_index_of(s, b"z"), -1);
        assert_eq!(last_index_of(s, b"hello"), 0);
        // Empty needle: the length (ECMA-262).
        assert_eq!(last_index_of(s, b""), 11);
        assert_eq!(last_index_of(b"", b""), 0);
        assert_eq!(last_index_of(b"ab", b"abc"), -1);
    }

    #[test]
    fn split_matches_js_piece_order() {
        let parts = split(b"a,b,,c", b",");
        assert_eq!(parts, vec![&b"a"[..], b"b", b"", b"c"]);
        // No match: the whole string.
        assert_eq!(split(b"ab", b"x"), vec![&b"ab"[..]]);
        // Leading/trailing separators produce empty pieces.
        assert_eq!(split(b",a,", b","), vec![&b""[..], b"a", b""]);
        // The empty string splits to one empty piece.
        assert_eq!(split(b"", b","), vec![&b""[..]]);
        // Multi-byte separator.
        assert_eq!(split(b"xabyab", b"ab"), vec![&b"x"[..], b"y", b""]);
        // The documented empty-separator fallback (the FFI traps first).
        assert_eq!(split(b"ab", b""), vec![&b"ab"[..]]);
    }

    #[test]
    fn trim_family_strips_exactly_ecma_whitespace() {
        let s = b"  x\t";
        assert_eq!(trim(s), b"x");
        assert_eq!(trim_start(s), b"x\t");
        assert_eq!(trim_end(s), b"  x");
        // Every ECMA WhiteSpace + LineTerminator code point.
        let all = "\u{0009}\u{000A}\u{000B}\u{000C}\u{000D}\u{0020}\
                   \u{00A0}\u{1680}\u{2000}\u{2001}\u{2002}\u{2003}\
                   \u{2004}\u{2005}\u{2006}\u{2007}\u{2008}\u{2009}\
                   \u{200A}\u{2028}\u{2029}\u{202F}\u{205F}\u{3000}\
                   \u{FEFF}";
        assert_eq!(trim(all.as_bytes()), b"");
        assert_eq!(trim(b""), b"");
        // Interior whitespace is untouched.
        assert_eq!(
            trim("\u{3000}a\u{00A0}b\u{FEFF}".as_bytes()),
            "a\u{00A0}b".as_bytes()
        );
        // U+0085 is Unicode White_Space but not ECMA whitespace.
        assert_eq!(
            trim("\u{0085}x\u{0085}".as_bytes()),
            "\u{0085}x\u{0085}".as_bytes()
        );
        // Total fallback for bytes outside the language's UTF-8 contract.
        assert_eq!(trim(&[0xFF, b' ']), &[0xFF, b' ']);
    }

    #[test]
    fn repeat_counts_including_zero() {
        assert_eq!(repeat(b"ab", 0), b"");
        assert_eq!(repeat(b"ab", 1), b"ab");
        assert_eq!(repeat(b"ab", 3), b"ababab");
        assert_eq!(repeat(b"", 5), b"");
        // The documented negative fallback (the FFI traps first).
        assert_eq!(repeat(b"ab", -1), b"");
    }

    #[test]
    fn pad_truncates_the_final_repeat_like_js() {
        // The JS-verified truncation rule: "ab".padStart(5, "xy") is
        // "xyxab" and .padEnd(5, "xy") is "abxyx".
        assert_eq!(pad(b"ab", 5, b"xy", true), b"xyxab");
        assert_eq!(pad(b"ab", 5, b"xy", false), b"abxyx");
        // Default single-space pad.
        assert_eq!(pad(b"7", 3, b" ", true), b"  7");
        assert_eq!(pad(b"7", 3, b" ", false), b"7  ");
        // Exact and already-long-enough receivers: unchanged bytes.
        assert_eq!(pad(b"abc", 3, b"x", true), b"abc");
        assert_eq!(pad(b"abcd", 2, b"x", true), b"abcd");
        assert_eq!(pad(b"ab", -1, b"x", true), b"ab");
        // The documented empty-pad fallback (the FFI traps first).
        assert_eq!(pad(b"ab", 5, b"", true), b"ab");
    }

    #[test]
    fn case_mapping_uses_unicode_default_conversion() {
        assert_eq!(to_upper(b"mix 3d!"), b"MIX 3D!");
        assert_eq!(to_lower(b"MIX 3D!"), b"mix 3d!");
        // Round trip over the letters; digits/punctuation untouched.
        assert_eq!(to_lower(&to_upper(b"aZ09_")), b"az09_");
        assert_eq!(to_upper("ß ﬄ ΣΣς ı".as_bytes()), "SS FFL ΣΣΣ I".as_bytes());
        assert_eq!(to_lower("ΣΣς İ".as_bytes()), "σσς i\u{0307}".as_bytes());
        // U+0130 expands from two UTF-8 bytes to three.
        assert_eq!(to_lower("İ".as_bytes()).len(), 3);
        // Total fallback for bytes outside the language's UTF-8 contract.
        assert_eq!(to_upper(&[0xFF, b'a']), &[0xFF, b'a']);
    }

    #[test]
    fn replace_first_is_literal_and_first_only() {
        assert_eq!(replace_first(b"aaa", b"a", b"b"), b"baa");
        assert_eq!(replace_first(b"abc", b"z", b"y"), b"abc");
        // `$` is not interpreted (Q21).
        assert_eq!(replace_first(b"x=1", b"1", b"$&"), b"x=$&");
        // Empty pattern matches at 0 (ECMA-262): repl + s.
        assert_eq!(replace_first(b"abc", b"", b"X"), b"Xabc");
    }

    #[test]
    fn replace_all_never_rescans_a_replacement() {
        assert_eq!(replace_all(b"abcabc", b"bc", b"X"), b"aXaX");
        // The replacement contains the pattern; one pass, no rescan.
        assert_eq!(replace_all(b"aa", b"a", b"aa"), b"aaaa");
        assert_eq!(replace_all(b"abc", b"z", b"y"), b"abc");
        assert_eq!(replace_all(b"x=1", b"1", b"$&"), b"x=$&");
        // The documented empty-pattern fallback (the FFI traps first).
        assert_eq!(replace_all(b"ab", b"", b"X"), b"ab");
    }

    #[test]
    fn ecma_whitespace_predicate_includes_and_excludes_the_differences() {
        for ch in [
            '\u{0009}',
            '\u{000A}',
            '\u{000B}',
            '\u{000C}',
            '\u{000D}',
            '\u{0020}',
            '\u{00A0}',
            '\u{1680}',
            '\u{2000}',
            '\u{200A}',
            '\u{2028}',
            '\u{2029}',
            '\u{202F}',
            '\u{205F}',
            '\u{3000}',
            '\u{FEFF}',
        ] {
            assert!(is_ecma_whitespace(ch), "{ch:?}");
        }
        for ch in ['\u{0000}', '\u{0085}', '\u{180E}', '\u{200B}', 'x'] {
            assert!(!is_ecma_whitespace(ch), "{ch:?}");
        }
    }
}
