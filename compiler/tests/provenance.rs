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
    let diagnostics = reject("missing-header.d.ts", "declare function engRun(): void;\n");
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
         declare function engRun(): void;\n",
    );
    assert_named(&diagnostics, "malformed.d.ts", "malformed provenance record");
    assert_named(&diagnostics, "malformed.d.ts", "include=engine.h");
}

#[test]
fn a_record_naming_a_nonexistent_parameter_is_rejected() {
    let diagnostics = reject(
        "wrong-parameter.d.ts",
        "// @subscript-c-header include=\"engine.h\"\n\
         // @subscript-c-descriptor function=\"engRead\" parameter=\"engMissing\" aggregate=\"EngItems\" element=\"uint32_t\" const=true\n\
         declare function engRead(engItems: u32[]): void;\n",
    );
    assert_named(
        &diagnostics,
        "wrong-parameter.d.ts",
        "engRead.engMissing",
    );
    assert_named(
        &diagnostics,
        "wrong-parameter.d.ts",
        "aggregate=\"EngItems\"",
    );
}

#[test]
fn duplicate_records_for_one_parameter_are_rejected() {
    let diagnostics = reject(
        "duplicate.d.ts",
        "// @subscript-c-header include=\"engine.h\"\n\
         // @subscript-c-string-view function=\"engName\" parameter=\"engText\" aggregate=\"EngText\"\n\
         // @subscript-c-string-view function=\"engName\" parameter=\"engText\" aggregate=\"EngOtherText\"\n\
         declare function engName(engText: string): void;\n",
    );
    assert_named(&diagnostics, "duplicate.d.ts", "duplicate provenance");
    assert_named(&diagnostics, "duplicate.d.ts", "EngOtherText");
}

#[test]
fn well_formed_records_are_attached_to_the_typed_hir_surface() {
    let mirror = "\
// @subscript-c-header include=\"engine.h\"
// @subscript-c-descriptor function=\"engUse\" parameter=\"engRead\" aggregate=\"EngItemView\" element=\"EngItem\" const=true
// @subscript-c-descriptor function=\"engUse\" parameter=\"engWrite\" aggregate=\"EngItemOut\" element=\"EngItem\" const=false
// @subscript-c-string-view function=\"engUse\" parameter=\"engText\" aggregate=\"EngStringView\"
// @subscript-c-callback typedef=\"EngCallback\"
type EngCallback = (engMessage: string, engUserdata1: object | null, engUserdata2: object | null) => void;
declare class EngItem {
  engValue: u32;
  constructor(engValue: u32);
}
declare class EngSink {
  engCallback: EngCallback;
  engUserdata1: object | null;
  engUserdata2: object | null;
  constructor(engCallback: EngCallback, engUserdata1: object | null, engUserdata2: object | null);
}
declare function engUse(engRead: EngItem[], engWrite: EngItem[], engText: string, engSink: EngSink): void;
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
        .find(|function| function.name == "engUse")
        .expect("foreign function");
    assert_eq!(function.mirror, ForeignMirrorId(0));
    assert_eq!(
        function.params[0].foreign_provenance,
        Some(ForeignTypeProvenance::Descriptor {
            aggregate: "EngItemView".to_string(),
            element: "EngItem".to_string(),
            element_const: true,
        })
    );
    assert_eq!(
        function.params[1].foreign_provenance,
        Some(ForeignTypeProvenance::Descriptor {
            aggregate: "EngItemOut".to_string(),
            element: "EngItem".to_string(),
            element_const: false,
        })
    );
    assert_eq!(
        function.params[2].foreign_provenance,
        Some(ForeignTypeProvenance::StringView {
            aggregate: "EngStringView".to_string(),
        })
    );
    assert_eq!(function.params[3].foreign_provenance, None);

    let sink = module
        .classes
        .iter()
        .find(|class| class.name == "EngSink")
        .expect("callback registration struct");
    assert_eq!(
        sink.fields[0].foreign_provenance,
        Some(ForeignTypeProvenance::Callback {
            typedef_name: "EngCallback".to_string(),
        })
    );
}

#[test]
fn foreign_free_ambient_source_needs_no_provenance() {
    let module = check_program(&[SourceFile::ambient(
        "types.d.ts",
        "declare class EngValue {\n\
         \x20 engValue: u32;\n\
         \x20 constructor(engValue: u32);\n\
         }\n",
    )])
    .expect("a foreign-free ambient source needs no records");
    assert!(module.foreign_fns.is_empty());
    assert!(module.foreign_mirrors.is_empty());
}
