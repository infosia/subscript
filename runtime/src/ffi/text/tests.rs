//! Unit tests of the text functions, with the Unicode 17.0 conformance
//! data of UAX #29 (`GraphemeBreakTest-17.0.0.txt`) and the NFC columns
//! of UAX #15 (`NormalizationTest-17.0.0.txt`). The two data files sit
//! beside this file. The eight tests run in the runtime library test
//! binary: about 0.04 s in release and 0.4 s in debug.

use super::*;

const GRAPHEME_TEST: &str = include_str!("GraphemeBreakTest-17.0.0.txt");
const NORMALIZATION_TEST: &str = include_str!("NormalizationTest-17.0.0-nfc.txt");

fn scalar(hex: &str) -> char {
    char::from_u32(u32::from_str_radix(hex, 16).unwrap()).unwrap()
}

fn hex_text(field: &str) -> String {
    field.split_whitespace().map(scalar).collect()
}

/// The clusters of one GraphemeBreakTest line: `÷` is a break and `×`
/// is no break.
fn grapheme_case(line: &str) -> Option<(String, Vec<String>)> {
    let data = line.split('#').next()?.trim();
    if data.is_empty() {
        return None;
    }
    let mut clusters = Vec::new();
    let mut current = String::new();
    for token in data.split_whitespace() {
        match token {
            "÷" => {
                if !current.is_empty() {
                    clusters.push(std::mem::take(&mut current));
                }
            }
            "×" => {}
            hex => current.push(scalar(hex)),
        }
    }
    Some((clusters.concat(), clusters))
}

/// The receiver must outlive the call; one Context holds every string.
fn string(ctx: &mut Context, text: &str) -> *mut u8 {
    ctx.alloc_str(text.as_bytes(), 0)
}

/// The text of a result handle.
fn read(ctx: &Context, handle: *const u8) -> String {
    assert!(!handle.is_null());
    // SAFETY: a live handle of `ctx`.
    String::from_utf8(unsafe { ctx.str_bytes(handle) }.to_vec()).unwrap()
}

#[test]
fn grapheme_functions_match_the_uax29_test_data() {
    let mut ctx = Context::new();
    let mut cases = 0;
    for (text, clusters) in GRAPHEME_TEST.lines().filter_map(grapheme_case) {
        let s = string(&mut ctx, &text);
        let p: *mut Context = &mut *ctx;
        // SAFETY: valid Context and live handles.
        unsafe {
            assert_eq!(
                subscript_rt_str_grapheme_length(p, s),
                clusters.len() as i32,
                "{text:?}"
            );
            for (index, cluster) in clusters.iter().enumerate() {
                let start = index as i32;
                let one = subscript_rt_str_slice_graphemes(p, s, start, start + 1, 0);
                assert_eq!(&read(&ctx, one), cluster, "{text:?} cluster {index}");
                let back = subscript_rt_str_slice_graphemes(
                    p,
                    s,
                    start - clusters.len() as i32,
                    i32::MAX,
                    0,
                );
                assert_eq!(
                    read(&ctx, back),
                    clusters[index..].concat(),
                    "{text:?} from {index}"
                );
            }
        }
        cases += 1;
    }
    // The file header states 766 test lines.
    assert_eq!(cases, 766);
}

#[test]
fn normalize_matches_the_uax15_nfc_columns() {
    let mut ctx = Context::new();
    let mut cases = 0;
    for line in NORMALIZATION_TEST
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let columns: Vec<String> = line.split(';').map(hex_text).collect();
        assert_eq!(columns.len(), 5, "{line}");
        for (input, expected) in [(0, 1), (1, 1), (2, 1), (3, 3), (4, 3)] {
            let s = string(&mut ctx, &columns[input]);
            let p: *mut Context = &mut *ctx;
            // SAFETY: valid Context and live handle.
            let result = unsafe { subscript_rt_str_normalize(p, s, 0) };
            assert_eq!(
                read(&ctx, result),
                columns[expected],
                "{line} column {}",
                input + 1
            );
        }
        cases += 1;
        if cases % 2048 == 0 {
            ctx.collect();
        }
    }
    assert_eq!(cases, 20_034);
}

#[test]
fn japanese_and_emoji_clusters() {
    let mut ctx = Context::new();
    let p: *mut Context = &mut *ctx;
    for (text, count) in [
        ("", 0),
        ("abc", 3),
        ("日本語", 3),
        ("𠮷野家", 3),
        ("か\u{3099}", 1),
        (
            "\u{1F469}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}",
            1,
        ),
        ("\u{1F1EF}\u{1F1F5}", 1),
        ("\r\n", 1),
    ] {
        let s = string(&mut ctx, text);
        // SAFETY: valid Context and live handle.
        assert_eq!(
            unsafe { subscript_rt_str_grapheme_length(p, s) },
            count,
            "{text:?}"
        );
    }
}

#[test]
fn slice_graphemes_follows_the_slice_rules() {
    let mut ctx = Context::new();
    let s = string(&mut ctx, "か\u{3099}き𠮷\u{1F1EF}\u{1F1F5}");
    let p: *mut Context = &mut *ctx;
    for (start, end, expected) in [
        (0, i32::MAX, "か\u{3099}き𠮷\u{1F1EF}\u{1F1F5}"),
        (0, 1, "か\u{3099}"),
        (1, 3, "き𠮷"),
        (-1, i32::MAX, "\u{1F1EF}\u{1F1F5}"),
        (-3, -1, "き𠮷"),
        (2, 1, ""),
        (-100, 1, "か\u{3099}"),
        (3, 100, "\u{1F1EF}\u{1F1F5}"),
        (4, i32::MAX, ""),
        (100, 200, ""),
    ] {
        // SAFETY: valid Context and live handle.
        let result = unsafe { subscript_rt_str_slice_graphemes(p, s, start, end, 0) };
        assert_eq!(read(&ctx, result), expected, "({start}, {end})");
    }
}

#[test]
fn ascii_text_takes_the_byte_path_except_for_cr_lf() {
    let mut ctx = Context::new();
    let plain = string(&mut ctx, "abcdef");
    let crlf = string(&mut ctx, "ab\r\ncd\r");
    let p: *mut Context = &mut *ctx;
    // SAFETY: valid Context and live handles.
    unsafe {
        assert_eq!(subscript_rt_str_grapheme_length(p, plain), 6);
        assert_eq!(subscript_rt_str_grapheme_length(p, crlf), 6);
        let slice = subscript_rt_str_slice_graphemes(p, plain, -4, -1, 0);
        assert_eq!(read(&ctx, slice), "cde");
        let slice = subscript_rt_str_slice_graphemes(p, crlf, 1, 4, 0);
        assert_eq!(read(&ctx, slice), "b\r\nc");
        assert_eq!(subscript_rt_str_normalize(p, plain, 0), plain);
    }
}

#[test]
fn count_and_nfc_of_nfc_text_allocate_nothing() {
    let mut ctx = Context::new();
    let s = string(&mut ctx, "が𠮷\u{1F469}\u{200D}\u{1F467}");
    let p: *mut Context = &mut *ctx;
    let before = ctx.live_count();
    // SAFETY: valid Context and live handle.
    unsafe {
        assert_eq!(subscript_rt_str_grapheme_length(p, s), 3);
        assert_eq!(subscript_rt_str_normalize(p, s, 0), s);
        assert_eq!(subscript_rt_str_slice_graphemes(p, s, 0, i32::MAX, 0), s);
    }
    assert_eq!(ctx.live_count(), before);
}

#[test]
fn nfc_of_other_text_and_a_slice_make_one_context_string_each() {
    let mut ctx = Context::new();
    let s = string(&mut ctx, "か\u{3099}き\u{3099}");
    let p: *mut Context = &mut *ctx;
    let before = ctx.live_count();
    // SAFETY: valid Context and live handle.
    let nfc = unsafe { subscript_rt_str_normalize(p, s, 0) };
    assert_eq!(read(&ctx, nfc), "がぎ");
    assert_eq!(ctx.live_count(), before + 1);
    // SAFETY: valid Context and live handle.
    let first = unsafe { subscript_rt_str_slice_graphemes(p, s, 0, 1, 0) };
    assert_eq!(read(&ctx, first), "か\u{3099}");
    assert_eq!(ctx.live_count(), before + 2);
}

#[test]
fn nfc_keeps_a_prefix_and_composes_the_tail() {
    let mut ctx = Context::new();
    let s = string(&mut ctx, "abc日本\u{1E0A}\u{0323}x");
    let p: *mut Context = &mut *ctx;
    // SAFETY: valid Context and live handle.
    let nfc = unsafe { subscript_rt_str_normalize(p, s, 0) };
    assert_eq!(read(&ctx, nfc), "abc日本\u{1E0C}\u{0307}x");
}

#[test]
fn grapheme_functions_read_bytes_that_are_not_utf8_as_empty() {
    let mut ctx = Context::new();
    // "が" followed by a lone continuation byte.
    let s = ctx.alloc_str(&[0xE3, 0x81, 0x8C, 0x80], 0);
    let p: *mut Context = &mut *ctx;
    // SAFETY: valid Context and live handle.
    unsafe {
        assert_eq!(subscript_rt_str_grapheme_length(p, s), 0);
        let slice = subscript_rt_str_slice_graphemes(p, s, 0, i32::MAX, 0);
        assert_eq!(read(&ctx, slice), "");
    }
}
