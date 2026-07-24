//! Emitter hardening, end to end through the libclang frontend
//! (`specs/blocks/compiler.md` §13.2, the P6.1 review's carried-in
//! requirement). Raw C builtins map to the sized numerics on the LP64
//! target; any base spelling that is neither a mapped scalar/builtin nor a
//! registered named type makes `generate()` return an `Err` naming the
//! offending type — never a literal in the mirror, never a panic.

use subscript_bindgen::generate;

#[test]
fn raw_c_builtins_map_to_sized_numerics() {
    // A header whose fields are raw C builtins (no stdint typedefs).
    let header = "typedef struct SubRaw { int a; unsigned int b; long c; \
                  unsigned long long d; float e; double f; } SubRaw;";
    let mirror = generate(header).expect("raw builtins map cleanly");
    for expect in [
        "a: i32;", "b: u32;", "c: i64;", "d: u64;", "e: f32;", "f: f64;",
    ] {
        assert!(mirror.contains(expect), "missing `{expect}` in:\n{mirror}");
    }
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
fn bare_char_scalar_field_fails_loud() {
    // A standalone `char` scalar has no language type (it is the
    // string-view element only): fail loud, never emit a literal `char`.
    let header = "typedef struct W { char c; } W;";
    let err = generate(header).expect_err("a bare char scalar must fail loud");
    assert!(err.to_string().contains("char"), "message names char: {err}");
}
