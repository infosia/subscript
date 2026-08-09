//! R24 CEnum alias references emitted from header-owned typedef spellings.

use subscript_bindgen::generate_for_header;

fn reject(header: &str) -> String {
    generate_for_header(header, "engine.h")
        .expect_err("the invalid @subscript-cenum header must fail")
        .0
}

#[test]
fn scalar_and_enum_typedefs_reference_aliases_without_declaring_them() {
    let header = r#"
        #include <stdint.h>
        typedef int32_t EngineModeC;
        /* @subscript-cenum EngineModeC EngineMode */

        typedef enum EngineFlavorC {
            ENGINE_FLAVOR_COLD = 16,
            ENGINE_FLAVOR_WARM = 23,
        } EngineFlavorC;
        /* @subscript-cenum EngineFlavorC EngineFlavor */

        EngineModeC engineModeNext(void);
        int32_t engineModeEcho(EngineModeC value);
        EngineFlavorC engineFlavorNext(void);
        int32_t engineFlavorEcho(EngineFlavorC value);
    "#;
    let mirror = generate_for_header(header, "engine.h").expect("both CEnum bases bind");

    assert!(mirror.contains(
        "// @subscript-c-cenum typedef=\"EngineModeC\" alias=\"EngineMode\""
    ));
    assert!(mirror.contains(
        "// @subscript-c-cenum typedef=\"EngineFlavorC\" alias=\"EngineFlavor\""
    ));
    assert!(mirror.contains("declare function engineModeNext(): EngineMode;"));
    assert!(mirror.contains("declare function engineModeEcho(value: EngineMode): i32;"));
    assert!(mirror.contains("declare function engineFlavorNext(): EngineFlavor;"));
    assert!(mirror.contains("declare function engineFlavorEcho(value: EngineFlavor): i32;"));
    for forbidden in [
        "type EngineMode =",
        "type EngineModeC =",
        "declare enum EngineFlavor",
        "declare enum EngineFlavorC",
    ] {
        assert!(!mirror.contains(forbidden), "unexpected `{forbidden}` in:\n{mirror}");
    }
}

#[test]
fn missing_named_typedef_is_an_error_naming_the_directive_site() {
    let error = reject("/* @subscript-cenum EngineMissing EngineMode */\nvoid engineRun(void);");
    assert_eq!(
        error,
        "`@subscript-cenum` names typedef `EngineMissing`, but no such typedef is declared in this header"
    );
}

#[test]
fn non_int32_non_enum_base_is_an_error_naming_the_typedef() {
    let error = reject(
        "#include <stdint.h>\n\
         typedef uint32_t EngineModeC;\n\
         /* @subscript-cenum EngineModeC EngineMode */\n\
         EngineModeC engineModeNext(void);",
    );
    assert_eq!(
        error,
        "`@subscript-cenum` typedef `EngineModeC` has base `uint32_t`; expected exactly `int32_t` or an enum typedef"
    );
}

#[test]
fn zero_direct_function_uses_is_an_error_naming_the_typedef() {
    let error = reject(
        "#include <stdint.h>\n\
         typedef int32_t EngineModeC;\n\
         /* @subscript-cenum EngineModeC EngineMode */\n\
         void engineRun(void);",
    );
    assert_eq!(
        error,
        "`@subscript-cenum` typedef `EngineModeC` has zero uses in supported boundary positions"
    );
}

#[test]
fn direct_struct_member_emits_the_ambient_alias() {
    let mirror = generate_for_header(
        "#include <stdint.h>\n\
         typedef int32_t EngineModeC;\n\
         /* @subscript-cenum EngineModeC EngineMode */\n\
         typedef struct EngineState { EngineModeC mode; } EngineState;\n\
         EngineModeC engineModeNext(void);",
        "engine.h",
    )
    .expect("§52 lifts the direct struct-member restriction");
    assert!(mirror.contains("  mode: EngineMode;"), "{mirror}");
    assert!(mirror.contains("  constructor(mode: EngineMode);"), "{mirror}");
}

#[test]
fn recognized_standalone_and_embedded_pairs_emit_alias_arrays() {
    let mirror = generate_for_header(
        "#include <stddef.h>\n\
         #include <stdint.h>\n\
         typedef int32_t EngineModeC;\n\
         /* @subscript-cenum EngineModeC EngineMode */\n\
         typedef struct EngineModeSlice { const EngineModeC *data; size_t count; } EngineModeSlice;\n\
         typedef struct EngineState { size_t modesCount; const EngineModeC *modes; int32_t tag; } EngineState;\n\
         int32_t engineModesFirst(EngineModeSlice values);\n\
         int32_t engineStateFirst(const EngineState *state);",
        "engine.h",
    )
    .expect("§52 maps recognized CEnum pairs to alias arrays");
    assert!(mirror.contains("declare function engineModesFirst(values: EngineMode[]): i32;"), "{mirror}");
    assert!(mirror.contains("  modes: EngineMode[];"), "{mirror}");
    assert!(mirror.contains("  constructor(modes: EngineMode[], tag: i32);"), "{mirror}");
}

#[test]
fn pointer_target_use_is_an_error_naming_the_parameter() {
    let error = reject(
        "#include <stdint.h>\n\
         typedef int32_t EngineModeC;\n\
         /* @subscript-cenum EngineModeC EngineMode */\n\
         EngineModeC engineModeNext(void);\n\
         void engineModeWrite(EngineModeC *mode);",
    );
    assert_eq!(
        error,
        "`@subscript-cenum` typedef `EngineModeC` is used at foreign function `engineModeWrite` parameter `mode` pointer target; supported uses are direct bound-function parameters/returns, direct struct members, and recognized array-pair elements"
    );
}

#[test]
fn array_element_use_is_an_error_naming_the_member() {
    let error = reject(
        "#include <stdint.h>\n\
         typedef int32_t EngineModeC;\n\
         /* @subscript-cenum EngineModeC EngineMode */\n\
         typedef struct EngineModes { EngineModeC current; EngineModeC modes[2]; } EngineModes;\n\
         EngineModeC engineModeNext(void);",
    );
    assert_eq!(
        error,
        "`@subscript-cenum` typedef `EngineModeC` is used at struct `EngineModes` member `modes` array element; supported uses are direct bound-function parameters/returns, direct struct members, and recognized array-pair elements"
    );
}

#[test]
fn another_typedef_base_use_is_an_error_naming_that_typedef() {
    let error = reject(
        "#include <stdint.h>\n\
         typedef int32_t EngineModeC;\n\
         typedef EngineModeC EngineOtherModeC;\n\
         /* @subscript-cenum EngineModeC EngineMode */\n\
         EngineModeC engineModeNext(void);",
    );
    assert_eq!(
        error,
        "`@subscript-cenum` typedef `EngineModeC` is used at typedef `EngineOtherModeC` base; supported uses are direct bound-function parameters/returns, direct struct members, and recognized array-pair elements"
    );
}

#[test]
fn alias_collision_with_a_header_declaration_names_both_sites() {
    let error = reject(
        "#include <stdint.h>\n\
         typedef int32_t EngineModeC;\n\
         /* @subscript-cenum EngineModeC engineRun */\n\
         EngineModeC engineModeNext(void);\n\
         void engineRun(void);",
    );
    assert_eq!(
        error,
        "`@subscript-cenum` alias `engineRun` for typedef `EngineModeC` collides with header function `engineRun`"
    );
}

#[test]
fn alias_collision_with_a_bind_emitted_name_names_the_generated_site() {
    let error = reject(
        "#include <stdint.h>\n\
         typedef int32_t EngineModeC;\n\
         /* @subscript-cenum EngineModeC constructor */\n\
         typedef struct EngineState { int32_t value; } EngineState;\n\
         EngineModeC engineModeNext(void);",
    );
    assert_eq!(
        error,
        "`@subscript-cenum` alias `constructor` for typedef `EngineModeC` collides with bind-emitted constructor for struct `EngineState`"
    );
}

#[test]
fn duplicate_directive_for_the_same_typedef_is_an_error() {
    let error = reject(
        "#include <stdint.h>\n\
         typedef int32_t EngineModeC;\n\
         /* @subscript-cenum EngineModeC EngineMode */\n\
         /* @subscript-cenum EngineModeC EngineOtherMode */\n\
         EngineModeC engineModeNext(void);",
    );
    assert_eq!(
        error,
        "duplicate `@subscript-cenum` directive for typedef `EngineModeC`"
    );
}
