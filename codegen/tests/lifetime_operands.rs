//! Operand lifetime checks through the corpus interpreter (compiler.md §120).

#[allow(dead_code)]
#[path = "corpus/mod.rs"]
mod corpus;

use std::fs;
use subscript_codegen::{interpreter::interpret, lir::lower_module};
use subscript_compiler::check_program;

fn probe(name: &str, setup: &str, operation: &str, column: u32) {
    let directory =
        std::env::temp_dir().join(format!("subscript-lifetime-{}-{name}", std::process::id()));
    fs::create_dir_all(&directory).expect("probe directory");
    for freed in [false, true] {
        let name = format!("{name}-{freed}");
        let release = if freed { "Context.free(x);" } else { "" };
        let source = format!("export function main(): void {{\n  print(\"before\");\n  {setup}\n  {release}\n  {operation}\n  print(\"done\");\n}}\n");
        let path = directory.join(format!("{name}.ts"));
        fs::write(&path, source).expect("probe source");
        let files = corpus::entry_sources(&directory, &name);
        let module = check_program(&files).expect("checked probe");
        let lir = lower_module(&module).expect("lowered probe");
        let reference = interpret(&lir);
        if freed {
            let error = format!("{:?}", reference.unwrap_err());
            assert!(error.contains("use-after-delete"), "{name}: {error}");
            assert!(
                error.contains(&format!("line: 5, col: {column}")),
                "{name}: {error}"
            );
        } else {
            assert_eq!(
                reference.expect("live operand"),
                b"before\ndone\n",
                "{name}"
            );
        }
    }
    fs::remove_dir_all(directory).expect("remove probes");
}

#[test]
fn map_constructor_source() {
    probe(
        "map-copy",
        "const x = new Map<i32, i32>(); x.set(1, 2);",
        "const y = new Map(x);",
        21,
    );
}

#[test]
fn set_constructor_source() {
    probe(
        "set-copy",
        "const x = new Set<i32>(); x.add(2);",
        "const y = new Set<i32>(x);",
        26,
    );
}

#[test]
fn array_from_source() {
    probe(
        "array-from",
        "const x = new Set<i32>(); x.add(2);",
        "const y = Array.from(x);",
        24,
    );
}

#[test]
fn set_algebra_argument() {
    for method in [
        "union",
        "intersection",
        "difference",
        "symmetricDifference",
        "isSubsetOf",
        "isSupersetOf",
        "isDisjointFrom",
    ] {
        let operation = format!("const y = a.{method}(x);");
        let column = operation.find("(x)").unwrap() as u32 + 4;
        probe(
            method,
            "const a = new Set<i32>(); const x = new Set<i32>(); x.add(2);",
            &operation,
            column,
        );
    }
}

#[test]
fn spread_container_argument() {
    probe(
        "spread",
        "const x = new Set<i32>(); x.add(2);",
        "const y = [0, ...x];",
        20,
    );
}

#[test]
fn runtime_receiver() {
    probe(
        "receiver",
        "const x = new Map<i32, i32>(); x.set(1, 2);",
        "x.has(1);",
        3,
    );
}

#[test]
fn stored_value_is_checked_only_when_read() {
    probe(
        "stored-read",
        "const a = new Map<i32, Set<i32>>(); const x = new Set<i32>(); x.add(2); a.set(1, x);",
        "a.set(2, x); const y = x.size;",
        26,
    );
}

#[test]
fn reduction_accumulator_argument() {
    for (name, setup, callback) in [
        ("static-reduce", "const a: i32[] = [1]; const x = new Set<i32>();", "(acc: Set<i32>, v: i32): Set<i32> => acc"),
        ("dynamic-reduce", "const a: i32[] = [1]; const x = new Set<i32>(); const keep = (acc: Set<i32>, v: i32): Set<i32> => acc;", "keep"),
    ] {
        let operation = format!("const y = a.reduce({callback}, x);");
        let column = operation.rfind("x)").unwrap() as u32 + 3;
        probe(name, setup, &operation, column);
    }
}

#[test]
fn lifetime_index_must_name_an_instruction_operand() {
    let source = subscript_compiler::SourceFile::new("index.ts", "export function main(): void { const a = new Set<i32>(); const b = new Set<i32>(); a.union(b); }");
    let hir = check_program(&[source]).unwrap();
    let mut lir = lower_module(&hir).unwrap();
    subscript_codegen::lir::verify_module(&lir).unwrap();
    let instruction = lir
        .functions
        .iter_mut()
        .flat_map(|function| &mut function.blocks)
        .flat_map(|block| &mut block.instructions)
        .find(|instruction| {
            instruction.traps.iter().any(|trap| {
                matches!(
                    trap.kind,
                    subscript_compiler::lir::TrapKind::DevOnlyLifetime(1)
                )
            })
        })
        .unwrap();
    instruction.operands.pop();
    let errors = subscript_codegen::lir::verify_module(&lir).unwrap_err();
    assert!(
        errors.iter().any(|error| error
            .to_string()
            .contains("lifetime operand index is out of range")),
        "{errors:?}"
    );
}

#[test]
fn read_through_instruction_without_a_site_is_rejected() {
    use subscript_compiler::{lir as l, SourceFile};
    let hir = check_program(&[SourceFile::new(
        "missing.ts",
        "export function main(): void { const x = new Set<i32>(); for (const v of x) {} }",
    )])
    .unwrap();
    let mut lir = lower_module(&hir).unwrap();
    subscript_codegen::lir::verify_module(&lir).unwrap();
    let instruction = lir
        .functions
        .iter_mut()
        .flat_map(|function| &mut function.blocks)
        .flat_map(|block| &mut block.instructions)
        .find(|instruction| matches!(instruction.kind, l::InstructionKind::IteratorCreate { .. }))
        .unwrap();
    let invalid = l::Instruction {
        traps: Vec::new(),
        ..instruction.clone()
    };
    *instruction = invalid;
    let errors = subscript_codegen::lir::verify_module(&lir).unwrap_err();
    assert!(
        errors.iter().any(|error| error
            .to_string()
            .contains("read-through operand 0 has no lifetime site")),
        "{errors:?}"
    );
}

#[test]
fn double_delete_checks_owner_fields_before_the_load() {
    for declaration in [
        "async function value(): Promise<i32> { return 1; } class Holder { handle: Promise<i32>; constructor() { this.handle = value(); } }",
        "class Holder { value: i32 = 1; }",
    ] {
        for twice in [false, true] {
            let second = if twice { "Context.free(x);" } else { "" };
            let source = format!("{declaration}\nexport function main(): void {{\n  const x = new Holder();\n  Context.free(x);\n  {second}\n  print(\"done\");\n}}\n");
            let hir = check_program(&[subscript_compiler::SourceFile::new("release.ts", source)]).unwrap();
            let lir = lower_module(&hir).unwrap();
            let output = interpret(&lir);
            if twice {
                let error = format!("{:?}", output.unwrap_err());
                assert!(error.contains("double-delete"), "{error}");
                assert!(error.contains("Context.free of an already-deleted allocation"), "{error}");
                assert!(error.contains("line: 5, col: 3"), "{error}");
            } else {
                assert_eq!(output.unwrap(), b"done\n");
            }
        }
    }
}

#[test]
fn a_site_on_a_stored_handle_is_rejected() {
    use subscript_compiler::{lir as l, SourceFile};
    let hir = check_program(&[SourceFile::new(
        "spurious.ts",
        "export function main(): void { const x = new Set<i32>(); const a: Set<i32>[] = [x]; }",
    )])
    .unwrap();
    let mut lir = lower_module(&hir).unwrap();
    subscript_codegen::lir::verify_module(&lir).unwrap();
    let instruction = lir
        .functions
        .iter_mut()
        .flat_map(|function| &mut function.blocks)
        .flat_map(|block| &mut block.instructions)
        .find(|instruction| matches!(instruction.kind, l::InstructionKind::ArrayLiteral))
        .unwrap();
    let mut traps = instruction.traps.clone();
    traps.push(l::Trap {
        kind: l::TrapKind::DevOnlyLifetime(0),
        pos: instruction.pos.clone(),
    });
    *instruction = l::Instruction {
        traps,
        ..instruction.clone()
    };
    let errors = subscript_codegen::lir::verify_module(&lir).unwrap_err();
    assert!(
        errors.iter().any(|error| error
            .to_string()
            .contains("lifetime operand 0 is neither read-through nor guarded")),
        "{errors:?}"
    );
}

#[test]
fn missing_intrinsic_record_is_rejected() {
    use subscript_compiler::SourceFile;
    let hir = check_program(&[SourceFile::new(
        "missing-intrinsic.ts",
        "export function main(): void { print(\"ok\"); }",
    )])
    .unwrap();
    let mut lir = lower_module(&hir).unwrap();
    subscript_codegen::lir::verify_module(&lir).unwrap();
    lir.intrinsic_operations.clear();
    let errors = subscript_codegen::lir::verify_module(&lir).unwrap_err();
    assert!(
        errors.iter().any(|error| error
            .to_string()
            .contains("intrinsic operation record is missing")),
        "{errors:?}"
    );
}

#[test]
fn a_lifetime_site_on_a_scalar_operand_is_rejected() {
    use subscript_compiler::{lir as l, SourceFile};
    for kind in [
        l::TrapKind::DevOnlyLifetime(0),
        l::TrapKind::DevOnlyRelease(0),
    ] {
        let hir = check_program(&[SourceFile::new(
            "scalar.ts",
            "export function main(): void { const a: i32[] = [1]; const n = a.reduce((acc: i32, v: i32): i32 => acc + v, 0); }",
        )]).unwrap();
        let mut lir = lower_module(&hir).unwrap();
        subscript_codegen::lir::verify_module(&lir).unwrap();
        let instruction = lir
            .functions
            .iter_mut()
            .flat_map(|function| &mut function.blocks)
            .flat_map(|block| &mut block.instructions)
            .find(|instruction| matches!(instruction.kind, l::InstructionKind::Binary(_)))
            .unwrap();
        *instruction = l::Instruction {
            kind: l::InstructionKind::Copy,
            operands: vec![instruction.operands[0].clone()],
            traps: vec![l::Trap {
                kind,
                pos: instruction.pos.clone(),
            }],
            ..instruction.clone()
        };
        let errors = subscript_codegen::lir::verify_module(&lir).unwrap_err();
        assert!(
            errors.iter().any(|error| error
                .to_string()
                .contains("lifetime operand 0 type does not need a lifetime trap")),
            "{errors:?}"
        );
        assert!(
            errors.iter().any(|error| error
                .to_string()
                .contains("lifetime operand 0 is neither read-through nor guarded")),
            "{errors:?}"
        );
    }
}
