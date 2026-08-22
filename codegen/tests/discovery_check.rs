use subscript_codegen::{emit_c, value_class_layouts};
use subscript_compiler::{check_program_with, CheckOptions, SourceFile};

fn discovery_hir() -> subscript_compiler::hir::Module {
    let source = "import { A_SIZE } from \"./p.typegpu\";\n\
                  export function main(): void { const size: i32 = A_SIZE; }\n";
    let mut options = CheckOptions::default();
    options.poison_missing_modules = vec!["./p.typegpu".to_string()];
    check_program_with(&[SourceFile::new("main.ts", source)], &options).expect("discovery check")
}

#[test]
fn c_emission_rejects_a_discovery_hir() {
    let error = emit_c(&discovery_hir()).expect_err("discovery HIR must not emit C");

    assert_eq!(
        error,
        "cannot emit discovery HIR: poisoned import `./p.typegpu`"
    );
}

#[test]
fn value_class_layouts_rejects_a_discovery_hir() {
    let error =
        value_class_layouts(&discovery_hir()).expect_err("discovery HIR must not produce layouts");

    assert_eq!(
        error,
        "cannot lay out discovery HIR: poisoned import `./p.typegpu`"
    );
}
