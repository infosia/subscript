//! Emitter hardening, end to end through the libclang frontend
//! (`specs/blocks/compiler.md` §13.2, the P6.1 review's carried-in
//! requirement). Raw C builtins map to the sized numerics on the LP64
//! target; any base spelling that is neither a mapped scalar/builtin nor a
//! registered named type makes `generate()` return an `Err` naming the
//! offending type — never a literal in the mirror, never a panic.

use subscript_bindgen::generate;

#[test]
fn raw_c_builtins_map_to_sized_numerics() {
    // A header whose fields are the width-stable raw C builtins (no stdint
    // typedefs). `long`/`unsigned long` are intentionally excluded (LP64 vs
    // LLP64), so a 64-bit int is spelled `long long` here.
    let header = "typedef struct SubRaw { int a; unsigned int b; long long c; \
                  unsigned long long d; float e; double f; } SubRaw;";
    let mirror = generate(header).expect("raw builtins map cleanly");
    for expect in [
        "a: i32;", "b: u32;", "c: i64;", "d: u64;", "e: f32;", "f: f64;",
    ] {
        assert!(mirror.contains(expect), "missing `{expect}` in:\n{mirror}");
    }
}

#[test]
fn bare_long_is_target_dependent_and_fails_loud() {
    // `long` is 64-bit on LP64 but 32-bit on LLP64 (Windows): dropped from
    // the builtin map so it cannot mirror a target-dependent width.
    let header = "typedef struct SubHasLong { long n; } SubHasLong;";
    let err = generate(header).expect_err("bare long must fail loud");
    assert!(err.to_string().contains("long"), "message names long: {err}");
}

#[test]
fn double_pointer_field_fails_loud() {
    let header = "typedef struct T { int x; } T; \
                  typedef struct U { const T **pp; } U;";
    let err = generate(header).expect_err("a double pointer must fail loud");
    assert!(err.to_string().contains('T'), "message names the type: {err}");
    assert!(
        err.to_string().contains("unmapped"),
        "clean unmapped-type error: {err}"
    );
}

#[test]
fn anonymous_inline_struct_field_fails_loud() {
    let header = "typedef struct V { struct { int x; } inner; } V;";
    let err = generate(header).expect_err("an anonymous struct must fail loud");
    assert!(
        err.to_string().contains("unnamed") || err.to_string().contains("anonymous"),
        "message names the anonymous type: {err}"
    );
}

#[test]
fn narrow_c_scalars_and_target_resolved_plain_char_map() {
    let header = "#include <stdint.h>\ntypedef struct W { int8_t a; uint8_t b; signed char c; \
                  unsigned char d; int16_t e; uint16_t f; short g; \
                  unsigned short h; char target_char; _Float16 half; } W;";
    let mirror = generate(header).expect("narrow scalars map");
    for expected in [
        "a: i8;",
        "b: u8;",
        "c: i8;",
        "d: u8;",
        "e: i16;",
        "f: u16;",
        "g: i16;",
        "h: u16;",
        "half: f16;",
    ] {
        assert!(mirror.contains(expected), "missing `{expected}` in:\n{mirror}");
    }
    assert!(
        mirror.contains("target_char: i8;") || mirror.contains("target_char: u8;"),
        "plain char follows libclang's known target signedness:\n{mirror}"
    );
}

#[test]
fn typedefed_binary16_float_maps_to_f16() {
    let header =
        "typedef _Float16 SubHalf; typedef struct W { SubHalf half; } W;";
    let mirror = generate(header).expect("typedefed binary16 maps");
    assert!(mirror.contains("type SubHalf = f16;"), "{mirror}");
    assert!(mirror.contains("half: SubHalf;"), "{mirror}");
}
