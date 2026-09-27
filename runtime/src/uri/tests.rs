use super::*;
use crate::{ffi, Context};

#[test]
fn encode_sets_and_unicode() {
    let common = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_.!~*'()";
    for component in [false, true] {
        assert_eq!(encode(common, component), common);
        assert_eq!(
            encode("héllo 😀\u{2028}%".as_bytes(), component),
            b"h%C3%A9llo%20%F0%9F%98%80%E2%80%A8%25"
        );
        assert_eq!(encode(b"", component), b"");
    }
    assert_eq!(encode(RESERVED, false), RESERVED);
    assert_eq!(encode(RESERVED, true), b"%3B%2C%2F%3F%3A%40%26%3D%2B%24%23");
}

#[test]
fn decode_success_and_preserved_escape_case() {
    for component in [false, true] {
        assert_eq!(
            decode(b"a%20b%E2%82%AC%F0%9F%98%80", component),
            Ok("a b€😀".as_bytes().to_vec())
        );
        assert_eq!(
            decode("héllo".as_bytes(), component),
            Ok("héllo".as_bytes().to_vec())
        );
        assert_eq!(decode(b"%00%25", component), Ok(vec![0, b'%']));
    }
    assert_eq!(decode(b"%3b%2f%3F", false), Ok(b"%3b%2f%3F".to_vec()));
    assert_eq!(decode(b"%3b%2f%3F", true), Ok(b";/?".to_vec()));
}

#[test]
fn malformed_classes_and_byte_offsets() {
    for input in [
        "%",
        "%0",
        "%zz",
        "%C3",
        "%ED%A0%80",
        "%C0%80",
        "%FF",
        "%80",
        "%E2%82",
        "%E2%28%A1",
        "%F4%90%80%80",
        "%F5%80%80%80",
        "%E0%80%80",
        "%F0%80%80%80",
        "%C3é",
    ] {
        for component in [false, true] {
            assert_eq!(decode(input.as_bytes(), component), Err(0), "{input}");
            let input = format!("é%20{input}");
            assert_eq!(decode(input.as_bytes(), component), Err(5));
            let name = if component {
                "decodeURIComponent"
            } else {
                "decodeURI"
            };
            assert_eq!(
                failure(input.as_bytes(), component),
                format!("{name}: malformed escape at byte 5")
            );
        }
    }
    assert_eq!(failure(b"%C3%A9", true), "");
}

#[test]
fn ffi_entries_transform_and_format() {
    let mut ctx = Context::new();
    // SAFETY: all handles are live strings in this exclusive Context.
    unsafe {
        let input = ctx.alloc_str(b"a /", 0);
        let component = ffi::subscript_rt_encode_uri_component(&mut *ctx, input, 0);
        assert_eq!(ctx.str_bytes(component), b"a%20%2F");
        let uri = ffi::subscript_rt_encode_uri(&mut *ctx, input, 0);
        assert_eq!(ctx.str_bytes(uri), b"a%20/");
        let decoded = ffi::subscript_rt_decode_uri_component(&mut *ctx, component, 0);
        assert_eq!(ctx.str_bytes(decoded), b"a /");
        let decoded = ffi::subscript_rt_decode_uri(&mut *ctx, component, 0);
        assert_eq!(ctx.str_bytes(decoded), b"a %2F");
        let valid = ffi::subscript_rt_decode_uri_failure(&mut *ctx, uri, 0);
        assert_eq!(ctx.str_bytes(valid), b"");
        let valid = ffi::subscript_rt_decode_uri_component_failure(&mut *ctx, uri, 0);
        assert_eq!(ctx.str_bytes(valid), b"");
        let bad = ctx.alloc_str("é%FF".as_bytes(), 0);
        let failure = ffi::subscript_rt_decode_uri_failure(&mut *ctx, bad, 0);
        assert_eq!(
            ctx.str_bytes(failure),
            b"decodeURI: malformed escape at byte 2"
        );
        let failure = ffi::subscript_rt_decode_uri_component_failure(&mut *ctx, bad, 0);
        assert_eq!(
            ctx.str_bytes(failure),
            b"decodeURIComponent: malformed escape at byte 2"
        );
        for (name, message, expected) in [
            ("RangeError", "m", "RangeError: m"),
            ("", "x", "x"),
            ("N", "", "N"),
            ("", "", ""),
        ] {
            let name = ctx.alloc_str(name.as_bytes(), 0);
            let message = ctx.alloc_str(message.as_bytes(), 0);
            let result = ffi::subscript_rt_error_to_string(&mut *ctx, name, message, 0);
            assert_eq!(ctx.str_bytes(result), expected.as_bytes());
        }
    }
}
