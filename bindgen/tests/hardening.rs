//! Emitter hardening, end to end through the libclang frontend
//! (`specs/blocks/compiler.md` §13.2). Raw C builtins map to the sized
//! numerics on the LP64 target; any base spelling that is neither a mapped
//! scalar/builtin nor a registered named type, or a record layout the
//! language cannot reproduce, makes `generate()` return an `Err` naming the
//! offending construct — never a literal in the mirror, never a panic.

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
    assert!(
        err.to_string().contains("long"),
        "message names long: {err}"
    );
}

#[test]
fn double_pointer_field_fails_loud() {
    let header = "typedef struct T { int x; } T; \
                  typedef struct U { const T **pp; } U;";
    let err = generate(header).expect_err("a double pointer must fail loud");
    assert!(
        err.to_string().contains('T'),
        "message names the type: {err}"
    );
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
fn narrow_c_scalars_map_but_plain_char_fails_without_an_explicit_target() {
    let header = "#include <stdint.h>\ntypedef struct W { int8_t a; uint8_t b; signed char c; \
                  unsigned char d; int16_t e; uint16_t f; short g; \
                  unsigned short h; char target_char; _Float16 half; } W;";
    let err = generate(header).expect_err("plain char must not follow the host target");
    assert_eq!(
        err.to_string(),
        "bindgen: plain `char` has target-dependent signedness; bindgen does not infer it from the host, so use an explicit `signed char` or `unsigned char` spelling"
    );

    let header = "#include <stdint.h>\ntypedef struct W { int8_t a; uint8_t b; signed char c; \
                  unsigned char d; int16_t e; uint16_t f; short g; \
                  unsigned short h; _Float16 half; } W;";
    let mirror = generate(header).expect("unambiguous narrow scalars map");
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
        assert!(
            mirror.contains(expected),
            "missing `{expected}` in:\n{mirror}"
        );
    }
}

#[test]
fn typedefed_binary16_float_maps_to_f16() {
    let header = "typedef _Float16 SubHalf; typedef struct W { SubHalf half; } W;";
    let mirror = generate(header).expect("typedefed binary16 maps");
    assert!(mirror.contains("type SubHalf = f16;"), "{mirror}");
    assert!(mirror.contains("half: SubHalf;"), "{mirror}");
}

#[test]
fn bitfield_record_fails_loud() {
    let header = "#include <stdint.h>\ntypedef struct W { uint8_t a : 3; uint8_t b : 5; } W;";
    let err = generate(header).expect_err("bitfields cannot be mirrored");
    assert_eq!(
        err.to_string(),
        "bindgen: record `W` contains bitfield member `a`; the language cannot reproduce bitfield layout"
    );
}

#[test]
fn union_record_fails_loud() {
    let header = "#include <stdint.h>\ntypedef union U { uint8_t a; uint16_t b; } U;";
    let err = generate(header).expect_err("unions cannot be mirrored");
    assert_eq!(
        err.to_string(),
        "bindgen: record `U` is a union; the language cannot reproduce union layout"
    );
}

#[test]
fn packed_record_fails_loud() {
    let header = "#include <stdint.h>\ntypedef struct __attribute__((packed)) P { uint8_t a; _Float16 h; } P;";
    let err = generate(header).expect_err("packed records cannot be mirrored");
    assert_eq!(
        err.to_string(),
        "bindgen: record `P` uses packed layout; the language cannot reproduce its field offsets"
    );
}

#[test]
fn over_aligned_record_fails_loud() {
    let header =
        "#include <stdint.h>\ntypedef struct __attribute__((aligned(16))) A { uint8_t a; } A;";
    let err = generate(header).expect_err("over-aligned records cannot be mirrored");
    assert_eq!(
        err.to_string(),
        "bindgen: record `A` is explicitly aligned to 16 bytes; the language cannot reproduce that alignment"
    );
}

/// compiler.md §136.1 rule 1a: an unsigned alias member prints as the
/// unsigned value of the alias width.
#[test]
fn unsigned_flag_members_print_as_unsigned_values_of_their_width() {
    let header = "#include <stdint.h>\n\
                  typedef uint64_t F; static const F F_ALL = ~(uint64_t)0;\n\
                  typedef uint32_t H; static const H H_ALL = ~(uint32_t)0;\n\
                  typedef uint16_t S; static const S S_ALL = (S)-1;\n\
                  typedef uint8_t B; static const B B_ALL = 255; static const B B_ONE = 1;\n";
    let mirror = generate(header).expect("unsigned flag members bind");
    for expect in [
        "declare const F_ALL = 18446744073709551615;",
        "declare const H_ALL = 4294967295;",
        "declare const S_ALL = 65535;",
        "declare const B_ALL = 255;",
        "declare const B_ONE = 1;",
    ] {
        assert!(mirror.contains(expect), "missing `{expect}` in:\n{mirror}");
    }
}

/// compiler.md §136.1 rule 1a: a member of a signed alias fails `bind`,
/// and the message names the constant and its alias.
#[test]
fn a_signed_alias_member_fails_bind_naming_the_constant_and_alias() {
    for (header, constant, alias) in [
        (
            "#include <stdint.h>\ntypedef int32_t G; static const G G_NEG = -1;\n",
            "G_NEG",
            "G",
        ),
        (
            "#include <stdint.h>\ntypedef int64_t P; static const P P_ONE = 1;\n",
            "P_ONE",
            "P",
        ),
    ] {
        let error = generate(header).expect_err("a signed alias member fails bind");
        assert_eq!(
            error.to_string(),
            format!(
                "bindgen: `static const` member `{constant}` of alias `{alias}` is outside the mirror \
                 surface: a member needs an unsigned integer alias (u8, u16, u32, u64)"
            ),
        );
    }
}
