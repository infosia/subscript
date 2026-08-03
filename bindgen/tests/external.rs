//! R20 external types supplied by another generated ambient mirror.

use subscript_bindgen::generate_for_header;

#[test]
fn external_type_is_referenced_without_a_declaration_and_records_provenance() {
    // The directive deliberately follows the first use: header comments are
    // collected before libclang parses any declaration.
    let header = r#"
        SubDevice subExternalDeviceIdentity(SubDevice device);
        /* @subscript-external SubDevice */
    "#;
    let mirror = generate_for_header(header, "external.h")
        .expect("an external type may be supplied by another ambient mirror");

    assert!(mirror.contains("// @subscript-c-external type=\"SubDevice\""));
    assert!(mirror.contains(
        "declare function subExternalDeviceIdentity(device: SubDevice): SubDevice;"
    ));
    assert!(!mirror.contains("interface SubDevice"));
    assert!(!mirror.contains("declare class SubDevice"));
    assert!(!mirror.contains("declare enum SubDevice"));
    assert!(!mirror.contains("type SubDevice ="));
}

#[test]
fn unused_external_directive_is_an_error() {
    let error = generate_for_header(
        "/* @subscript-external SubDevice */\nvoid subExternalPing(void);",
        "external.h",
    )
    .expect_err("an external directive that affects no declaration must fail");

    assert_eq!(
        error.0,
        "external type `SubDevice` is not used by any declaration in this header"
    );
}

#[test]
fn external_type_defined_in_the_same_header_is_an_error() {
    let error = generate_for_header(
        "/* @subscript-external SubDevice */\n\
         typedef struct SubDevice_T *SubDevice;\n\
         SubDevice subExternalDeviceIdentity(SubDevice device);",
        "external.h",
    )
    .expect_err("one header cannot own and externalize the same type");

    assert_eq!(
        error.0,
        "external type `SubDevice` is also defined in this header; a type cannot be both external and local"
    );
}

#[test]
fn unmapped_type_diagnostic_names_the_external_directive() {
    let error = generate_for_header(
        "typedef void *Mystery;\nvoid subExternalConsume(Mystery value);",
        "external.h",
    )
    .expect_err("an unmapped pointer alias must fail at its boundary use");

    assert_eq!(
        error.0,
        "unmapped C type `Mystery` at a boundary use site: it is neither a mapped \
         scalar/builtin nor a named type declared by this header. If another \
         ambient mirror declares it, add `/* @subscript-external Mystery */` to \
         this header; refusing to emit an unresolved name otherwise."
    );
}
