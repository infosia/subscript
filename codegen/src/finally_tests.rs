//! Completion and frame-storage controls for §180.
use subscript_compiler::{check_program, lir as l, SourceFile};

pub(crate) const FRAME_SOURCE: &str = r#"
async function f(tag: string): Promise<void> {
  try { throw new Error(tag); }
  finally { await Context.suspend(); }
}
export function main(): void {}
"#;

fn exception_capture_findings(kinds: &[l::InstructionKind]) -> Vec<crate::lir::VerifyError> {
    let hir = check_program(&[SourceFile::new(
        "capture.ts",
        "export function main(): void {}",
    )])
    .unwrap();
    let mut module = crate::lir::lower_module(&hir).unwrap();
    let error_class = module
        .classes
        .iter()
        .find(|class| class.source_name == "Error")
        .unwrap()
        .id;
    let function = module
        .functions
        .iter_mut()
        .find(|function| function.source_name == "main")
        .unwrap();
    assert!(function.blocks[0].instructions.is_empty());
    for kind in kinds {
        let result_type = match kind {
            l::InstructionKind::ExceptionMessage => Some(subscript_compiler::Type::Str),
            l::InstructionKind::ExceptionPosition => Some(subscript_compiler::Type::U32),
            l::InstructionKind::CatchEntry => Some(subscript_compiler::Type::Class(error_class)),
            _ => None,
        };
        let result = if let Some(ty) = result_type {
            let id = l::ValueId(function.values.len() as u32);
            function.values.push(l::Value {
                id,
                ty: l::ValueType::Data(ty),
                fresh_owner: false,
                source_name: None,
            });
            Some(id)
        } else {
            None
        };
        function.blocks[0].instructions.push(l::Instruction {
            result,
            count_action: None,
            kind: kind.clone(),
            operands: Vec::new(),
            invalidates: Vec::new(),
            traps: Vec::new(),
            pos: function.pos.clone(),
        });
    }
    let errors = crate::lir::verify_module(&module).unwrap_err();
    assert!(
        errors
            .iter()
            .all(|error| !error.message.contains("signature is invalid")),
        "{errors:?}"
    );
    errors
}

#[test]
fn s180_exception_message_without_capture_suffix_is_rejected() {
    let errors = exception_capture_findings(&[l::InstructionKind::ExceptionMessage]);
    assert!(
        errors.iter().any(|error| error
            .message
            .ends_with("block 0 instruction 0 has no complete frame-owned exception capture")),
        "{errors:?}"
    );
}

#[test]
fn s180_exception_message_outside_block_start_is_rejected() {
    let errors = exception_capture_findings(&[
        l::InstructionKind::FinalizerEnter(Some(l::FinalizerCompletion::FallThrough)),
        l::InstructionKind::ExceptionMessage,
        l::InstructionKind::ExceptionPosition,
        l::InstructionKind::CatchEntry,
    ]);
    assert!(
        errors.iter().any(|error| error
            .message
            .ends_with("block 0 instruction 1 has no complete frame-owned exception capture")),
        "{errors:?}"
    );
}

#[test]
fn s180_catch_entry_without_capture_prefix_is_rejected() {
    let errors = exception_capture_findings(&[
        l::InstructionKind::FinalizerEnter(Some(l::FinalizerCompletion::FallThrough)),
        l::InstructionKind::FinalizerEnter(Some(l::FinalizerCompletion::FallThrough)),
        l::InstructionKind::CatchEntry,
    ]);
    assert!(
        errors.iter().any(|error| error
            .message
            .ends_with("block 0 instruction 2 is a catch entry that does not start its block")),
        "{errors:?}"
    );
}

#[test]
fn s180_finalizer_without_completion_is_rejected() {
    let hir = check_program(&[SourceFile::new(
        "completion.ts",
        "export function main(): void {}",
    )])
    .unwrap();
    let mut module = crate::lir::lower_module(&hir).unwrap();
    let function = &mut module.functions[0];
    function.blocks[0].instructions.push(l::Instruction {
        result: None,
        count_action: None,
        kind: l::InstructionKind::FinalizerEnter(None),
        operands: Vec::new(),
        invalidates: Vec::new(),
        traps: Vec::new(),
        pos: function.pos.clone(),
    });
    let errors = crate::lir::verify_module(&module).unwrap_err();
    assert!(
        errors.iter().any(|error| error
            .message
            .contains("finalizer exit has no completion record")),
        "{errors:?}"
    );
}

#[test]
fn s180_replaced_returns_and_local_loops_keep_owner_counts() {
    let source = r#"
async function value(n: i32): Promise<i32> { return n; }
function saved(): Promise<i32> {
  try { return value(7); }
  finally { for(let i:i32=0;i<2;i++) { if(i==0) {continue;} break; } }
}
function caught(): Promise<i32> {
  try { return value(8); }
  finally { try { throw new Error("local"); } catch { print("caught"); } }
}
function same(h:Promise<i32>):Promise<i32> { try {return h;} finally {return h;} }
function replaced(): i32 {
  try { try { return 1; } finally { return 2; } }
  finally { print("outer"); }
}
export async function main():Promise<void> {
  const a=saved(); print(`${await a}`);
  const b=caught(); print(`${await b}`);
  const input=value(9); const c=same(input); print(`${await c}`);
  print(`${replaced()}`);
}
"#;
    let sources = [SourceFile::new("owners.ts", source)];
    let hir = check_program(&sources).unwrap();
    let lir = crate::lir::lower_module(&hir).unwrap();
    let expected = b"7\ncaught\n8\n9\nouter\n2\n";
    assert_eq!(crate::interpreter::interpret(&lir).unwrap(), expected);
    assert_eq!(crate::run_jit(&sources).unwrap(), expected);
    assert_eq!(crate::run_c_aot(&sources).unwrap(), expected);
}

#[test]
fn s180_c_suspended_finalizers_have_distinct_frame_slots() {
    use std::fs;
    let start = std::time::Instant::now();
    let hir = check_program(&[SourceFile::new("slots.ts", FRAME_SOURCE)]).unwrap();
    let lir = crate::lir::lower_module(&hir).unwrap();
    let id = lir
        .functions
        .iter()
        .find(|f| f.source_name == "f")
        .unwrap()
        .id
        .0;
    let program = crate::cemit::emit_lir_c(&lir, false).unwrap();
    let driver = crate::host_entry(
        r#"
#include <assert.h>
static void check( SubFrame@ID@* frame, const char* text) {
  assert(frame->l0 != NULL);
  assert(frame->l2 != 0);
  uint64_t len = *(uint64_t*)frame->l1;
  assert(len == strlen(text));
  assert(memcmp((uint8_t*)frame->l1 + 8, text, len) == 0);
}
int main(void) {
  for (int count = 1; count <= 2; count++) {
    void* ctx = subscript_rt_ctx_new();
    void* a = subscript_rt_str_lit(ctx, (const unsigned char*)"A", 1, 0);
    SubFrame@ID@* first = sub_f@ID@(ctx, a);
    assert(subscript_rt_async_start(ctx, first, NULL, 0) == 0);
    SubFrame@ID@* second = NULL;
    if (count == 2) {
      void* b = subscript_rt_str_lit(ctx, (const unsigned char*)"B", 1, 0);
      second = sub_f@ID@(ctx, b);
      assert(subscript_rt_async_start(ctx, second, NULL, 0) == 0);
    }
    check(first, "Error: A");
    if (second) { check(second, "Error: B"); assert(first->l0 != second->l0); }
    subscript_rt_ctx_release(ctx);
  }
  return 0;
}
"#,
        "",
    )
    .unwrap()
    .replace("@ID@", &id.to_string());
    let dir = std::env::temp_dir().join(format!("s180-c-slots-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("program.h"), program.host_header).unwrap();
    fs::write(
        dir.join("program.c"),
        format!("{}\n{driver}", program.source),
    )
    .unwrap();
    let compiler = crate::host_c_compiler().unwrap();
    let mut command = compiler.command();
    crate::add_c11_optimized_flags(&mut command, compiler.style());
    crate::add_object_directory(&mut command, &dir, compiler.style());
    command
        .arg(dir.join("program.c"))
        .arg(crate::runtime_staticlib_path().unwrap())
        .args(crate::runtime_system_libraries(compiler.style()));
    let exe = dir.join(format!("slots{}", std::env::consts::EXE_SUFFIX));
    crate::add_executable_output(&mut command, &exe, compiler.style());
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(std::process::Command::new(&exe).status().unwrap().success());
    fs::remove_dir_all(dir).unwrap();
    eprintln!(
        "s180 C frame-slot control: {:.3}s",
        start.elapsed().as_secs_f64()
    );
}

#[test]
fn s180_dispose_in_a_finalizer_obeys_the_local_catch() {
    let source = include_str!("../../corpus/accept/a352-finalizer-walker-controls/main.ts");
    let sources = [
        SourceFile::entry("main.ts", source),
        SourceFile::new(
            "lib.ts",
            include_str!("../../corpus/accept/a352-finalizer-walker-controls/lib.ts"),
        ),
    ];
    let hir = check_program(&sources).unwrap();
    let lir = crate::lir::lower_module(&hir).unwrap();
    let expected = include_bytes!("../../corpus/accept/a352-finalizer-walker-controls.expected");
    assert_eq!(crate::interpreter::interpret(&lir).unwrap(), expected);
    assert_eq!(crate::run_jit(&sources).unwrap(), expected);
    assert_eq!(crate::run_c_aot(&sources).unwrap(), expected);
}

#[test]
fn s180_generator_close_matches_the_golden_in_three_tiers() {
    let start = std::time::Instant::now();
    let source = include_str!("../../corpus/accept/a351-generator-close-runs-finally.ts");
    let sources = [SourceFile::new("a351.ts", source)];
    let hir = check_program(&sources).unwrap();
    let module = crate::lir::lower_module(&hir).unwrap();
    let expected = include_bytes!("../../corpus/accept/a351-generator-close-runs-finally.expected");
    assert_eq!(crate::interpreter::interpret(&module).unwrap(), expected);
    assert_eq!(crate::run_jit(&sources).unwrap(), expected);
    assert_eq!(crate::run_c_aot(&sources).unwrap(), expected);
    eprintln!(
        "s180 generator close three-tier cost: {:?}",
        start.elapsed()
    );
}

#[test]
fn s180_generator_close_without_an_enclosing_finalizer_is_rejected() {
    let start = std::time::Instant::now();
    let sources = [SourceFile::new(
        "omitted.ts",
        r#"
function* values(): Generator<i32> {
  try { try { yield 1; } finally { print("inner"); } }
  finally { print("outer"); }
}
export function main(): void {}
"#,
    )];
    let hir = check_program(&sources).unwrap();
    let mut module = crate::lir::lower_module(&hir).unwrap();
    let generator = module
        .functions
        .iter_mut()
        .find(|function| function.is_generator)
        .unwrap();
    let outer = generator.liveness.generator_close[0].finalizers[1].clone();
    // Build the violating continuation. Keep its lexical requirement unchanged.
    for block in &mut generator.blocks {
        block.instructions.retain(|instruction| {
            !matches!(&instruction.kind,
            l::InstructionKind::GeneratorFinalizer(pos) if *pos == outer)
        });
    }
    let errors = crate::lir::verify_module(&module).unwrap_err();
    assert!(
        errors.iter().any(|error| error
            .message
            .contains("close continuation omits enclosing finalizer")),
        "{errors:?}"
    );
    eprintln!(
        "s180 omitted finalizer verifier cost: {:?}",
        start.elapsed()
    );
}

#[test]
fn s180_close_releases_a_discarded_counted_yield() {
    let start = std::time::Instant::now();
    let sources = [SourceFile::new(
        "close-count.ts",
        r#"
async function fail(): Promise<i32> { throw new Error("discarded"); }
async function ok(): Promise<i32> { return 1; }
let transfer: Promise<i32>[] = [];
function consume(task: Promise<i32>): void { transfer.push(task); }
function* values(tasks: Promise<i32>[]): Generator<Promise<i32>> {
  consume(tasks[0]); transfer.pop();
  try { yield tasks[0]; }
  finally { yield tasks[1]; }
}
export async function main(): Promise<void> {
  const first = ok(); const last = fail();
  if (false) { await first; await last; }
  for (const value of values([first, last])) { break; }
  print("after");
}
"#,
    )];
    let hir = check_program(&sources).unwrap();
    let module = crate::lir::lower_module(&hir).unwrap();
    let error = crate::interpreter::interpret(&module).unwrap_err();
    assert!(format!("{error:?}").contains("discarded"), "{error:?}");
    for run in [crate::run_jit, crate::run_c_aot] {
        let crate::RunError::Trap(report) = run(&sources).unwrap_err() else {
            panic!("discarded yield trap");
        };
        assert_eq!(report.rule, subscript_runtime::TrapKind::UncaughtException);
        assert_eq!(report.message.as_str(), "Error: discarded");
        assert_eq!((report.pos.line, report.pos.col), (2, 39));
    }
    eprintln!(
        "s180 discarded counted yield three-tier cost: {:?}",
        start.elapsed()
    );
}

#[test]
fn s180_invalid_instruction_signatures_are_rejected() {
    use subscript_compiler::Type;
    let hir = check_program(&[SourceFile::new(
        "signatures.ts",
        "export function main(): void {}",
    )])
    .unwrap();
    let baseline = crate::lir::lower_module(&hir).unwrap();
    let integer = l::Operand::Constant(l::Constant {
        ty: Type::I32,
        kind: l::ConstantKind::Integer(1),
    });
    let cases = [
        (
            l::InstructionKind::FinalizerEnter(Some(l::FinalizerCompletion::Throw)),
            vec![],
            "finalizer exception payload is invalid",
        ),
        (
            l::InstructionKind::FinalizerEnter(Some(l::FinalizerCompletion::Return)),
            vec![integer.clone()],
            "finalizer return payload is invalid",
        ),
        (
            l::InstructionKind::FinalizerEnter(Some(l::FinalizerCompletion::Break(l::BlockId(
                u32::MAX,
            )))),
            vec![],
            "finalizer jump target is invalid",
        ),
        (
            l::InstructionKind::FinalizerEnter(Some(l::FinalizerCompletion::Continue(l::BlockId(
                0,
            )))),
            vec![integer.clone()],
            "finalizer jump target is invalid",
        ),
        (
            l::InstructionKind::FinalizerEnter(Some(l::FinalizerCompletion::FallThrough)),
            vec![integer.clone()],
            "finalizer fall-through payload is invalid",
        ),
        (
            l::InstructionKind::GeneratorFinalizer(baseline.functions[0].pos.clone()),
            vec![],
            "generator finalizer marker signature is invalid",
        ),
        (
            l::InstructionKind::GeneratorClose,
            vec![integer.clone()],
            "generator close signature is invalid",
        ),
        (
            l::InstructionKind::GeneratorIsClosing,
            vec![],
            "generator close flag signature is invalid",
        ),
        (
            l::InstructionKind::ExceptionMessage,
            vec![],
            "exception payload read signature is invalid",
        ),
        (
            l::InstructionKind::ExceptionPosition,
            vec![integer.clone()],
            "exception payload read signature is invalid",
        ),
        (
            l::InstructionKind::ExceptionRestore,
            vec![integer],
            "exception restore signature is invalid",
        ),
    ];
    for (kind, operands, message) in cases {
        let mut module = baseline.clone();
        let function = &mut module.functions[0];
        function.blocks[0].instructions.push(l::Instruction {
            result: None,
            count_action: None,
            kind,
            operands,
            invalidates: vec![],
            traps: vec![],
            pos: function.pos.clone(),
        });
        let errors = crate::lir::verify_module(&module).unwrap_err();
        if message == "exception restore signature is invalid" {
            assert!(
                errors.iter().any(|error| error
                    .message
                    .contains("finalizer exception restore does not read a frame-owned slot")),
                "{errors:?}"
            );
        }
        assert!(
            errors.iter().any(|error| error.message.contains(message)),
            "{message}: {errors:?}"
        );
    }
}

#[test]
fn s180_suspension_close_requirements_read_the_violating_graph() {
    let hir = check_program(&[SourceFile::new(
        "close-graph.ts",
        "function* g(): Generator<i32> { yield 1; } export function main(): void {}",
    )])
    .unwrap();
    let baseline = crate::lir::lower_module(&hir).unwrap();
    for dispatch_missing in [false, true] {
        let mut module = baseline.clone();
        let function = module
            .functions
            .iter_mut()
            .find(|f| f.is_generator)
            .unwrap();
        if dispatch_missing {
            let state = &function.liveness.generator_close[0];
            let l::Terminator::Suspend { successor, .. } =
                function.blocks[state.suspension.0 as usize].terminator
            else {
                panic!("yield");
            };
            let block = &mut function.blocks[successor.0 as usize];
            let l::Terminator::ConditionalBranch { else_target, .. } = &block.terminator else {
                panic!("dispatch");
            };
            block.terminator = l::Terminator::Branch(else_target.clone());
        } else {
            // Build an extra suspension with no declared close continuation.
            let state = &function.liveness.generator_close[0];
            let mut block = function.blocks[state.suspension.0 as usize].clone();
            block.id = l::BlockId(function.blocks.len() as u32);
            block.instructions.clear();
            function.blocks.push(block);
        }
        let message = if dispatch_missing {
            "does not dispatch to its close continuation"
        } else {
            "suspension has no close continuation"
        };
        let errors = crate::lir::verify_module(&module).unwrap_err();
        assert!(
            errors.iter().any(|error| error.message.contains(message)),
            "{message}: {errors:?}"
        );
    }
}
