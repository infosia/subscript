//! compiler.md §326 rule 4: fixed fields keep their element pass.
use subscript_bindgen::generate_for_header;

#[test]
fn a_fixed_struct_array_reaches_string_fields() {
    let header = "#include <stdint.h>\n#include <stddef.h>\ntypedef struct View { const char *data; size_t length; } View;\ntypedef struct S { int32_t x; View name; } S;\ntypedef struct B { int32_t k; S ss[2]; } B;\nvoid fill(B *b);";
    let mirror = generate_for_header(header, "array.h").expect("fixed struct array");
    assert!(mirror.contains("ss: FixedArray<S, 2>"));
}

#[test]
fn an_array_of_pointers_has_no_element_pass() {
    let header = "#include <stdint.h>\ntypedef struct B { int32_t *qs[2]; } B;\nvoid fill(B *b);";
    let error = generate_for_header(header, "array.h").expect_err("pointer array field");
    let text = error.to_string();
    assert!(text.contains("qs"), "{text}");
    assert!(text.contains("array of pointers"), "{text}");
}

#[test]
fn callback_and_descriptor_arrays_have_no_element_pass() {
    for (declaration, element) in [
        (
            "typedef void (*Callback)(View message, void *first, void *second);",
            "Callback",
        ),
        (
            "typedef struct Pair { const int32_t *items; size_t count; } Pair;",
            "Pair",
        ),
    ] {
        let header = format!("#include <stdint.h>\n#include <stddef.h>\ntypedef struct View {{ const char *data; size_t length; }} View;\n{declaration}\ntypedef struct B {{ {element} values[2]; View label; }} B;\nvoid fill(B *b);");
        let error =
            generate_for_header(&header, "array.h").expect_err("unsupported fixed array field");
        let text = error.to_string();
        assert!(text.contains("field `values`"), "{text}");
        assert!(text.contains("has no write-direction pass"), "{text}");
    }
}

/// compiler.md §326 rule 4: an unused host struct does not declare a boundary position.
#[test]
fn an_unused_callback_array_does_not_reject_the_header() {
    let header = "#include <stdint.h>\n#include <stddef.h>\ntypedef struct View { const char *data; size_t length; } View;\ntypedef void (*Cb)(View message, void *first, void *second);\ntypedef struct Hooks { Cb cbs[2]; int32_t x; } Hooks;\ntypedef struct P { int32_t x; } P;\nvoid f(P *p);";
    let mirror = generate_for_header(header, "hooks.h").expect("unused callback array");
    assert!(mirror.contains("declare function f(p: P | null): void;"));
}

/// compiler.md §326 rule 4: every reached fixed field needs an element pass.
#[test]
fn a_reached_callback_array_names_the_field_with_no_pass() {
    for position in [
        "void f(Hooks *hooks);",
        "void f(const Hooks *hooks);",
        "void f(Hooks hooks);",
        "void f(Hooks hooks[2]);",
        "Hooks f(void);",
        "const Hooks *f(void);",
        "typedef struct Outer { Hooks hooks; } Outer; void f(Outer *out);",
        "typedef struct Outer { Hooks hooks[2]; } Outer; void f(Outer *out);",
        "typedef struct Outer { Hooks *hooks; } Outer; void f(Outer *out);",
    ] {
        let header = format!("#include <stdint.h>\n#include <stddef.h>\ntypedef struct View {{ const char *data; size_t length; }} View;\ntypedef void (*Cb)(View message, void *first, void *second);\ntypedef struct Hooks {{ Cb cbs[2]; int32_t x; }} Hooks;\n{position}");
        let error = generate_for_header(&header, "hooks.h").expect_err("reached callback array");
        let text = error.to_string();
        assert!(
            text.contains("struct `Hooks` field `cbs`"),
            "{position}: {text}"
        );
        assert!(
            text.contains("has no write-direction pass"),
            "{position}: {text}"
        );
    }
}
