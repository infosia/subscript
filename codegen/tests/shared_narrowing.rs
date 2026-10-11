//! Shared-read guards, local copies, boundary places, and disposal order (compiler.md §124).

// compiler.md §190.1 rule 3: a test function that the phase list does
// not name fails the build.
#![deny(dead_code)]

#[path = "support/main_thread.rs"]
mod main_thread;

#[path = "../../compiler/tests/corpus/interop.rs"]
#[allow(dead_code)]
mod interop;

use subscript_codegen::{interpreter::interpret, lir::lower_module, run_c_aot, run_jit};
use subscript_compiler::{check_program, SourceFile};

/// One three-engine run pins loop edges that the shared-narrowing corpus does not exercise.
fn while_checks_the_condition_after_continue_disposal() {
    let files = [SourceFile::new(
        "test.ts",
        r#"
        let count: i32 = 0;
        class Tick { [Symbol.dispose](): void { count++; } }
        export function main(): void {
            const tick = new Tick();
            while (count < 2) {
                using active = tick;
                continue;
            }
            print(`${count}`);
            print("once");
            while (true) { break; }
        }
    "#,
    )];
    let expected = b"2\nonce\n";
    let module =
        lower_module(&check_program(&files).expect("checked source")).expect("lowered source");
    assert_eq!(interpret(&module).expect("interpreter"), expected);
    assert_eq!(run_jit(&files).expect("dev JIT"), expected);
    assert_eq!(run_c_aot(&files).expect("ship C"), expected);
}

fn a_shared_function_or_global_read_traps_but_a_local_copy_survives() {
    use subscript_codegen::{interpreter::InterpretError, RunError};
    use subscript_runtime::TrapKind;
    for (declaration, expression) in [
        (
            "class Cell { v: i32 = 7; } let shared: Cell | null = new Cell();",
            "VALUE.v",
        ),
        ("let shared: (() => i32) | null = (): i32 => 7;", "VALUE()"),
    ] {
        for local in [false, true] {
            let body = if local {
                "const copied = shared; if (copied !== null) { S.acc += READ; }"
            } else {
                "if (shared !== null) { S.acc += READ; }"
            };
            let body = body.replace(
                "READ",
                &expression.replace("VALUE", if local { "copied" } else { "shared" }),
            );
            let files = [SourceFile::new(
                "test.ts",
                format!(
                    "{declaration}
                 class S {{ static get acc(): i32 {{ shared = null; return 0; }}
                   static set acc(value: i32) {{ print(`${{value}}`); }} }}
                 export function main(): void {{ {body} }}"
                ),
            )];
            let module = lower_module(&check_program(&files).expect("checked source"))
                .expect("lowered source");
            let interpreted = interpret(&module).map_err(|error| match error {
                InterpretError::Execution { source, .. } => *source,
                other => other,
            });
            if local {
                assert_eq!(interpreted.expect("local interpreter"), b"7\n");
            } else {
                assert!(
                    matches!(interpreted, Err(InterpretError::Trap { runtime_kind: Some(TrapKind::SharedNullNarrowing), ref message, .. }) if message == "a narrowed shared location is null"),
                    "{interpreted:?}"
                );
            }
            for result in [run_jit(&files), run_c_aot(&files)] {
                if local {
                    assert_eq!(result.expect("local execution"), b"7\n");
                } else {
                    assert!(
                        matches!(result, Err(RunError::Trap(ref trap)) if trap.rule == TrapKind::SharedNullNarrowing),
                        "{result:?}"
                    );
                }
            }
        }
    }
}

#[path = "support/native_fixture.rs"]
mod native_fixture;

fn narrowed_boundary_field_and_global_stores_keep_the_box() {
    use subscript_codegen::{run_c_aot_configured, run_jit_configured, RunConfig};
    let Some(fixture) = native_fixture::fixture() else {
        return;
    };
    let libraries = [fixture.library()];
    for (declaration, path) in [
        ("class Holder { b: SGPUProbeBlendState | null = new SGPUProbeBlendState(1, 2); } const h: Holder = new Holder();", "h.b"),
        ("let b: SGPUProbeBlendState | null = new SGPUProbeBlendState(1, 2);", "b"),
    ] {
        let files = [
            interop::mirror("interop.generated.d.ts", SourceFile::ambient),
            SourceFile::new("test.ts", format!("{declaration}\nexport function main(): void {{ if ({path} !== null) {{ {path}.colorOperation = 9; print(`${{{path}.colorOperation}}`); }} }}")),
        ];
        let module = lower_module(&check_program(&files).expect("checked boundary store"))
            .expect("verified boundary store");
        assert_eq!(interpret(&module).expect("interpreter"), b"9\n");
        let config = RunConfig::default().with_native_libraries(&libraries);
        assert_eq!(run_jit_configured(&files, config).expect("JIT").stdout, b"9\n");
        assert_eq!(run_c_aot_configured(&files, config).expect("C").stdout, b"9\n");
    }
}

fn shared_narrowing_site_rejects_a_load_instead_of_a_conversion() {
    use subscript_codegen::lir::verify_module;
    use subscript_compiler::lir::{InstructionKind, TrapKind};
    let files = [SourceFile::new("test.ts", "class Cell { v: i32 = 7; } let g: Cell | null = new Cell(); export function main(): void { if (g !== null) { print(`${g.v}`); } }")];
    let mut module = lower_module(&check_program(&files).expect("checked shared read"))
        .expect("valid conversion");
    let function = module
        .functions
        .iter_mut()
        .find(|function| function.source_name == "main")
        .expect("main");
    let block = function
        .blocks
        .iter_mut()
        .find(|block| {
            block.instructions.iter().any(|instruction| {
                instruction
                    .traps
                    .iter()
                    .any(|trap| trap.kind == TrapKind::SharedNullNarrowing)
            })
        })
        .expect("narrowing block");
    let conversion_index = block
        .instructions
        .iter()
        .position(|instruction| {
            instruction
                .traps
                .iter()
                .any(|trap| trap.kind == TrapKind::SharedNullNarrowing)
        })
        .expect("conversion");
    let load_index = block.instructions[..conversion_index]
        .iter()
        .rposition(|instruction| matches!(instruction.kind, InstructionKind::LoadGlobal(_)))
        .expect("global load");
    // Move the load operation onto the guarded instruction; the guard stays unchanged.
    block.instructions[conversion_index].kind = block.instructions[load_index].kind.clone();
    let errors = verify_module(&module).expect_err("a load cannot own the conversion check");
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("shared null narrowing requires")),
        "{errors:?}"
    );
}

fn narrowing_origins_require_their_sites_and_local_copies_pass() {
    use subscript_codegen::lir::verify_module;
    use subscript_compiler::lir::{InstructionKind, NarrowOrigin, TrapKind};
    for (body, origin, site) in [
        (
            "if (g !== null) { print(`${g.v}`); }",
            NarrowOrigin::SharedRead,
            Some(TrapKind::SharedNullNarrowing),
        ),
        (
            "const copy = g; if (copy !== null) { print(`${copy.v}`); }",
            NarrowOrigin::Local,
            None,
        ),
    ] {
        let files = [SourceFile::new("test.ts", format!("class Cell {{ v: i32 = 7; }} let g: Cell | null = new Cell(); export function main(): void {{ {body} }}"))];
        let module =
            lower_module(&check_program(&files).expect("checked source")).expect("valid origin");
        verify_module(&module).expect("origin and site agree");
        let mut stripped = module.clone();
        let conversion = stripped
            .functions
            .iter_mut()
            .flat_map(|f| &mut f.blocks)
            .flat_map(|b| &mut b.instructions)
            .find(|i| i.kind == InstructionKind::NarrowNonNull(origin))
            .expect("origin conversion");
        if let Some(site) = site {
            assert!(conversion.traps.iter().any(|trap| trap.kind == site));
            conversion.traps.retain(|trap| trap.kind != site);
            let errors = verify_module(&stripped).expect_err("missing required site");
            assert!(
                errors
                    .iter()
                    .any(|error| error.message.contains(&format!("requires {site:?}"))),
                "{errors:?}"
            );
        } else {
            assert!(!conversion.traps.iter().any(|trap| matches!(
                trap.kind,
                TrapKind::SharedNullNarrowing | TrapKind::NullNarrowing
            )));
            verify_module(&stripped).expect("local requires no null site");
        }
        for kind in [
            InstructionKind::Copy,
            InstructionKind::Cast,
            InstructionKind::Coerce,
        ] {
            let mut bypass = module.clone();
            let conversion = bypass
                .functions
                .iter_mut()
                .flat_map(|f| &mut f.blocks)
                .flat_map(|b| &mut b.instructions)
                .find(|i| i.kind == InstructionKind::NarrowNonNull(origin))
                .expect("origin conversion");
            conversion.kind = kind;
            let errors =
                verify_module(&bypass).expect_err("only NarrowNonNull can remove nullability");
            assert!(
                errors.iter().any(|error| error
                    .message
                    .contains("nullable-to-value conversion requires NarrowNonNull")),
                "{errors:?}"
            );
        }
    }
}

fn as_cast_null_traps_keep_their_runtime_identity() {
    use subscript_codegen::interpreter::InterpretError;
    use subscript_runtime::TrapKind;
    let files = [
        interop::mirror("interop.generated.d.ts", SourceFile::ambient),
        SourceFile::new("test.ts", "class Cell { v: i32 = 7; } export function main(): void { const info = new SubCallbackInfo((message, userdata, userparam) => {}, null, null); const value = info.userdata as Cell; print(`${value.v}`); }")
    ];
    let module = lower_module(&check_program(&files).expect("checked cast")).expect("valid cast");
    let cast = module
        .functions
        .iter()
        .flat_map(|f| &f.blocks)
        .flat_map(|b| &b.instructions)
        .find(|i| i.kind == subscript_compiler::lir::InstructionKind::Cast)
        .expect("type-changing Cast");
    assert!(cast
        .traps
        .iter()
        .any(|trap| trap.kind == subscript_compiler::lir::TrapKind::NullNarrowing));
    assert!(cast.traps.iter().any(|trap| matches!(
        trap.kind,
        subscript_compiler::lir::TrapKind::ClassMismatch(_)
    )));
    let error = interpret(&module).expect_err("null cast traps");
    let error = match error {
        InterpretError::Execution { source, .. } => *source,
        other => other,
    };
    assert!(
        matches!(error, InterpretError::Trap { runtime_kind: Some(TrapKind::NullNarrowing), ref message, .. } if message == "`as` narrowing applied to null"),
        "{error:?}"
    );
    let Some(fixture) = native_fixture::fixture() else {
        return;
    };
    let libraries = [fixture.library()];
    let config = subscript_codegen::RunConfig::default().with_native_libraries(&libraries);
    for result in [
        subscript_codegen::run_jit_configured(&files, config),
        subscript_codegen::run_c_aot_configured(&files, config),
    ] {
        assert!(
            matches!(result, Err(subscript_codegen::RunError::Trap(ref trap)) if trap.rule == TrapKind::NullNarrowing),
            "{result:?}"
        );
    }
}

fn terminators_report_each_narrowing_kind() {
    use subscript_codegen::lir::verify_module;
    use subscript_compiler::lir::{Terminator, TrapKind};
    let files = [SourceFile::new("test.ts", "class Cell { v: i32 = 7; } let g: Cell | null = new Cell(); export function main(): void { if (g !== null) { print(`${g.v}`); } }")];
    let module = lower_module(&check_program(&files).expect("checked source")).expect("valid LIR");
    for (kind, message) in [
        (
            TrapKind::SharedNullNarrowing,
            "shared null narrowing requires a nullable-to-value conversion",
        ),
        (
            TrapKind::NullNarrowing,
            "as null narrowing requires a checked cast",
        ),
    ] {
        let mut invalid = module.clone();
        let function = invalid
            .functions
            .iter_mut()
            .find(|f| f.source_name == "main")
            .expect("main");
        let mut trap = function
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .flat_map(|i| &i.traps)
            .find(|t| t.kind == TrapKind::SharedNullNarrowing)
            .expect("shared trap")
            .clone();
        trap.kind = kind.clone();
        function.blocks[0].terminator = Terminator::Trap(trap);
        let errors = verify_module(&invalid).expect_err("terminator cannot own narrowing");
        assert!(
            errors.iter().any(|error| error.message.contains(message)),
            "{errors:?}"
        );
        let other = if kind == TrapKind::NullNarrowing {
            "shared null narrowing requires a nullable-to-value conversion"
        } else {
            "as null narrowing requires a checked cast"
        };
        assert!(
            !errors.iter().any(|error| error.message.contains(other)),
            "{errors:?}"
        );
    }
}

// compiler.md §190.1 rule 3: phase 1 runs in parallel; each phase 2 test
// starts a dev run with a native library or a file provider and runs in
// its own process, on that process's main thread.
fn main() -> std::process::ExitCode {
    main_thread::run(&main_thread_tests![
        parallel: [
            while_checks_the_condition_after_continue_disposal,
            a_shared_function_or_global_read_traps_but_a_local_copy_survives,
            shared_narrowing_site_rejects_a_load_instead_of_a_conversion,
            narrowing_origins_require_their_sites_and_local_copies_pass,
            terminators_report_each_narrowing_kind,
        ],
        main_thread: [
            narrowed_boundary_field_and_global_stores_keep_the_box,
            as_cast_null_traps_keep_their_runtime_identity,
        ],
    ])
}
