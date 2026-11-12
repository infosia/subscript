//! C emission controls for compiler.md section 259.
use subscript_codegen::emit_c;
use subscript_compiler::{check_program, SourceFile};

#[test]
fn alias_comparison_uses_no_string_comparison() {
    let source = SourceFile::new("compare.ts", "type Mode = \"a\" | \"b\"; function same(a: Mode, b: Mode): boolean { return a === b || a !== \"a\"; } export function main(): void { print(`${same(\"a\", \"b\")}`); }");
    let hir = check_program(&[source]).unwrap();
    let c = emit_c(&hir).unwrap().source;
    assert!(!c.contains("subscript_rt_str_eq("));

    let source = SourceFile::new("compare.ts", "function same(a: string, b: string): boolean { return a === b || a !== \"a\"; } export function main(): void { print(`${same(\"a\", \"b\")}`); }");
    let hir = check_program(&[source]).unwrap();
    let c = emit_c(&hir).unwrap().source;
    assert!(c.contains("subscript_rt_str_eq("));
}

#[test]
fn verifier_rejects_alias_string_with_a_string_operand() {
    use subscript_codegen::lir::{lower_module, verify_module};
    use subscript_compiler::{lir, Type};

    let source = SourceFile::new(
        "signature.ts",
        "type Mode = \"a\" | \"b\"; function convert(m: Mode, s: string): string { return m; }",
    );
    let hir = check_program(&[source]).unwrap();
    let mut module = lower_module(&hir).unwrap();
    verify_module(&module).expect("valid alias conversion control");
    let function = module
        .functions
        .iter_mut()
        .find(|f| f.source_name == "convert")
        .unwrap();
    let string = function
        .values
        .iter()
        .find(|v| v.ty == lir::ValueType::Data(Type::Str) && v.source_name.as_deref() == Some("s"))
        .expect("string parameter")
        .id;
    let instruction = function
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.kind, lir::InstructionKind::AliasString(_)))
        .expect("alias conversion");
    instruction.operands = vec![lir::Operand::Value(string)];
    let errors = verify_module(&module).expect_err("string is not an alias operand");
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("alias string signature is invalid")),
        "{errors:?}"
    );
}
