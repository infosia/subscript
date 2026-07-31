//! P25 typed ingestion of fixed-shape C provenance records.

use subscript_compiler::hir::{ForeignMirrorId, ForeignTypeProvenance};
use subscript_compiler::{check_program, Diagnostic, SourceFile};

fn reject(name: &str, mirror: &str) -> Vec<Diagnostic> {
    check_program(&[SourceFile::ambient(name, mirror)])
        .expect_err("the malformed mirror must be rejected")
}

fn assert_named(diagnostics: &[Diagnostic], mirror: &str, needle: &str) {
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains(mirror)
                && diagnostic.message.contains(needle)),
        "expected `{mirror}` and `{needle}` in {diagnostics:#?}"
    );
}

#[test]
fn foreign_declarations_without_a_header_record_are_rejected() {
    let diagnostics = reject("missing-header.d.ts", "declare function engineRun(): void;\n");
    assert_named(
        &diagnostics,
        "missing-header.d.ts",
        "@subscript-c-header",
    );
}

#[test]
fn a_malformed_record_is_rejected_with_its_mirror_name() {
    let diagnostics = reject(
        "malformed.d.ts",
        "// @subscript-c-header include=engine.h\n\
         declare function engineRun(): void;\n",
    );
    assert_named(&diagnostics, "malformed.d.ts", "malformed provenance record");
    assert_named(&diagnostics, "malformed.d.ts", "include=engine.h");
}

#[test]
fn a_record_naming_a_nonexistent_parameter_is_rejected() {
    let diagnostics = reject(
        "wrong-parameter.d.ts",
        "// @subscript-c-header include=\"engine.h\"\n\
         // @subscript-c-descriptor function=\"engineRead\" parameter=\"engineMissing\" aggregate=\"EngineItems\" element=\"uint32_t\" const=true\n\
         declare function engineRead(engineItems: u32[]): void;\n",
    );
    assert_named(
        &diagnostics,
        "wrong-parameter.d.ts",
        "engineRead.engineMissing",
    );
    assert_named(
        &diagnostics,
        "wrong-parameter.d.ts",
        "aggregate=\"EngineItems\"",
    );
}

#[test]
fn duplicate_records_for_one_parameter_are_rejected() {
    let diagnostics = reject(
        "duplicate.d.ts",
        "// @subscript-c-header include=\"engine.h\"\n\
         // @subscript-c-string-view function=\"engineName\" parameter=\"engineText\" aggregate=\"EngineText\"\n\
         // @subscript-c-string-view function=\"engineName\" parameter=\"engineText\" aggregate=\"EngineOtherText\"\n\
         declare function engineName(engineText: string): void;\n",
    );
    assert_named(&diagnostics, "duplicate.d.ts", "duplicate provenance");
    assert_named(&diagnostics, "duplicate.d.ts", "EngineOtherText");
}

#[test]
fn well_formed_records_are_attached_to_the_typed_hir_surface() {
    let mirror = "\
// @subscript-c-header include=\"engine.h\"
// @subscript-c-descriptor function=\"engineUse\" parameter=\"engineRead\" aggregate=\"EngineItemView\" element=\"EngineItem\" const=true
// @subscript-c-descriptor function=\"engineUse\" parameter=\"engineWrite\" aggregate=\"EngineItemOut\" element=\"EngineItem\" const=false
// @subscript-c-string-view function=\"engineUse\" parameter=\"engineText\" aggregate=\"EngineStringView\"
// @subscript-c-callback typedef=\"EngineCallback\"
type EngineCallback = (engineMessage: string, engineUserdata1: object | null, engineUserdata2: object | null) => void;
declare class EngineItem {
  engineValue: u32;
  constructor(engineValue: u32);
}

declare class EngineSink {
  engineCallback: EngineCallback;
  engineUserdata1: object | null;
  engineUserdata2: object | null;
  constructor(engineCallback: EngineCallback, engineUserdata1: object | null, engineUserdata2: object | null);
}
declare function engineUse(engineRead: EngineItem[], engineWrite: EngineItem[], engineText: string, engineSink: EngineSink): void;
";
    let module = check_program(&[SourceFile::ambient("engine.generated.d.ts", mirror)])
        .expect("well-formed provenance is ingested");

    assert_eq!(module.foreign_mirrors.len(), 1);
    assert_eq!(
        module.foreign_mirrors[0].source_name,
        "engine.generated.d.ts"
    );
    assert_eq!(module.foreign_mirrors[0].include, "engine.h");

    let function = module
        .foreign_fns
        .iter()
        .find(|function| function.name == "engineUse")
        .expect("foreign function");
    assert_eq!(function.mirror, ForeignMirrorId(0));
    assert_eq!(
        function.params[0].foreign_provenance,
        Some(ForeignTypeProvenance::Descriptor {
            aggregate: "EngineItemView".to_string(),
            element: "EngineItem".to_string(),
            element_const: true,
        })
    );
    assert_eq!(
        function.params[1].foreign_provenance,
        Some(ForeignTypeProvenance::Descriptor {
            aggregate: "EngineItemOut".to_string(),
            element: "EngineItem".to_string(),
            element_const: false,
        })
    );
    assert_eq!(
        function.params[2].foreign_provenance,
        Some(ForeignTypeProvenance::StringView {
            aggregate: "EngineStringView".to_string(),
        })
    );
    assert_eq!(function.params[3].foreign_provenance, None);

    let sink = module
        .classes
        .iter()
        .find(|class| class.name == "EngineSink")
        .expect("callback registration struct");
    assert_eq!(
        sink.fields[0].foreign_provenance,
        Some(ForeignTypeProvenance::Callback {
            typedef_name: "EngineCallback".to_string(),
        })
    );
}

#[test]
fn scalar_parameter_pair_record_is_attached_to_the_typed_hir_surface() {
    let mirror = "\
// @subscript-c-header include=\"bytes.h\"
// @subscript-c-scalar-pair function=\"engineFillBytes\" parameter=\"engineData\" element=\"uint8_t\" const=false
declare function engineFillBytes(engineData: u8[]): void;
";
    let module = check_program(&[SourceFile::ambient("bytes.generated.d.ts", mirror)])
        .expect("well-formed scalar-pair provenance is ingested");
    let function = module
        .foreign_fns
        .iter()
        .find(|function| function.name == "engineFillBytes")
        .expect("scalar-pair foreign function");
    assert_eq!(
        function.params[0].foreign_provenance,
        Some(ForeignTypeProvenance::ScalarPair {
            element: "uint8_t".to_string(),
            element_const: false,
        })
    );
}

#[test]
fn foreign_free_ambient_source_needs_no_provenance() {
    let module = check_program(&[SourceFile::ambient(
        "types.d.ts",
        "declare class EngineValue {\n\
         \x20 engineValue: u32;\n\
         \x20 constructor(engineValue: u32);\n\
         }\n",
    )])
    .expect("a foreign-free ambient source needs no records");
    assert!(module.foreign_fns.is_empty());
    assert!(module.foreign_mirrors.is_empty());
}

#[test]
fn absorbed_parameters_without_records_are_rejected() {
    let diagnostics = reject(
        "missing-parameter-records.d.ts",
        "// @subscript-c-header include=\"engine.h\"\n\
         declare function engineUse(engineItems: u32[], engineText: string): void;\n",
    );
    assert_named(
        &diagnostics,
        "missing-parameter-records.d.ts",
        "engineUse.engineItems",
    );
    assert_named(
        &diagnostics,
        "missing-parameter-records.d.ts",
        "@subscript-c-descriptor",
    );
    assert_named(
        &diagnostics,
        "missing-parameter-records.d.ts",
        "engineUse.engineText",
    );
    assert_named(
        &diagnostics,
        "missing-parameter-records.d.ts",
        "@subscript-c-string-view",
    );
}

#[test]
fn kind_incompatible_parameter_record_is_rejected() {
    let diagnostics = reject(
        "wrong-record-kind.d.ts",
        "// @subscript-c-header include=\"engine.h\"\n\
         // @subscript-c-string-view function=\"engineUse\" parameter=\"engineItems\" aggregate=\"EngineText\"\n\
         declare function engineUse(engineItems: u32[]): void;\n",
    );
    assert_named(
        &diagnostics,
        "wrong-record-kind.d.ts",
        "incompatible with parameter `engineUse.engineItems`",
    );
    assert_named(
        &diagnostics,
        "wrong-record-kind.d.ts",
        "aggregate=\"EngineText\"",
    );
}

#[test]
fn foreign_string_view_return_is_rejected() {
    let diagnostics = reject(
        "string-return.d.ts",
        "// @subscript-c-header include=\"engine.h\"\n\
         declare function engineWorldName(): string;\n",
    );
    assert_named(
        &diagnostics,
        "string-return.d.ts",
        "foreign function `engineWorldName` returns a string view",
    );
    assert_named(
        &diagnostics,
        "string-return.d.ts",
        "return provenance cannot be represented",
    );
}

#[test]
fn foreign_array_descriptor_return_is_rejected() {
    let diagnostics = reject(
        "array-return.d.ts",
        "// @subscript-c-header include=\"engine.h\"\n\
         declare function engineWorldEntities(): u32[];\n",
    );
    assert_named(
        &diagnostics,
        "array-return.d.ts",
        "foreign function `engineWorldEntities` returns an array descriptor",
    );
    assert_named(
        &diagnostics,
        "array-return.d.ts",
        "return provenance cannot be represented",
    );
}

#[test]
fn foreign_direct_callback_parameter_is_rejected() {
    let diagnostics = reject(
        "callback-parameter.d.ts",
        "// @subscript-c-header include=\"engine.h\"\n\
         // @subscript-c-callback typedef=\"EngineDone\"\n\
         type EngineDone = (message: string, userdata1: object | null, userdata2: object | null) => void;\n\
         declare function engineInstall(callback: EngineDone): void;\n",
    );
    assert_named(
        &diagnostics,
        "callback-parameter.d.ts",
        "parameter `callback` is a direct callback",
    );
}

#[test]
fn foreign_direct_callback_return_is_rejected() {
    let diagnostics = reject(
        "callback-return.d.ts",
        "// @subscript-c-header include=\"engine.h\"\n\
         // @subscript-c-callback typedef=\"EngineDone\"\n\
         type EngineDone = (message: string, userdata1: object | null, userdata2: object | null) => void;\n\
         declare function engineCallback(): EngineDone;\n",
    );
    assert_named(
        &diagnostics,
        "callback-return.d.ts",
        "foreign function `engineCallback` returns a direct callback",
    );
}
