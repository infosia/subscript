//! Operand lifetime checks on the dev JIT and interpreter (compiler.md §120).
//! The JIT retains freed allocations with threshold 0 bytes and a 1 GiB budget (compiler.md §8.1a-2, §8.1a-3).
//! Threshold 0 retains every probe, including one-field JSON and Worker objects.
//! Cost: each JIT run forks one child on Unix; supported cases use the same source on both engines.

use subscript_codegen::{
    interpreter::{interpret, InterpretError},
    lir::lower_module,
    run_jit_with_freed_handle_diagnostics_and_native_libraries, RunError,
};
use subscript_compiler::{check_program, SourceFile};

fn check_case(
    name: &str,
    source: &str,
    stdout: &[u8],
    trap: Option<(&str, u32, u32)>,
    worker: bool,
) {
    let files = [SourceFile::new(format!("{name}.ts"), source)];
    let actual = run_jit_with_freed_handle_diagnostics_and_native_libraries(&files, &[]);
    match (actual, trap) {
        (Ok(actual), None) => assert_eq!(actual, stdout, "{name}: JIT"),
        (Err(RunError::Trap(actual)), Some((kind, line, col))) => {
            assert_eq!(actual.stdout, stdout, "{name}: JIT");
            assert_eq!(actual.rule.to_string(), kind, "{name}: JIT");
            assert_eq!(
                (actual.pos.line, actual.pos.col),
                (line, col),
                "{name}: JIT"
            );
            let message = if kind == "double-delete" {
                "Context.free of an already-deleted allocation"
            } else {
                "use of a deleted allocation"
            };
            assert_eq!(actual.message, message, "{name}: JIT");
        }
        (actual, expected) => panic!("{name}: JIT {actual:?}, expected {expected:?}"),
    }
    if worker {
        return;
    }
    let hir = check_program(&files).expect("checked probe");
    let lir = lower_module(&hir).expect("lowered probe");
    let actual = interpret(&lir);
    let (output, error) = match actual {
        Ok(output) => (output, None),
        Err(InterpretError::Execution { output, source }) => (output, Some(*source)),
        Err(error) => (Vec::new(), Some(error)),
    };
    assert_eq!(output, stdout, "{name}: interpreter");
    match (error, trap) {
        (None, None) => {}
        (
            Some(InterpretError::Trap {
                kind, pos, message, ..
            }),
            Some((expected, line, col)),
        ) => {
            assert_eq!(kind, expected, "{name}: interpreter");
            assert_eq!((pos.line, pos.col), (line, col), "{name}: interpreter");
            if expected == "double-delete" {
                assert_eq!(
                    message, "Context.free of an already-deleted allocation",
                    "{name}: interpreter"
                );
            }
        }
        (actual, expected) => panic!("{name}: interpreter {actual:?}, expected {expected:?}"),
    }
}

fn probe(name: &str, setup: &str, operation: &str, column: u32) {
    for freed in [false, true] {
        let release = if freed { "Context.free(x);" } else { "" };
        let source = format!("export function main(): void {{\n  print(\"before\");\n  {setup}\n  {release}\n  {operation}\n  print(\"done\");\n}}\n");
        check_case(
            name,
            &source,
            if freed {
                b"before\n"
            } else {
                b"before\ndone\n"
            },
            freed.then_some(("use-after-delete", 5, column)),
            false,
        );
    }
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
fn worker_message_argument() {
    // JIT only: the interpreter requires a runtime worker adapter and a second interpreter Context (compiler.md §120).
    for freed in [false, true] {
        let release = if freed { "Context.free(x);" } else { "" };
        let source = format!("class Message {{ value: i32; constructor() {{ this.value = 1; }} }}\nfunction sink(input: Inbox<Message>, output: Outbox<Message>): void {{ input.wait(); }}\nexport function main(): void {{\n  const worker = Worker.spawn(sink); const x = new Message();\n  {release}\n  worker.post(x);\n  worker.close(); worker.join(); print(\"done\");\n}}\n");
        check_case(
            "worker",
            &source,
            if freed { b"" } else { b"done\n" },
            freed.then_some(("use-after-delete", 6, 15)),
            true,
        );
    }
}

#[test]
fn synthesized_json_helper_argument() {
    let fields = "value: i32 = 1;";
    for freed in [false, true] {
        let release = if freed { "Context.free(x);" } else { "" };
        let source = format!("class Item {{ {fields} }}\nexport function main(): void {{\n  const x = new Item();\n  {release}\n  JSON.stringify(x);\n  print(\"done\");\n}}\n");
        check_case(
            "json",
            &source,
            if freed { b"" } else { b"done\n" },
            freed.then_some(("use-after-delete", 5, 18)),
            false,
        );
    }
}

#[test]
fn nullable_json_helper_argument() {
    let fields = "value: i32 = 1;";
    for (name, setup, release, expected) in [
        ("null", "const x: Person | null = null;", "", "null\n"),
        (
            "freed",
            "const person = new Person(); const x: Person | null = person;",
            "Context.free(person);",
            "",
        ),
    ] {
        let source = format!("class Person {{ {fields} }}\nexport function main(): void {{\n  {setup}\n  {release}\n  print(JSON.stringify(x));\n}}\n");
        check_case(
            name,
            &source,
            expected.as_bytes(),
            (name == "freed").then_some(("use-after-delete", 5, 24)),
            false,
        );
    }
}

#[test]
fn for_of_map_keys() {
    probe(
        "for-map-keys",
        "const x = new Map<i32, i32>(); x.set(1, 2);",
        "for (const v of x.keys()) {}",
        19,
    );
}

#[test]
fn for_of_map_values() {
    probe(
        "for-map-values",
        "const x = new Map<i32, i32>(); x.set(1, 2);",
        "for (const v of x.values()) {}",
        19,
    );
}

#[test]
fn for_of_set_values() {
    for (name, subject) in [
        ("set", "x"),
        ("set-keys", "x.keys()"),
        ("set-values", "x.values()"),
    ] {
        probe(
            name,
            "const x = new Set<i32>(); x.add(1);",
            &format!("for (const v of {subject}) {{}}"),
            19,
        );
    }
}

#[test]
fn for_of_non_releasable_subjects() {
    for (name, setup, subject, ty) in [
        ("array-values", "const x: i32[] = [1];", "x", "i32[]"),
        ("array-keys", "const x: i32[] = [1];", "x.keys()", "i32[]"),
        (
            "fixed-array",
            "const x: FixedArray<i32, 2> = [1, 2];",
            "x",
            "FixedArray<i32, 2>",
        ),
        ("string", "const x = \"a\";", "x", "string"),
    ] {
        for freed in [false, true] {
            let release = if freed { "Context.free(x);" } else { "" };
            let source = format!("export function main(): void {{ {setup} {release} for (const v of {subject}) {{}} print(\"done\"); }}");
            if freed {
                let files = [SourceFile::new(format!("{name}.ts"), source)];
                let errors = check_program(&files).unwrap_err();
                let error = errors
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("\n");
                assert!(error.contains("S100"), "{name}: {error}");
                assert!(
                    error.contains(&format!("the argument expects `object`, got `{ty}`")),
                    "{name}: {error}"
                );
            } else {
                check_case(name, &source, b"done\n", None, false);
            }
        }
    }
    probe(
        "releasable-for-of",
        "const x = new Set<i32>(); x.add(1);",
        "for (const v of x) {}",
        19,
    );
}

#[test]
fn non_storing_generic_arguments() {
    for method in ["includes", "indexOf", "lastIndexOf"] {
        let operation = format!("a.{method}(x);");
        let column = operation.find("(x)").unwrap() as u32 + 4;
        probe(
            &format!("array-{method}"),
            "const x = new Set<i32>(); const a: Set<i32>[] = [x];",
            &operation,
            column,
        );
    }
    probe(
        "map-fallback",
        "const a = new Map<i32, Set<i32>>(); const x = new Set<i32>();",
        "a.getOr(1, x);",
        14,
    );
}

#[test]
fn double_delete_keeps_the_call_position_with_async_owner_fields() {
    for (name, declaration) in [
        ("owner", "async function value(): Promise<i32> { return 1; } class Holder { handle: Promise<i32>; constructor() { this.handle = value(); } }"),
        ("scalar", "class Holder { value: i32 = 1; }"),
    ] {
        for twice in [false, true] {
            let second = if twice { "Context.free(x);" } else { "" };
            let source = format!("{declaration}\nexport function main(): void {{\n  const x = new Holder();\n  Context.free(x);\n  {second}\n  print(\"done\");\n}}\n");
            check_case(name, &source, if twice { b"" } else { b"done\n" },
                twice.then_some(("double-delete", 5, 3)), false);
        }
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
