//! Budgeted, Context-cached regular-expression operations (P23/Q31).

use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

use regress::Regex;

use crate::context::{Context, CLASS_REGEX};
use crate::trap::TrapKind;

#[repr(C)]
struct RegexHeader {
    source: *mut u8,
    flags: *mut u8,
}

#[derive(Clone)]
struct CaptureMatch {
    range: Range<usize>,
    captures: Vec<Option<Range<usize>>>,
    named: HashMap<String, Option<Range<usize>>>,
}

impl CaptureMatch {
    fn from_regress(found: regress::Match) -> Self {
        let named = found
            .named_groups()
            .map(|(name, range)| (name.to_string(), range))
            .collect();
        CaptureMatch {
            range: found.range(),
            captures: found.captures.clone(),
            named,
        }
    }

    fn group(&self, index: usize) -> Option<Range<usize>> {
        if index == 0 {
            Some(self.range.clone())
        } else {
            self.captures.get(index - 1).cloned().flatten()
        }
    }
}

struct RegexValue {
    compiled: Arc<Regex>,
    global: bool,
    last_match: Option<CaptureMatch>,
}

/// Context-owned compiled-pattern cache and per-RegExp match state.
#[derive(Default)]
pub(crate) struct RegexStore {
    compiled: HashMap<(String, String), Arc<Regex>>,
    values: HashMap<usize, RegexValue>,
    #[cfg(test)]
    compilations: usize,
}

struct CheckedFlags {
    canonical: String,
    engine: String,
    global: bool,
}

fn checked_flags(flags: &str) -> Result<CheckedFlags, String> {
    let mut seen = [false; 128];
    for flag in flags.chars() {
        if !flag.is_ascii() || !matches!(flag, 'd' | 'g' | 'i' | 'm' | 's' | 'u' | 'v') {
            return Err(format!(
                "unsupported regular-expression flag `{flag}`; supported flags are d, g, i, m, s, u, v"
            ));
        }
        let slot = &mut seen[flag as usize];
        if *slot {
            return Err(format!("duplicate regular-expression flag `{flag}`"));
        }
        *slot = true;
    }
    if seen[usize::from(b'u')] && seen[usize::from(b'v')] {
        return Err("regular-expression flags `u` and `v` are mutually exclusive".to_string());
    }
    let canonical: String = "dgimsuv"
        .chars()
        .filter(|flag| seen[*flag as usize])
        .collect();
    let engine = canonical
        .chars()
        .filter(|flag| !matches!(flag, 'd' | 'g'))
        .collect();
    Ok(CheckedFlags {
        canonical,
        engine,
        global: seen[usize::from(b'g')],
    })
}

fn normalized_source(pattern: &str) -> Vec<u8> {
    if pattern.is_empty() {
        return b"(?:)".to_vec();
    }
    let mut source = Vec::with_capacity(pattern.len());
    let mut preceding_backslashes = 0usize;
    for ch in pattern.chars() {
        match ch {
            '/' if preceding_backslashes % 2 == 0 => source.extend_from_slice(b"\\/"),
            '\n' => source.extend_from_slice(b"\\n"),
            '\r' => source.extend_from_slice(b"\\r"),
            '\u{2028}' => source.extend_from_slice(b"\\u2028"),
            '\u{2029}' => source.extend_from_slice(b"\\u2029"),
            _ => {
                let mut utf8 = [0u8; 4];
                source.extend_from_slice(ch.encode_utf8(&mut utf8).as_bytes());
            }
        }
        if ch == '\\' {
            preceding_backslashes += 1;
        } else {
            preceding_backslashes = 0;
        }
    }
    source
}

impl RegexStore {
    fn compile(&mut self, pattern: &str, flags: &CheckedFlags) -> Result<Arc<Regex>, String> {
        let key = (pattern.to_string(), flags.canonical.clone());
        if let Some(compiled) = self.compiled.get(&key) {
            return Ok(Arc::clone(compiled));
        }
        let compiled = Arc::new(
            Regex::with_flags(pattern, flags.engine.as_str()).map_err(|error| error.to_string())?,
        );
        self.compiled.insert(key, Arc::clone(&compiled));
        #[cfg(test)]
        {
            self.compilations += 1;
        }
        Ok(compiled)
    }

    fn matcher(&self, handle: *const u8) -> Option<(Arc<Regex>, bool)> {
        self.values
            .get(&(handle as usize))
            .map(|value| (Arc::clone(&value.compiled), value.global))
    }

    fn record(&mut self, handle: *const u8, found: Option<CaptureMatch>) {
        if let Some(value) = self.values.get_mut(&(handle as usize)) {
            value.last_match = found;
        }
    }
}

fn trap_regex(ctx: &mut Context, message: impl Into<String>, pos_id: u32) {
    ctx.trap(TrapKind::Regex, message, pos_id);
}

fn text_from_handle(
    ctx: &mut Context,
    handle: *const u8,
    what: &str,
    pos_id: u32,
) -> Option<String> {
    if handle.is_null() {
        trap_regex(ctx, format!("{what} is null"), pos_id);
        return None;
    }
    // SAFETY: generated code passes a live language string.
    match std::str::from_utf8(unsafe { ctx.str_bytes(handle) }) {
        Ok(text) => Some(text.to_string()),
        Err(_) => {
            ctx.trap(
                TrapKind::Internal,
                format!("{what} is not valid UTF-8"),
                pos_id,
            );
            None
        }
    }
}

fn text_parts(
    ctx: &mut Context,
    handle: *const u8,
    what: &str,
    pos_id: u32,
) -> Option<(*const u8, usize)> {
    if handle.is_null() {
        trap_regex(ctx, format!("{what} is null"), pos_id);
        return None;
    }
    // SAFETY: generated code passes a live language string.
    let bytes = unsafe { ctx.str_bytes(handle) };
    if std::str::from_utf8(bytes).is_err() {
        ctx.trap(
            TrapKind::Internal,
            format!("{what} is not valid UTF-8"),
            pos_id,
        );
        return None;
    }
    Some((bytes.as_ptr(), bytes.len()))
}

unsafe fn text_from_parts<'a>(data: *const u8, len: usize) -> &'a str {
    // SAFETY: `text_parts` validated the bytes. Context allocations are
    // stable and no regex scalar operation deletes or collects strings.
    unsafe {
        std::str::from_utf8_unchecked(std::slice::from_raw_parts(data, len))
    }
}

fn matcher(ctx: &mut Context, handle: *const u8, pos_id: u32) -> Option<(Arc<Regex>, bool)> {
    if handle.is_null() || !ctx.require_live_handle(handle as usize, pos_id) {
        return None;
    }
    let value = ctx.regex_store().matcher(handle);
    if value.is_none() {
        ctx.trap(
            TrapKind::Internal,
            "RegExp handle is not registered in this Context",
            pos_id,
        );
    }
    value
}

fn find_from(
    compiled: &Regex,
    text: &str,
    start: usize,
    budget: u64,
) -> Result<Option<CaptureMatch>, regress::BudgetExhausted> {
    compiled
        .find_from_budgeted(text, start, budget)
        .map(|found| found.map(CaptureMatch::from_regress))
}

fn find_and_record(
    ctx: &mut Context,
    regex: *const u8,
    text: &str,
    pos_id: u32,
) -> Option<Option<CaptureMatch>> {
    let (compiled, _) = matcher(ctx, regex, pos_id)?;
    match find_from(&compiled, text, 0, ctx.regex_budget()) {
        Ok(found) => {
            ctx.regex_store().record(regex, found.clone());
            Some(found)
        }
        Err(_) => {
            ctx.trap(
                TrapKind::RegexBudget,
                format!(
                    "regular-expression execution exhausted its budget of {}",
                    ctx.regex_budget()
                ),
                pos_id,
            );
            None
        }
    }
}

/// Compiles or reuses a cached pattern and allocates one RegExp handle.
pub(crate) fn new(
    ctx: &mut Context,
    pattern_handle: *const u8,
    flags_handle: *const u8,
    pos_id: u32,
) -> *mut u8 {
    let Some(pattern) = text_from_handle(ctx, pattern_handle, "RegExp pattern", pos_id) else {
        return std::ptr::null_mut();
    };
    let Some(flags_text) = text_from_handle(ctx, flags_handle, "RegExp flags", pos_id) else {
        return std::ptr::null_mut();
    };
    let flags = match checked_flags(&flags_text) {
        Ok(flags) => flags,
        Err(error) => {
            trap_regex(ctx, error, pos_id);
            return std::ptr::null_mut();
        }
    };
    let compiled = match ctx.regex_store().compile(&pattern, &flags) {
        Ok(compiled) => compiled,
        Err(error) => {
            trap_regex(ctx, format!("invalid regular expression: {error}"), pos_id);
            return std::ptr::null_mut();
        }
    };
    let source = ctx.alloc_str(&normalized_source(&pattern), pos_id);
    if source.is_null() {
        return std::ptr::null_mut();
    }
    let canonical_flags = ctx.alloc_str(flags.canonical.as_bytes(), pos_id);
    if canonical_flags.is_null() {
        return std::ptr::null_mut();
    }
    let handle = ctx.alloc(std::mem::size_of::<RegexHeader>(), CLASS_REGEX, pos_id);
    if handle.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: `handle` is a fresh allocation large enough for the header.
    unsafe {
        (handle as *mut RegexHeader).write(RegexHeader {
            source,
            flags: canonical_flags,
        });
    }
    ctx.regex_store().values.insert(
        handle as usize,
        RegexValue {
            compiled,
            global: flags.global,
            last_match: None,
        },
    );
    handle
}

/// Tests for a match and records its capture extents.
pub(crate) fn test(ctx: &mut Context, regex: *const u8, subject: *const u8, pos_id: u32) -> i32 {
    let Some((data, len)) = text_parts(ctx, subject, "RegExp subject", pos_id) else {
        return 0;
    };
    // SAFETY: `text_parts` validated a stable live string allocation.
    let text = unsafe { text_from_parts(data, len) };
    find_and_record(ctx, regex, text, pos_id)
        .map(|found| i32::from(found.is_some()))
        .unwrap_or(0)
}

/// Returns a RegExp's source string handle.
pub(crate) fn source(ctx: &mut Context, regex: *const u8, pos_id: u32) -> *mut u8 {
    if matcher(ctx, regex, pos_id).is_none() {
        return std::ptr::null_mut();
    }
    // SAFETY: a registered live RegExp handle has this payload layout.
    unsafe { (*(regex as *const RegexHeader)).source }
}

/// Returns a RegExp's canonical flags string handle.
pub(crate) fn flags(ctx: &mut Context, regex: *const u8, pos_id: u32) -> *mut u8 {
    if matcher(ctx, regex, pos_id).is_none() {
        return std::ptr::null_mut();
    }
    // SAFETY: a registered live RegExp handle has this payload layout.
    unsafe { (*(regex as *const RegexHeader)).flags }
}

/// Returns the first match's UTF-8 byte offset, or -1.
pub(crate) fn search(ctx: &mut Context, subject: *const u8, regex: *const u8, pos_id: u32) -> i32 {
    let Some((data, len)) = text_parts(ctx, subject, "RegExp subject", pos_id) else {
        return -1;
    };
    // SAFETY: `text_parts` validated a stable live string allocation.
    let text = unsafe { text_from_parts(data, len) };
    find_and_record(ctx, regex, text, pos_id)
        .map(|found| found.map_or(-1, |found| found.range.start as i32))
        .unwrap_or(-1)
}

fn append_regex_replacement(
    out: &mut Vec<u8>,
    source: &[u8],
    found: &CaptureMatch,
    replacement: &[u8],
) {
    crate::strops::append_replacement(
        out,
        source,
        found.range.start,
        found.range.end,
        replacement,
        found.captures.len(),
        !found.named.is_empty(),
        |index| found.group(index),
        |name| found.named.get(name).cloned().flatten(),
    );
}

/// Replaces the first regex match, or every match for a global RegExp,
/// with the shared ECMA substituter.
pub(crate) fn replace(
    ctx: &mut Context,
    subject: *const u8,
    regex: *const u8,
    replacement: *const u8,
    pos_id: u32,
) -> *mut u8 {
    let Some((_, global)) = matcher(ctx, regex, pos_id) else {
        return std::ptr::null_mut();
    };
    if global {
        return replace_all(ctx, subject, regex, replacement, pos_id);
    }
    let Some(text) = text_from_handle(ctx, subject, "RegExp subject", pos_id) else {
        return std::ptr::null_mut();
    };
    let Some(replacement) = text_from_handle(ctx, replacement, "RegExp replacement", pos_id) else {
        return std::ptr::null_mut();
    };
    let Some(found) = find_and_record(ctx, regex, &text, pos_id) else {
        return std::ptr::null_mut();
    };
    let mut out = Vec::new();
    if let Some(found) = found {
        out.extend_from_slice(&text.as_bytes()[..found.range.start]);
        append_regex_replacement(&mut out, text.as_bytes(), &found, replacement.as_bytes());
        out.extend_from_slice(&text.as_bytes()[found.range.end..]);
    } else {
        out.extend_from_slice(text.as_bytes());
    }
    ctx.alloc_str(&out, pos_id)
}

fn next_code_point(text: &str, at: usize) -> usize {
    text[at..]
        .chars()
        .next()
        .map_or(text.len(), |ch| at + ch.len_utf8())
}

/// Replaces all matches; every individual search is budgeted and an
/// empty match advances by one Unicode scalar without consuming it.
pub(crate) fn replace_all(
    ctx: &mut Context,
    subject: *const u8,
    regex: *const u8,
    replacement: *const u8,
    pos_id: u32,
) -> *mut u8 {
    let Some(text) = text_from_handle(ctx, subject, "RegExp subject", pos_id) else {
        return std::ptr::null_mut();
    };
    let Some(replacement) = text_from_handle(ctx, replacement, "RegExp replacement", pos_id) else {
        return std::ptr::null_mut();
    };
    let Some((compiled, global)) = matcher(ctx, regex, pos_id) else {
        return std::ptr::null_mut();
    };
    if !global {
        trap_regex(
            ctx,
            "string.replaceAll with a RegExp requires the `g` flag",
            pos_id,
        );
        return std::ptr::null_mut();
    }
    let mut out = Vec::new();
    let mut emitted = 0usize;
    let mut search_at = 0usize;
    let mut last = None;
    loop {
        let found = match find_from(&compiled, &text, search_at, ctx.regex_budget()) {
            Ok(found) => found,
            Err(_) => {
                ctx.trap(
                    TrapKind::RegexBudget,
                    format!(
                        "regular-expression execution exhausted its budget of {}",
                        ctx.regex_budget()
                    ),
                    pos_id,
                );
                return std::ptr::null_mut();
            }
        };
        let Some(found) = found else {
            break;
        };
        out.extend_from_slice(&text.as_bytes()[emitted..found.range.start]);
        append_regex_replacement(&mut out, text.as_bytes(), &found, replacement.as_bytes());
        emitted = found.range.end;
        let empty = found.range.start == found.range.end;
        search_at = if empty {
            if found.range.end == text.len() {
                last = Some(found);
                break;
            }
            next_code_point(&text, found.range.end)
        } else {
            found.range.end
        };
        last = Some(found);
    }
    out.extend_from_slice(&text.as_bytes()[emitted..]);
    ctx.regex_store().record(regex, last);
    ctx.alloc_str(&out, pos_id)
}

fn push_string(ctx: &mut Context, array: *mut u8, bytes: &[u8], pos_id: u32) -> bool {
    let string = ctx.alloc_str(bytes, pos_id);
    if string.is_null() {
        return false;
    }
    let word = string as u64;
    // SAFETY: `array` is an 8-byte-element string array allocated below.
    unsafe { ctx.array_push(array, (&word as *const u64).cast(), pos_id) >= 0 }
}

/// Splits on regex matches and reinjects captures. An unmatched capture
/// is represented by the empty string because the language has no
/// `undefined` element value.
pub(crate) fn split(
    ctx: &mut Context,
    subject: *const u8,
    regex: *const u8,
    pos_id: u32,
) -> *mut u8 {
    let Some(text) = text_from_handle(ctx, subject, "RegExp subject", pos_id) else {
        return std::ptr::null_mut();
    };
    let Some((compiled, _)) = matcher(ctx, regex, pos_id) else {
        return std::ptr::null_mut();
    };
    let array = ctx.array_new(8, pos_id);
    if array.is_null() {
        return std::ptr::null_mut();
    }
    if text.is_empty() {
        let found = match find_from(&compiled, &text, 0, ctx.regex_budget()) {
            Ok(found) => found,
            Err(_) => {
                ctx.trap(
                    TrapKind::RegexBudget,
                    format!(
                        "regular-expression execution exhausted its budget of {}",
                        ctx.regex_budget()
                    ),
                    pos_id,
                );
                return std::ptr::null_mut();
            }
        };
        ctx.regex_store().record(regex, found.clone());
        if found.is_none() && !push_string(ctx, array, b"", pos_id) {
            return std::ptr::null_mut();
        }
        return array;
    }

    let mut previous_end = 0usize;
    let mut search_at = 0usize;
    let mut last = None;
    while search_at < text.len() {
        let found = match find_from(&compiled, &text, search_at, ctx.regex_budget()) {
            Ok(found) => found,
            Err(_) => {
                ctx.trap(
                    TrapKind::RegexBudget,
                    format!(
                        "regular-expression execution exhausted its budget of {}",
                        ctx.regex_budget()
                    ),
                    pos_id,
                );
                return std::ptr::null_mut();
            }
        };
        let Some(found) = found else {
            break;
        };
        if found.range.start == text.len() && found.range.start == found.range.end {
            break;
        }
        if found.range.start == previous_end && found.range.start == found.range.end {
            search_at = next_code_point(&text, search_at);
            continue;
        }
        if !push_string(
            ctx,
            array,
            &text.as_bytes()[previous_end..found.range.start],
            pos_id,
        ) {
            return std::ptr::null_mut();
        }
        for capture in &found.captures {
            let bytes = capture
                .as_ref()
                .map_or(&[][..], |range| &text.as_bytes()[range.clone()]);
            if !push_string(ctx, array, bytes, pos_id) {
                return std::ptr::null_mut();
            }
        }
        previous_end = found.range.end;
        search_at = previous_end;
        last = Some(found);
    }
    if !push_string(ctx, array, &text.as_bytes()[previous_end..], pos_id) {
        return std::ptr::null_mut();
    }
    ctx.regex_store().record(regex, last);
    array
}

/// Returns one recorded capture boundary, or -1.
pub(crate) fn match_boundary(ctx: &mut Context, regex: *const u8, group: i32, end: bool) -> i32 {
    if group < 0 {
        return -1;
    }
    let Some(value) = ctx.regex_store().values.get(&(regex as usize)) else {
        return -1;
    };
    let Some(found) = &value.last_match else {
        return -1;
    };
    found.group(group as usize).map_or(-1, |range| {
        if end {
            range.end as i32
        } else {
            range.start as i32
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn string(ctx: &mut Context, text: &str) -> *mut u8 {
        ctx.alloc_str(text.as_bytes(), 0)
    }

    fn regex(ctx: &mut Context, pattern: &str, flags: &str) -> *mut u8 {
        let pattern = string(ctx, pattern);
        let flags = string(ctx, flags);
        new(ctx, pattern, flags, 0)
    }

    fn replace_text(
        ctx: &mut Context,
        subject: &str,
        pattern: &str,
        flags: &str,
        replacement: &str,
    ) -> Vec<u8> {
        let regex = regex(ctx, pattern, flags);
        let subject = string(ctx, subject);
        let replacement = string(ctx, replacement);
        let result = replace(ctx, subject, regex, replacement, 0);
        assert!(!result.is_null());
        // SAFETY: `replace` returned a live string owned by `ctx`.
        unsafe { ctx.str_bytes(result).to_vec() }
    }

    fn split_text(ctx: &mut Context, subject: &str, pattern: &str) -> Vec<Vec<u8>> {
        let regex = regex(ctx, pattern, "");
        let subject = string(ctx, subject);
        let result = split(ctx, subject, regex, 0);
        assert!(!result.is_null());
        // SAFETY: `split` returned a live array of live string handles.
        unsafe {
            let words = std::slice::from_raw_parts(
                ctx.array_data(result).cast::<u64>(),
                ctx.array_len(result) as usize,
            );
            words
                .iter()
                .map(|word| ctx.str_bytes(*word as *const u8).to_vec())
                .collect()
        }
    }

    #[test]
    fn compiled_patterns_are_cached_but_match_state_is_per_handle() {
        let mut ctx = Context::new();
        let pattern = string(&mut ctx, "(a)");
        let flags = string(&mut ctx, "");
        let first = new(&mut ctx, pattern, flags, 0);
        let second = new(&mut ctx, pattern, flags, 0);
        assert!(!first.is_null() && !second.is_null());
        assert_eq!(ctx.regex_store().compilations, 1);
        let subject = string(&mut ctx, "ba");
        assert_eq!(test(&mut ctx, first, subject, 0), 1);
        assert_eq!(match_boundary(&mut ctx, first, 1, false), 1);
        assert_eq!(match_boundary(&mut ctx, second, 1, false), -1);
    }

    #[test]
    fn replacement_uses_all_ecma_substitution_forms() {
        let mut ctx = Context::new();
        let pattern = string(&mut ctx, "(?<word>a)(b)?");
        let flags = string(&mut ctx, "");
        let regex = new(&mut ctx, pattern, flags, 0);
        let subject = string(&mut ctx, "xabz");
        let replacement = string(
            &mut ctx,
            "[$$][$&][$`][$'][$1][$2][$01][$10][$99][$<word>][$<missing>]",
        );
        let result = replace(&mut ctx, subject, regex, replacement, 0);
        // SAFETY: result is a live string returned by `replace`.
        assert_eq!(
            unsafe { ctx.str_bytes(result) },
            b"x[$][ab][x][z][a][b][a][a0][$99][a][]z"
        );

        let digits = string(&mut ctx, r"\d+");
        let global = string(&mut ctx, "g");
        let regex = new(&mut ctx, digits, global, 0);
        let subject = string(&mut ctx, "a1 b22");
        let replacement = string(&mut ctx, "#");
        let result = replace(&mut ctx, subject, regex, replacement, 0);
        // SAFETY: result is a live string returned by `replace`.
        assert_eq!(unsafe { ctx.str_bytes(result) }, b"a# b#");
    }

    #[test]
    fn repeated_matching_preserves_whole_subject_context_and_byte_offsets() {
        let mut ctx = Context::new();
        assert_eq!(
            replace_text(&mut ctx, "XXX", r"(?<=X)X", "g", "Z"),
            b"XZZ"
        );
        assert_eq!(
            replace_text(&mut ctx, "XXX", r"^X", "g", "Z"),
            b"ZXX"
        );
        assert_eq!(
            replace_text(&mut ctx, "ab cd", r"\b[a-z]", "g", "Z"),
            b"Zb Zd"
        );
        assert_eq!(
            replace_text(&mut ctx, "abc", r"ab|(?<=ab)c", "g", "Z"),
            b"ZZ"
        );
        assert_eq!(
            replace_text(&mut ctx, "XX", r"(?<=X)", "g", "-"),
            b"X-X-"
        );

        let pattern = string(&mut ctx, "X");
        let flags = string(&mut ctx, "g");
        let regex = new(&mut ctx, pattern, flags, 0);
        let subject = string(&mut ctx, "éXX");
        let replacement = string(&mut ctx, "Z");
        let result = replace_all(&mut ctx, subject, regex, replacement, 0);
        assert!(!result.is_null());
        // SAFETY: `replace_all` returned a live string owned by `ctx`.
        assert_eq!(unsafe { ctx.str_bytes(result) }, "éZZ".as_bytes());
        assert_eq!(match_boundary(&mut ctx, regex, 0, false), 3);
        assert_eq!(match_boundary(&mut ctx, regex, 0, true), 4);
    }

    #[test]
    fn repeated_split_preserves_whole_subject_context() {
        let mut ctx = Context::new();
        assert_eq!(
            split_text(&mut ctx, "XXX", r"(?<=X)X"),
            [b"X".as_slice(), b"", b""]
        );
        assert_eq!(
            split_text(&mut ctx, "XXX", r"^X"),
            [b"".as_slice(), b"XX"]
        );
        assert_eq!(
            split_text(&mut ctx, "ab cd", r"\b"),
            [b"ab".as_slice(), b" ", b"cd"]
        );
        assert_eq!(
            split_text(&mut ctx, "abc", r"ab|(?<=ab)c"),
            [b"".as_slice(), b"", b""]
        );
        assert_eq!(
            split_text(&mut ctx, "XX", r"(?<=X)"),
            [b"X".as_slice(), b"X"]
        );
    }

    #[test]
    fn source_uses_ecma_pattern_rendering() {
        let mut ctx = Context::new();
        let flags = string(&mut ctx, "");
        for (pattern_text, expected) in [
            ("", "(?:)"),
            ("/", "\\/"),
            ("\\/", "\\/"),
            ("\n\r\u{2028}\u{2029}", "\\n\\r\\u2028\\u2029"),
        ] {
            let pattern = string(&mut ctx, pattern_text);
            let regex = new(&mut ctx, pattern, flags, 0);
            let rendered = source(&mut ctx, regex, 0);
            // SAFETY: `source` returns a live string owned by `ctx`.
            assert_eq!(unsafe { ctx.str_bytes(rendered) }, expected.as_bytes());
        }
    }

    #[test]
    fn budget_exhaustion_is_not_flattened_into_a_miss() {
        let mut ctx = Context::new();
        ctx.set_regex_budget(100);
        let pattern = string(&mut ctx, "(a+)+$");
        let flags = string(&mut ctx, "");
        let regex = new(&mut ctx, pattern, flags, 0);
        let subject = string(&mut ctx, "aaaaaaaaaaaaaaaaaaaaaaaaab");
        assert_eq!(test(&mut ctx, regex, subject, 7), 0);
        assert_eq!(
            ctx.trap_record().map(|record| record.kind),
            Some(TrapKind::RegexBudget)
        );
    }

    #[test]
    fn replace_all_traps_when_a_later_search_exhausts_its_budget() {
        let mut ctx = Context::new();
        ctx.set_regex_budget(100);
        let regex = regex(&mut ctx, r"b|(a+)+$", "g");
        let subject_text = format!("bb{}!", "a".repeat(128));
        let (compiled, _) = matcher(&mut ctx, regex, 0).expect("registered regex");
        assert!(matches!(
            find_from(&compiled, &subject_text, 0, ctx.regex_budget()),
            Ok(Some(found)) if found.range == (0..1)
        ));
        assert!(matches!(
            find_from(&compiled, &subject_text, 1, ctx.regex_budget()),
            Ok(Some(found)) if found.range == (1..2)
        ));
        assert!(matches!(
            find_from(&compiled, &subject_text, 2, ctx.regex_budget()),
            Err(regress::BudgetExhausted)
        ));

        let subject = string(&mut ctx, &subject_text);
        let replacement = string(&mut ctx, "Z");
        let result = replace_all(&mut ctx, subject, regex, replacement, 9);
        assert!(result.is_null(), "must not return a partially replaced string");
        assert_eq!(
            ctx.trap_record().map(|record| record.kind),
            Some(TrapKind::RegexBudget)
        );
    }
}
