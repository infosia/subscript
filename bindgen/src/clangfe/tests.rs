use super::*;

#[test]
fn records_with_prior_declarations_keep_their_fields() {
    let preamble = "#include <stdint.h>\n#include <stddef.h>\n\
                    typedef struct Obj { int32_t v; } Obj;\n";
    for declarations in [
        "typedef struct A { int32_t k; struct B *b; } A; \
         typedef struct B { int32_t k; Obj *a; } B; void use(A a);",
        "struct B; typedef struct A { int32_t k; struct B *b; } A; \
         typedef struct B { int32_t k; int64_t a; } B; void use(A a);",
    ] {
        let header = format!("{preamble}{declarations}");
        let parsed = parse(&header).expect(declarations);
        let fields = parsed
            .decls
            .iter()
            .find_map(|decl| match decl {
                Decl::Struct { name, fields } if name == "B" => Some(fields),
                _ => None,
            })
            .expect("B must be a struct");
        assert_eq!(
            fields
                .iter()
                .map(|field| field.name.as_str())
                .collect::<Vec<_>>(),
            ["k", "a"],
            "{declarations}"
        );
        crate::generate_for_header(&header, "h.h").expect(declarations);
    }
}

#[test]
fn enum_with_a_prior_declaration_keeps_its_members() {
    let parsed =
        parse("enum E; typedef enum E E; enum E { E_FIRST = 3, E_SECOND = 7 }; void use(E e);")
            .expect("enum definition after its typedef");
    let members = parsed
        .decls
        .iter()
        .find_map(|decl| match decl {
            Decl::Enum { name, members } if name == "E" => Some(members),
            _ => None,
        })
        .expect("E must be an enum");
    assert_eq!(members, &[("E_FIRST".into(), 3), ("E_SECOND".into(), 7)]);
}

#[test]
fn record_without_a_definition_stays_an_opaque_handle() {
    let parsed = parse("typedef struct X *X; void use(X x);").expect("opaque handle");
    assert!(parsed
        .decls
        .iter()
        .any(|decl| matches!(decl, Decl::Handle { name } if name == "X")));
    crate::generate_for_header("typedef struct X *X; void use(X x);", "h.h")
        .expect("opaque handle mirror");
}

#[test]
fn handle_with_a_prior_declaration_checks_the_record_definition() {
    let parsed = parse("struct X; typedef struct X *X; struct X { int k; void *a; };")
        .expect("handle with a record definition");
    assert!(parsed
        .decls
        .iter()
        .any(|decl| matches!(decl, Decl::Handle { name } if name == "X")));

    let error = parse("struct X; typedef struct X *X; struct X { unsigned int k : 1; };")
        .expect_err("record definition with a bitfield");
    assert!(
        error.0.contains("contains bitfield member `k`"),
        "{}",
        error.0
    );
}
