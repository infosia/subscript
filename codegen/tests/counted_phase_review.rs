//! Counted release traps, loop holds, and temporary builtin receivers (§171).
use subscript_codegen::{
    interpreter::interpret, lir::lower_module, run_c_aot, run_jit, ReloadSession,
};
use subscript_compiler::{check_program, lir as l, SourceFile};

#[test]
fn bulk_replacement_traps_before_the_next_statement() {
    let mut valid = true;
    for method in ["fill(a)", "copyWithin(0, 1)"] {
        let source = include_str!("../../corpus/trap/t93-counted-fill-release.ts");
        let source = if method == "fill(a)" {
            source.to_string()
        } else {
            source
                .replace("[fail()]", "[fail(), a]")
                .replace("fill(a)", method)
        };
        let files = [SourceFile::new("bulk-release.ts", source)];
        let lir = lower_module(&check_program(&files).unwrap()).unwrap();
        let interpreted = interpret(&lir);
        let jit = run_jit(&files);
        let ship = run_c_aot(&files);
        println!("{method}: interpreter={interpreted:?}; JIT={jit:?}; C AOT={ship:?}");
        valid &= interpreted.is_err();
        for result in [&jit, &ship] {
            valid &= matches!(result, Err(subscript_codegen::RunError::Trap(trap))
                if trap.rule == subscript_runtime::TrapKind::UncaughtException && trap.stdout.is_empty());
        }
        let bulk = lir
            .functions
            .iter()
            .flat_map(|f| &f.blocks)
            .flat_map(|b| &b.instructions)
            .find(|i| {
                i.count_action
                    .as_ref()
                    .is_some_and(|a| a.release_type().is_some())
            })
            .unwrap();
        let checked = bulk.traps.iter().any(|t| t.kind == l::TrapKind::Call);
        println!("{method}: immediate Call trap={checked}");
        valid &= checked;
    }
    assert!(valid);
}

#[test]
fn loop_keeps_its_global_subject_after_reset() {
    let files = [SourceFile::new(
        "a340.ts",
        include_str!("../../corpus/accept/a340-counted-for-of-hold.ts"),
    )];
    let lir = lower_module(&check_program(&files).unwrap()).unwrap();
    let expected = include_bytes!("../../corpus/accept/a340-counted-for-of-hold.expected");
    let interpreted = interpret(&lir);
    let jit = run_jit(&files);
    let ship = run_c_aot(&files);
    println!("loop: interpreter={interpreted:?}; JIT={jit:?}; C AOT={ship:?}");
    assert_eq!(interpreted.unwrap(), expected);
    assert_eq!(jit.unwrap(), expected);
    assert_eq!(ship.unwrap(), expected);
    let held = lir.functions.iter().flat_map(|f| &f.blocks).any(|b| {
        b.instructions.iter().enumerate().any(|(index, i)| {
            matches!(i.kind, l::InstructionKind::IteratorCreate { .. })
                && b.instructions[..index].iter().any(|retain| {
                    retain.kind == l::InstructionKind::AsyncHandleArrayRetain
                        && retain.operands == i.operands
                        && retain.pos == i.pos
                })
        })
    });
    println!("loop: subject retain={held}");
    assert!(held);
}

#[test]
fn two_element_temporary_builtin_receivers_leave_no_tasks() {
    let mut valid = true;
    for method in ["pop", "push"] {
        for temporary in [false, true] {
            let call = if method == "pop" {
                "const last = RECEIVER.pop(); await last;"
            } else {
                "RECEIVER.push(h);"
            };
            let setup = if temporary {
                ""
            } else {
                "const batch = [h, h];"
            };
            let receiver = if temporary { "[h, h]" } else { "batch" };
            let source = format!(
                r#"
async function work(): Promise<i32> {{ return 7; }}
async function use(): Promise<void> {{
  const h = work(); await h;
  {setup}
  {}
  print("done");
}}
export async function main(): Promise<void> {{ await use(); }}
"#,
                call.replace("RECEIVER", receiver)
            );
            let files = [SourceFile::new("temporary-builtin.ts", source)];
            let lir = lower_module(&check_program(&files).unwrap()).unwrap();
            assert_eq!(interpret(&lir).unwrap(), b"done\n");
            assert_eq!(run_jit(&files).unwrap(), b"done\n");
            assert_eq!(run_c_aot(&files).unwrap(), b"done\n");
            let mut session = ReloadSession::new(&files).unwrap();
            session.call_main().unwrap();
            while session.async_pending() != 0 {
                session.async_step().unwrap();
            }
            let tasks = session.async_tasks();
            println!("{method} temporary={temporary}: tasks={tasks:?}");
            valid &= tasks.is_empty();
        }
    }
    assert!(valid);
}
