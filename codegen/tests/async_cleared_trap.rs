//! Ship C host checks for clearance and continuation ownership (§94.2).

#[path = "support/async_review.rs"]
mod programs;

use subscript_codegen::{
    add_c11_optimized_flags, add_executable_output, add_object_directory, emit_c, host_c_compiler,
    runtime_staticlib_path, runtime_system_libraries, tool_output_report, HOST_HEADER_C,
};
use subscript_compiler::{check_program, SourceFile};

#[test]
fn ship_c_cleared_continuations_never_replay_or_leak() {
    struct Scratch(std::path::PathBuf);
    impl Scratch {
        fn path(&self) -> &std::path::Path {
            &self.0
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let dir = Scratch(
        std::env::temp_dir().join(format!("subscript-async-review-{}", std::process::id())),
    );
    std::fs::create_dir_all(dir.path()).expect("host test directory");
    std::fs::write(dir.path().join("subscript_runtime.h"), HOST_HEADER_C).expect("header");
    let compiler = host_c_compiler().expect("host compiler");
    let runtime = runtime_staticlib_path().expect("runtime archive");
    for (source, expected, unfinished, control, line) in [
        (programs::BODY, "m1\nm2\n", 1, false, 6),
        (programs::CALLEE, "m1\nm2\nboom:start\n", 2, false, 4),
        (programs::SETTLED, "m1\nm2\n", 1, false, 7),
        (programs::CONTROL, "m1\nm2\nm3\n", 0, true, 0),
    ] {
        let hir = check_program(&[SourceFile::new("replay.ts", source)]).expect("checked source");
        let program = emit_c(&hir).expect("generated C");
        let trap_pos = if control {
            0
        } else {
            program
                .positions
                .iter()
                .position(|pos| pos.line == line && pos.col == 14)
                .expect("the independent source position has an emitted entry")
        };
        std::fs::write(dir.path().join("program.c"), program.source).expect("program");
        let host = r#"
#include "subscript_runtime.h"
#include <assert.h>
#include <stdio.h>
#include <string.h>
void subscript_init(subscript_rt_context*);
void subscript_export_main(subscript_rt_context*);
static void call(subscript_rt_context* ctx, void (*entry)(subscript_rt_context*)) {
    subscript_rt_ctx_enter_script(ctx);
    entry(ctx);
    subscript_rt_ctx_exit_script(ctx);
}
int main(void) {
    subscript_rt_context* ctx = subscript_rt_ctx_new();
    assert(ctx);
    call(ctx, subscript_init);
    call(ctx, subscript_export_main);
    assert(subscript_rt_ctx_async_pending(ctx) == 1);
    subscript_rt_ctx_async_step(ctx);
    assert(subscript_rt_ctx_async_pending(ctx) == 1);
    uint64_t allocations = subscript_rt_ctx_live_allocations(ctx);
    if (CONTROL) {
        assert(subscript_rt_ctx_trap_kind(ctx) == 0);
        assert(subscript_rt_ctx_clear_trap(ctx) == 1);
        assert(subscript_rt_ctx_async_step(ctx) == 0);
        assert(subscript_rt_ctx_async_unfinished(ctx) == 0);
    } else {
        assert(subscript_rt_ctx_trap_kind(ctx) == 1);
        assert(subscript_rt_ctx_trap_pos_id(ctx) == TRAP_POS);
        uint64_t message_length = 0;
        const uint8_t* message = subscript_rt_ctx_trap_message(ctx, &message_length);
        const char* expected_message = "index 5 out of bounds for array length 0";
        assert(message_length == strlen(expected_message));
        assert(memcmp(message, expected_message, message_length) == 0);
        assert(subscript_rt_ctx_async_unfinished(ctx) == UNFINISHED);
        assert(subscript_rt_ctx_async_step(ctx) == 1);
        assert(subscript_rt_ctx_async_step(ctx) == 1);
        assert(subscript_rt_ctx_live_allocations(ctx) == allocations);
        fprintf(stderr, "allocations=%llu unfinished=%d\n", (unsigned long long)allocations, UNFINISHED);
        for (int i = 0; i < 5; ++i) {
            assert(subscript_rt_ctx_clear_trap(ctx) == 1);
            assert(subscript_rt_ctx_async_pending(ctx) == 0);
            assert(subscript_rt_ctx_async_step(ctx) == 0);
            assert(subscript_rt_ctx_trap_kind(ctx) == 0);
            assert(subscript_rt_ctx_live_allocations(ctx) == allocations);
            assert(subscript_rt_ctx_async_unfinished(ctx) == UNFINISHED);
        }
    }
    uint64_t length = 0;
    const uint8_t* output = subscript_rt_ctx_stdout(ctx, &length);
    fwrite(output, 1, length, stdout);
    subscript_rt_ctx_release(ctx);
    return 0;
}
"#
        .replace("CONTROL", if control { "1" } else { "0" })
        .replace("UNFINISHED", &unfinished.to_string())
        .replace("TRAP_POS", &trap_pos.to_string());
        std::fs::write(dir.path().join("host.c"), host).expect("host");
        let executable = dir
            .path()
            .join(format!("host{}", std::env::consts::EXE_SUFFIX));
        let mut command = compiler.command();
        add_c11_optimized_flags(&mut command, compiler.style());
        add_object_directory(&mut command, dir.path(), compiler.style());
        command
            .arg(dir.path().join("program.c"))
            .arg(dir.path().join("host.c"))
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
        assert_eq!(output.stdout, expected.as_bytes());
        eprintln!(
            "replay.ts:{line}:14 {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
