//! A host entry during a host entry traps (`specs/blocks/compiler.md`
//! §223).
//!
//! One ship-tier program and one C test host. The host defines the
//! foreign functions, and each one calls a host entry of the running
//! Context. Each case runs on its own Context and prints a transcript:
//! the script output, the trap kind, and the trap message.

use subscript_codegen::{
    add_c11_optimized_flags, add_executable_output, add_object_directory, emit_c, host_c_compiler,
    host_entry, runtime_staticlib_path, runtime_system_libraries, tool_output_report,
    HOST_HEADER_C,
};
use subscript_compiler::{check_program, SourceFile};

const MIRROR: &str = "// @subscript-c-header include=\"engine.h\"
// @subscript-c-callback typedef=\"EngineCallback\"
type EngineCallback = (engineMessage: string, engineUserdata1: object | null, engineUserdata2: object | null) => void;
declare class EngineSink {
  engineCallback: EngineCallback;
  engineUserdata1: object | null;
  engineUserdata2: object | null;
  constructor(engineCallback: EngineCallback, engineUserdata1: object | null, engineUserdata2: object | null);
}
declare function engineUse(engineSink: EngineSink): void;
declare function hostReenter(): void;
declare function hostStep(): void;
declare function hostInit(): void;
declare function hostTake(): void;
";

// An export named `init` shares no C name with the module initializer.
const PROGRAM: &str = "export function inner(): void { print(\"inner\"); }
export function init(): void {}
export function main(): void {
  print(\"before\");
  hostReenter();
  print(\"after\");
}
export function take(value: i32): void { print(`take ${value}`); }
export function initAgain(): void {
  print(\"init-before\");
  hostInit();
  print(\"init-after\");
}
export function takeAgain(): void {
  print(\"take-before\");
  hostTake();
  print(\"take-after\");
}
export function callback(): void {
  engineUse(new EngineSink((m, a, b) => { print(`cb ${m.length}`); }, null, null));
  print(\"callback-after\");
}
export async function stepper(): Promise<void> {
  await Context.suspend();
  print(\"step-before\");
  hostStep();
  print(\"step-after\");
}
";

const ENGINE_H: &str = "#include <stddef.h>
#include <stdint.h>
typedef struct EngineView { const uint8_t* data; size_t len; } EngineView;
typedef void (*EngineCallback)(EngineView message, void* userdata1, void* userdata2);
typedef struct EngineSink { EngineCallback engineCallback; void* engineUserdata1; void* engineUserdata2; } EngineSink;
void engineUse(EngineSink sink);
void hostReenter(void);
void hostStep(void);
void hostInit(void);
void hostTake(void);
";

const EXPECTED: &str = "[export-in-foreign-call]
before
trap 34 host entry `subscript_export_inner` called during a host entry of the same Context
[export-after-outer-entry]
before
inner
trap 0
[init-in-foreign-call]
init-before
trap 34 host entry `subscript_init` called during a host entry of the same Context
[parameter-export-in-foreign-call]
take-before
trap 34 host entry `subscript_export_take` called during a host entry of the same Context
[parameter-export-after-outer-entry]
take 5
trap 0
[callback-in-foreign-call]
cb 3
callback-after
trap 0
[step-in-foreign-call]
step-before
trap 34 host entry `subscript_rt_ctx_async_step` called during a host entry of the same Context
";

/// §223.2 items 2–4: the inner entry runs no script code and the outer
/// entry stops, for an export, `subscript_init`, an export with a
/// parameter, and an async step. The controls run the same exports after
/// the outer entry, and a callback during a foreign call.
#[test]
fn a_host_entry_during_a_foreign_call_traps_and_the_outer_entry_stops() {
    // Cost: one emitted program, one C compile and link, and one host run
    // that covers the seven cases on seven Contexts (about 2 s).
    struct Scratch(std::path::PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let dir = Scratch(
        std::env::temp_dir().join(format!("subscript-host-reentry-{}", std::process::id())),
    );
    let dir_path = dir.0.as_path();
    std::fs::create_dir_all(dir_path).expect("host test directory");
    let hir = check_program(&[
        SourceFile::ambient("engine.generated.d.ts", MIRROR),
        SourceFile::new("prog.ts", PROGRAM),
    ])
    .expect("checked source");
    let program = emit_c(&hir).expect("generated C");
    // The public entry keeps the C signature and forwards its parameters to
    // the entry body through the host-entry guard.
    let entry = program
        .source
        .split("\nvoid subscript_export_take")
        .nth(1)
        .expect("public host entry");
    assert!(
        entry.starts_with("(subscript_rt_context* ctx, int32_t a"),
        "{entry}"
    );
    assert!(entry.contains("sub_host_take(ctx, a"), "{entry}");
    std::fs::write(dir_path.join("subscript_runtime.h"), HOST_HEADER_C).expect("header");
    std::fs::write(dir_path.join("engine.h"), ENGINE_H).expect("engine header");
    std::fs::write(dir_path.join("program.h"), &program.host_header).expect("program header");
    std::fs::write(dir_path.join("program.c"), &program.source).expect("program");
    let host = host_entry(
        r#"
#include "subscript_runtime.h"
#include "engine.h"
#include <stdio.h>
#include <string.h>
void subscript_init(subscript_rt_context*);
static subscript_rt_context* current;
void hostReenter(void) { subscript_export_inner(current); }
void hostStep(void) { (void)subscript_rt_ctx_async_step(current); }
void hostInit(void) { subscript_init(current); }
void hostTake(void) { subscript_export_take(current, 5); }
void engineUse(EngineSink sink) {
    EngineView view = { (const uint8_t*)"abc", 3 };
    sink.engineCallback(view, sink.engineUserdata1, sink.engineUserdata2);
}
static void report(const char* label) {
    uint64_t length = 0;
    const uint8_t* output = subscript_rt_ctx_stdout(current, &length);
    printf("[%s]\n", label);
    fwrite(output, 1, (size_t)length, stdout);
    uint64_t message_length = 0;
    const uint8_t* message = subscript_rt_ctx_trap_message(current, &message_length);
    printf("trap %u", subscript_rt_ctx_trap_kind(current));
    if (message_length > 0) {
        printf(" ");
        fwrite(message, 1, (size_t)message_length, stdout);
    }
    printf("\n");
    subscript_rt_ctx_release(current);
    current = NULL;
}
static void start(void) {
    current = subscript_rt_ctx_new();
    subscript_init(current);
}
int main(void) {
    start();
    subscript_export_main(current);
    report("export-in-foreign-call");

    start();
    subscript_export_main(current);
    (void)subscript_rt_ctx_clear_trap(current);
    hostReenter();
    report("export-after-outer-entry");

    start();
    subscript_export_initAgain(current);
    report("init-in-foreign-call");

    start();
    subscript_export_takeAgain(current);
    report("parameter-export-in-foreign-call");

    start();
    hostTake();
    report("parameter-export-after-outer-entry");

    start();
    subscript_export_callback(current);
    report("callback-in-foreign-call");

    start();
    subscript_export_stepper(current);
    (void)subscript_rt_ctx_async_step(current);
    report("step-in-foreign-call");
    return 0;
}
"#,
        &program.host_header,
    )
    .expect("host body");
    std::fs::write(dir_path.join("host.c"), host).expect("host");
    let compiler = host_c_compiler().expect("host compiler");
    let runtime = runtime_staticlib_path().expect("runtime archive");
    let executable = dir_path.join(format!("host{}", std::env::consts::EXE_SUFFIX));
    let mut command = compiler.command();
    add_c11_optimized_flags(&mut command, compiler.style());
    add_object_directory(&mut command, dir_path, compiler.style());
    command
        .arg(dir_path.join("program.c"))
        .arg(dir_path.join("host.c"))
        .arg(&runtime)
        .args(runtime_system_libraries(compiler.style()));
    add_executable_output(&mut command, &executable, compiler.style());
    let compiled = command.output().expect("C compiler");
    assert!(
        compiled.status.success(),
        "{}",
        tool_output_report(&compiled)
    );
    let output = std::process::Command::new(&executable)
        .output()
        .expect("host run");
    assert!(output.status.success(), "{}", tool_output_report(&output));
    assert_eq!(String::from_utf8_lossy(&output.stdout), EXPECTED);
}
