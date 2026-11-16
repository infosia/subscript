//! Generated generator releases free frames after the last holder in both tiers.
//! Cost: one JIT module and one C build/run; measured debug test: 0.55 s.
//! Both tiers must execute their separate release paths.

use subscript_codegen::{
    add_c11_optimized_flags, add_executable_output, add_object_directory, emit_c, host_c_compiler,
    host_entry, runtime_staticlib_path, runtime_system_libraries, tool_output_report,
    ReloadSession, HOST_HEADER_C,
};
use subscript_compiler::{check_program, SourceFile};

const SOURCE: &str = r#"
function* gen(): Generator<i32> { yield 7; }
let first: Generator<i32> = gen();
let second: Generator<i32> = gen();
export function make(): void { first = gen(); }
export function copy(): void { second = first; }
export function releaseFirst(): void { first = gen(); }
export function releaseSecond(): void { second = gen(); }
export function main(): void {}
"#;

#[test]
fn generated_last_holder_releases_free_exactly_once() {
    let files = [SourceFile::entry("last-holder.ts", SOURCE)];
    let mut dev = ReloadSession::new(&files).expect("dev JIT");
    let baseline = dev.live_allocations();
    for shared in [false, true] {
        dev.call_export("make").unwrap();
        assert_eq!(dev.live_allocations(), baseline);
        if shared {
            dev.call_export("copy").unwrap();
            assert_eq!(dev.live_allocations(), baseline - 1);
        }
        dev.call_export("releaseFirst").unwrap();
        assert_eq!(dev.live_allocations(), baseline);
        if shared {
            dev.call_export("releaseSecond").unwrap();
            assert_eq!(dev.live_allocations(), baseline);
        }
    }

    struct Scratch(std::path::PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let dir = Scratch(std::env::temp_dir().join(format!(
        "subscript-generator-last-holder-{}",
        std::process::id()
    )));
    std::fs::create_dir_all(&dir.0).unwrap();
    let program = emit_c(&check_program(&files).unwrap()).unwrap();
    std::fs::write(dir.0.join("program.c"), program.source).unwrap();
    std::fs::write(dir.0.join("subscript_runtime.h"), HOST_HEADER_C).unwrap();
    std::fs::write(dir.0.join("program.h"), &program.host_header).unwrap();
    let host = host_entry(
        r#"
#include "subscript_runtime.h"
#include <assert.h>
void subscript_init(subscript_rt_context*);
static void call(subscript_rt_context* ctx, void (*entry)(subscript_rt_context*)) {
    subscript_rt_ctx_enter_script(ctx);
    entry(ctx);
    subscript_rt_ctx_exit_script(ctx);
    assert(subscript_rt_ctx_trap_kind(ctx) == 0);
}
int main(void) {
    subscript_rt_context* ctx = subscript_rt_ctx_new();
    call(ctx, subscript_init);
    uint64_t baseline = subscript_rt_ctx_live_allocations(ctx);
    for (int shared = 0; shared <= 1; ++shared) {
        call(ctx, subscript_export_make);
        assert(subscript_rt_ctx_live_allocations(ctx) == baseline);
        if (shared) {
            call(ctx, subscript_export_copy);
            assert(subscript_rt_ctx_live_allocations(ctx) == baseline - 1);
        }
        call(ctx, subscript_export_releaseFirst);
        assert(subscript_rt_ctx_live_allocations(ctx) == baseline);
        if (shared) {
            call(ctx, subscript_export_releaseSecond);
            assert(subscript_rt_ctx_live_allocations(ctx) == baseline);
        }
    }
    subscript_rt_ctx_release(ctx);
    return 0;
}
"#,
        &program.host_header,
    )
    .unwrap();
    std::fs::write(dir.0.join("host.c"), host).unwrap();
    let compiler = host_c_compiler().unwrap();
    let executable = dir.0.join(format!("host{}", std::env::consts::EXE_SUFFIX));
    let mut command = compiler.command();
    add_c11_optimized_flags(&mut command, compiler.style());
    add_object_directory(&mut command, &dir.0, compiler.style());
    command
        .arg(dir.0.join("program.c"))
        .arg(dir.0.join("host.c"))
        .arg(runtime_staticlib_path().unwrap())
        .args(runtime_system_libraries(compiler.style()));
    add_executable_output(&mut command, &executable, compiler.style());
    let compiled = command.output().unwrap();
    assert!(
        compiled.status.success(),
        "{}",
        tool_output_report(&compiled)
    );
    let output = std::process::Command::new(executable).output().unwrap();
    assert!(output.status.success(), "{}", tool_output_report(&output));
}
