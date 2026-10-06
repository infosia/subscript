use super::*;

#[test]
fn posix_feature_arguments_match_the_host_contract() {
    let expected: &[&str] = if cfg!(target_os = "linux") {
        &["-D_POSIX_C_SOURCE=199309L"]
    } else {
        &[]
    };
    assert_eq!(super::posix_feature_arguments(), expected);
}

#[test]
fn c11_optimized_flags_carry_the_posix_feature_test_only_for_unix_on_linux() {
    let mut unix = Command::new("cc");
    add_c11_optimized_flags(&mut unix, CCompilerStyle::Unix);
    let expected: &[&str] = if cfg!(target_os = "linux") {
        &[
            "-std=c11",
            "-O2",
            "-fwrapv",
            "-ffp-contract=off",
            "-D_POSIX_C_SOURCE=199309L",
        ]
    } else {
        &["-std=c11", "-O2", "-fwrapv", "-ffp-contract=off"]
    };
    assert_eq!(unix.get_args().collect::<Vec<_>>(), expected);

    let mut msvc = Command::new("cl");
    add_c11_optimized_flags(&mut msvc, CCompilerStyle::Msvc);
    assert_eq!(
        msvc.get_args().collect::<Vec<_>>(),
        ["/nologo", "/std:c11", "/O2", "/utf-8", "/fp:strict"]
    );
}

fn sources(src: &str) -> Vec<SourceFile> {
    vec![SourceFile::new("test.ts", src)]
}

#[test]
fn aot_entry_without_host_hooks_is_byte_identical_to_the_standing_entry() {
    let generated = aot_entry_with_host_hooks(None, None).expect("generate entry");
    assert_eq!(generated.as_bytes(), AOT_ENTRY_C.as_bytes());
}

#[test]
fn aot_entry_host_hooks_are_optional_and_independent() {
    const PRE: &str = "fixture_pre_entry";
    const POST: &str = "fixture_post_run";
    for (pre, post) in [
        (None, None),
        (Some(PRE), None),
        (None, Some(POST)),
        (Some(PRE), Some(POST)),
    ] {
        let entry = aot_entry_with_host_hooks(pre, post).expect("generate entry");
        assert_eq!(
            entry.contains(&format!("extern void {PRE}(")),
            pre.is_some()
        );
        assert_eq!(entry.contains(&format!("    {PRE}(ctx);")), pre.is_some());
        assert_eq!(
            entry.contains(&format!("extern void {POST}(")),
            post.is_some()
        );
        assert_eq!(entry.contains(&format!("    {POST}(ctx);")), post.is_some());
    }

    let entry = aot_entry_with_host_hooks(Some(PRE), Some(POST)).expect("generate entry");
    let init = entry
        .find("    call_script_entry(ctx, subscript_init);")
        .expect("initializer call");
    let pre = entry.find("    fixture_pre_entry(ctx);").expect("pre hook");
    let main_guard = entry
        .find("    if (subscript_rt_ctx_trap_kind(ctx) == 0) {")
        .expect("main trap guard");
    assert!(init < pre && pre < main_guard);

    let pump = entry
        .find("    while (subscript_rt_ctx_trap_kind(ctx) == 0 &&")
        .expect("async pump");
    let post = entry.find("    fixture_post_run(ctx);").expect("post hook");
    let output = entry.find("    uint64_t len = 0;").expect("output capture");
    let release = entry
        .find("    subscript_rt_ctx_release(ctx);")
        .expect("Context release");
    assert!(pump < post && post < output && output < release);
}

#[test]
fn aot_entry_rejects_non_identifier_host_hook_names() {
    assert!(matches!(
        aot_entry_with_host_hooks(Some("bad-hook"), None),
        Err(RunError::Internal(message)) if message.contains("not a C identifier")
    ));
}

/// Builds an `Output` with the two streams. [`tool_output_report`]
/// reads no status, so the default status is enough.
fn tool_output(stdout: &str, stderr: &str) -> std::process::Output {
    std::process::Output {
        status: std::process::ExitStatus::default(),
        stdout: stdout.as_bytes().to_vec(),
        stderr: stderr.as_bytes().to_vec(),
    }
}

#[test]
fn tool_output_report_keeps_the_stream_that_spoke() {
    // MSVC `cl` reports on stdout; a stderr-only report loses this.
    let msvc = tool_output("host.h(12): error C2146: syntax error\n", "");
    assert_eq!(
        tool_output_report(&msvc),
        "--- stdout ---\nhost.h(12): error C2146: syntax error\n"
    );

    let unix = tool_output("", "host.h:12:5: error: expected ';'\n");
    assert_eq!(
        tool_output_report(&unix),
        "--- stderr ---\nhost.h:12:5: error: expected ';'\n"
    );
}

#[test]
fn tool_output_report_labels_both_streams_and_ends_every_line() {
    let both = tool_output("out line", "err line");
    assert_eq!(
        tool_output_report(&both),
        "--- stdout ---\nout line\n--- stderr ---\nerr line\n"
    );
}

#[test]
fn tool_output_report_names_a_silent_command() {
    let silent = tool_output("", "   \n");
    assert_eq!(
        tool_output_report(&silent),
        "(the command printed nothing)\n"
    );
}

#[test]
fn public_toolchain_api_carries_the_ship_contract() -> Result<(), String> {
    assert!(!CCompilerStyle::Unix.is_msvc());
    assert!(CCompilerStyle::Msvc.is_msvc());

    let mut unix = Command::new("cc");
    add_c11_optimized_flags(&mut unix, CCompilerStyle::Unix);
    add_object_directory(&mut unix, Path::new("objects"), CCompilerStyle::Unix);
    add_executable_output(&mut unix, Path::new("program"), CCompilerStyle::Unix);
    let expected: &[&str] = if cfg!(target_os = "linux") {
        &[
            "-std=c11",
            "-O2",
            "-fwrapv",
            "-ffp-contract=off",
            "-D_POSIX_C_SOURCE=199309L",
            "-o",
            "program",
        ]
    } else {
        &[
            "-std=c11",
            "-O2",
            "-fwrapv",
            "-ffp-contract=off",
            "-o",
            "program",
        ]
    };
    assert_eq!(unix.get_args().collect::<Vec<_>>(), expected);
    assert_eq!(
        include_directory_arg(CCompilerStyle::Unix, Path::new("include")),
        OsString::from("-Iinclude")
    );

    let mut msvc = Command::new("cl");
    add_c11_optimized_flags(&mut msvc, CCompilerStyle::Msvc);
    add_object_directory(&mut msvc, Path::new("objects"), CCompilerStyle::Msvc);
    add_executable_output(&mut msvc, Path::new("program.exe"), CCompilerStyle::Msvc);
    let msvc_args = msvc
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        &msvc_args[..5],
        ["/nologo", "/std:c11", "/O2", "/utf-8", "/fp:strict"]
    );
    assert!(msvc_args[5].starts_with("/Fo:objects"));
    assert_eq!(&msvc_args[6..], ["/Fe:program.exe", "-link"]);
    assert_eq!(
        include_directory_arg(CCompilerStyle::Msvc, Path::new("include")),
        OsString::from("/Iinclude")
    );

    assert!(matches!(
        runtime_staticlib_name(),
        "libsubscript_runtime.a" | "subscript_runtime.lib"
    ));
    assert_eq!(
        WINDOWS_SYSTEM_LIBRARIES,
        ["kernel32", "ntdll", "userenv", "ws2_32", "dbghelp"]
    );
    assert_eq!(
        system_library_arguments(SystemLibraryPlatform::Windows, CCompilerStyle::Msvc),
        [
            "kernel32.lib",
            "ntdll.lib",
            "userenv.lib",
            "ws2_32.lib",
            "dbghelp.lib",
        ]
        .map(OsString::from)
    );
    assert_eq!(
        system_library_arguments(SystemLibraryPlatform::Windows, CCompilerStyle::Unix),
        [
            "-lkernel32",
            "-lntdll",
            "-luserenv",
            "-lws2_32",
            "-ldbghelp",
        ]
        .map(OsString::from)
    );
    assert_eq!(
        system_library_arguments(SystemLibraryPlatform::Linux, CCompilerStyle::Unix),
        [
            "-lm",
            "-ldl",
            "-lpthread",
            "-lrt",
            "-lutil",
            "-lgcc_s",
            "-lc",
        ]
        .map(OsString::from)
    );
    assert!(
        system_library_arguments(SystemLibraryPlatform::MacOs, CCompilerStyle::Unix).is_empty()
    );
    assert_eq!(
        runtime_system_libraries(CCompilerStyle::Unix),
        system_library_arguments(
            if cfg!(target_os = "windows") {
                SystemLibraryPlatform::Windows
            } else if cfg!(target_os = "macos") {
                SystemLibraryPlatform::MacOs
            } else if cfg!(unix) {
                SystemLibraryPlatform::Linux
            } else {
                SystemLibraryPlatform::Other
            },
            CCompilerStyle::Unix
        )
    );

    let compiler = host_c_compiler().map_err(|error| error.to_string())?;
    assert!(!compiler.program().is_empty());
    assert_eq!(
        compiler.style(),
        if cfg!(all(windows, target_env = "msvc")) {
            CCompilerStyle::Msvc
        } else {
            CCompilerStyle::Unix
        }
    );
    assert_eq!(compiler.command().get_program(), compiler.program());
    Ok(())
}

/// Drives the emitted-C ship tier with a test-specific C host entry.
/// The compile/link flags and runtime inputs are exactly the ones used
/// by `run_c_aot`; only the host driver source differs.
fn run_c_aot_with_entry(files: &[SourceFile], entry: &str) -> std::process::Output {
    let hir = check_program(files).expect("test program checks");
    let program = crate::emit_c(&hir).expect("emit ship C");
    let staticlib = runtime_staticlib().expect("runtime staticlib");
    let dir = TempDir::new("host-api-test").expect("temp dir");
    let src_path = dir.path.join("program.c");
    let entry_path = dir.path.join("entry.c");
    let metadata_path = dir.path.join("allocation-metadata.h");
    let exe_path = dir
        .path
        .join(format!("program{}", std::env::consts::EXE_SUFFIX));
    write_file(&src_path, program.source.as_bytes()).expect("write program.c");
    write_file(&dir.path.join("program.h"), program.host_header.as_bytes()).unwrap();
    write_file(&entry_path, entry.as_bytes()).expect("write entry.c");
    write_file(
        &metadata_path,
        program.allocation_metadata_header.as_bytes(),
    )
    .expect("write allocation-metadata.h");

    let cc = host_c_compiler().expect("resolve C compiler");
    let mut command = cc.command();
    add_c11_optimized_flags(&mut command, cc.style());
    if cc.style().is_msvc() {
        command
            .arg(msvc_object_directory_arg(&dir.path))
            .arg("/I")
            .arg(&dir.path);
    } else {
        command.arg("-I").arg(&dir.path);
    }
    command
        .arg(&src_path)
        .arg(&entry_path)
        .arg(&staticlib)
        .args(runtime_system_libraries(cc.style()));
    if !cfg!(any(windows, target_os = "macos")) {
        command.arg("-pthread");
    }
    add_executable_output(&mut command, &exe_path, cc.style());
    let compile = command.output().expect("run C compiler");
    assert!(
        compile.status.success(),
        "compiling/linking test host failed:\n{}",
        tool_output_report(&compile)
    );
    Command::new(&exe_path).output().expect("run test host")
}

const MODULE_STATE_ISOLATION_SOURCE: &str = "let counter: i32 = 0;\n\
         export function advance(): void {\n\
         \x20 counter += 1;\n\
         \x20 print(`${counter}`);\n\
         }\n\
         export function main(): void {}\n";

fn run_dev_module_state_reference() -> Vec<u8> {
    let program = sources(MODULE_STATE_ISOLATION_SOURCE);
    let mut session = crate::ReloadSession::new(&program).expect("create dev session");
    session.call_export("advance").expect("first dev call");
    session.call_export("advance").expect("second dev call");
    session.take_output()
}

#[test]
fn module_state_is_isolated_between_concurrent_contexts_in_both_tiers() {
    let reference = run_dev_module_state_reference();
    assert_eq!(reference, b"1\n2\n", "single-Context reference");

    let threads: Vec<_> = (0..2)
        .map(|_| std::thread::spawn(run_dev_module_state_reference))
        .collect();
    let dev_outputs: Vec<Vec<u8>> = threads
        .into_iter()
        .map(|thread| thread.join().expect("join dev Context thread"))
        .collect();
    for (index, output) in dev_outputs.iter().enumerate() {
        assert_eq!(output, &reference, "dev Context {index} output");
    }

    let program = sources(MODULE_STATE_ISOLATION_SOURCE);
    let entry = host_entry(
            r#"
#include <stdio.h>
#include <string.h>
#ifdef _WIN32
#include <windows.h>
#else
#include <pthread.h>
#endif


struct worker_result {
    uint8_t stdout_bytes[32];
    uint64_t stdout_len;
    int error;
};

static void call_entry(subscript_rt_context* ctx, subscript_main_entry entry) {
    subscript_rt_ctx_enter_script(ctx);
    entry(ctx);
    subscript_rt_ctx_exit_script(ctx);
}

#ifdef _WIN32
struct worker_args {
    int id;
    HANDLE init_done;
    HANDLE all_initialized;
    HANDLE turns[4];
    volatile LONG* ready;
    struct worker_result* result;
};

static DWORD WINAPI run_worker(LPVOID raw) {
    struct worker_args* args = (struct worker_args*)raw;
    subscript_rt_context* ctx = subscript_rt_ctx_new();
    if (ctx == NULL) {
        args->result->error = 1;
        return 0;
    }
    if (args->id == 1) WaitForSingleObject(args->init_done, INFINITE);
    call_entry(ctx, subscript_init);
    if (args->id == 0) SetEvent(args->init_done);
    if (InterlockedIncrement(args->ready) == 2) SetEvent(args->all_initialized);
    WaitForSingleObject(args->all_initialized, INFINITE);

    for (int round = 0; round < 2; ++round) {
        const int turn = round * 2 + args->id;
        WaitForSingleObject(args->turns[turn], INFINITE);
        call_entry(ctx, subscript_export_advance);
        if (turn + 1 < 4) SetEvent(args->turns[turn + 1]);
    }

    args->result->stdout_len = 0;
    const uint8_t* bytes = subscript_rt_ctx_stdout(ctx, &args->result->stdout_len);
    if (args->result->stdout_len > sizeof args->result->stdout_bytes) {
        args->result->error = 2;
    } else if (args->result->stdout_len > 0) {
        memcpy(args->result->stdout_bytes, bytes, (size_t)args->result->stdout_len);
    }
    if (subscript_rt_ctx_trap_kind(ctx) != 0) args->result->error = 3;
    subscript_rt_ctx_release(ctx);
    return 0;
}
#else
struct coordinator {
    pthread_mutex_t mutex;
    pthread_cond_t condition;
    int init_done;
    int ready;
    int turn;
};

struct worker_args {
    int id;
    struct coordinator* coordinator;
    struct worker_result* result;
};

static void* run_worker(void* raw) {
    struct worker_args* args = (struct worker_args*)raw;
    struct coordinator* coordinator = args->coordinator;
    subscript_rt_context* ctx = subscript_rt_ctx_new();
    if (ctx == NULL) {
        args->result->error = 1;
        return NULL;
    }

    pthread_mutex_lock(&coordinator->mutex);
    while (args->id == 1 && !coordinator->init_done) {
        pthread_cond_wait(&coordinator->condition, &coordinator->mutex);
    }
    pthread_mutex_unlock(&coordinator->mutex);
    call_entry(ctx, subscript_init);
    pthread_mutex_lock(&coordinator->mutex);
    if (args->id == 0) coordinator->init_done = 1;
    coordinator->ready += 1;
    pthread_cond_broadcast(&coordinator->condition);
    while (coordinator->ready != 2) {
        pthread_cond_wait(&coordinator->condition, &coordinator->mutex);
    }
    pthread_mutex_unlock(&coordinator->mutex);

    for (int round = 0; round < 2; ++round) {
        const int expected_turn = round * 2 + args->id;
        pthread_mutex_lock(&coordinator->mutex);
        while (coordinator->turn != expected_turn) {
            pthread_cond_wait(&coordinator->condition, &coordinator->mutex);
        }
        pthread_mutex_unlock(&coordinator->mutex);
        call_entry(ctx, subscript_export_advance);
        pthread_mutex_lock(&coordinator->mutex);
        coordinator->turn += 1;
        pthread_cond_broadcast(&coordinator->condition);
        pthread_mutex_unlock(&coordinator->mutex);
    }

    args->result->stdout_len = 0;
    const uint8_t* bytes = subscript_rt_ctx_stdout(ctx, &args->result->stdout_len);
    if (args->result->stdout_len > sizeof args->result->stdout_bytes) {
        args->result->error = 2;
    } else if (args->result->stdout_len > 0) {
        memcpy(args->result->stdout_bytes, bytes, (size_t)args->result->stdout_len);
    }
    if (subscript_rt_ctx_trap_kind(ctx) != 0) args->result->error = 3;
    subscript_rt_ctx_release(ctx);
    return NULL;
}
#endif

static int compare_result(
    const struct worker_result* result,
    const uint8_t* reference,
    uint64_t reference_len,
    int index) {
    if (result->error != 0 || result->stdout_len != reference_len ||
        memcmp(result->stdout_bytes, reference, (size_t)reference_len) != 0) {
        fprintf(stderr, "Context %d stdout: ", index);
        fwrite(result->stdout_bytes, 1, (size_t)result->stdout_len, stderr);
        fprintf(stderr, "single-Context reference: ");
        fwrite(reference, 1, (size_t)reference_len, stderr);
        return 30 + index;
    }
    return 0;
}

int main(void) {
    uint8_t reference[32];
    uint64_t reference_len = 0;
    subscript_rt_context* reference_ctx = subscript_rt_ctx_new();
    if (reference_ctx == NULL) return 2;
    call_entry(reference_ctx, subscript_init);
    call_entry(reference_ctx, subscript_export_advance);
    call_entry(reference_ctx, subscript_export_advance);
    const uint8_t* reference_bytes = subscript_rt_ctx_stdout(reference_ctx, &reference_len);
    if (reference_len > sizeof reference) return 3;
    memcpy(reference, reference_bytes, (size_t)reference_len);
    subscript_rt_ctx_release(reference_ctx);

    struct worker_result results[2] = {0};
#ifdef _WIN32
    volatile LONG ready = 0;
    HANDLE init_done = CreateEvent(NULL, TRUE, FALSE, NULL);
    HANDLE all_initialized = CreateEvent(NULL, TRUE, FALSE, NULL);
    HANDLE turns[4];
    for (int i = 0; i < 4; ++i) turns[i] = CreateEvent(NULL, FALSE, i == 0, NULL);
    struct worker_args args[2] = {
        {0, init_done, all_initialized, {turns[0], turns[1], turns[2], turns[3]}, &ready, &results[0]},
        {1, init_done, all_initialized, {turns[0], turns[1], turns[2], turns[3]}, &ready, &results[1]},
    };
    HANDLE threads[2] = {
        CreateThread(NULL, 0, run_worker, &args[0], 0, NULL),
        CreateThread(NULL, 0, run_worker, &args[1], 0, NULL),
    };
    WaitForMultipleObjects(2, threads, TRUE, INFINITE);
    for (int i = 0; i < 2; ++i) CloseHandle(threads[i]);
    CloseHandle(init_done);
    CloseHandle(all_initialized);
    for (int i = 0; i < 4; ++i) CloseHandle(turns[i]);
#else
    struct coordinator coordinator = {
        PTHREAD_MUTEX_INITIALIZER, PTHREAD_COND_INITIALIZER, 0, 0, 0
    };
    struct worker_args args[2] = {
        {0, &coordinator, &results[0]},
        {1, &coordinator, &results[1]},
    };
    pthread_t threads[2];
    if (pthread_create(&threads[0], NULL, run_worker, &args[0]) != 0) return 4;
    if (pthread_create(&threads[1], NULL, run_worker, &args[1]) != 0) return 5;
    pthread_join(threads[0], NULL);
    pthread_join(threads[1], NULL);
    pthread_cond_destroy(&coordinator.condition);
    pthread_mutex_destroy(&coordinator.mutex);
#endif

    int comparison = compare_result(&results[0], reference, reference_len, 0);
    if (comparison != 0) return comparison;
    comparison = compare_result(&results[1], reference, reference_len, 1);
    if (comparison != 0) return comparison;
    return 0;
}
"#, &crate::emit_c_without_main(&check_program(&program).unwrap()).unwrap().host_header).unwrap();
    let run = run_c_aot_with_entry(&program, &entry);
    assert!(
        run.status.success(),
        "ship concurrent host exited with {}: {}",
        run.status,
        String::from_utf8_lossy(&run.stderr)
    );
}

#[test]
fn ship_c_host_trap_observer_and_clear_api_preserve_unwind_semantics() {
    let program = sources(
        "let calls: i32 = 0;\n\
             export function main(): void {\n\
               calls += 1;\n\
               print(`start:${calls}`);\n\
               if (calls === 1) {\n\
                 const failed: i32 = JSON.parse<i32>(\"nope\");\n\
                 print(`${failed}`);\n\
               }\n\
               print(\"done\");\n\
             }\n",
    );
    let entry = host_entry(
        r#"
#include <stdio.h>
#include <string.h>

struct observed_trap {
    uint32_t calls;
    uint32_t kind;
    uint32_t pos_id;
    const uint8_t* message;
    uint64_t message_len;
};

static void observe(
    void* userdata, uint32_t kind, uint32_t pos_id,
    const uint8_t* message, uint64_t message_len) {
    struct observed_trap* observed = (struct observed_trap*)userdata;
    observed->calls += 1;
    observed->kind = kind;
    observed->pos_id = pos_id;
    observed->message = message;
    observed->message_len = message_len;
}

static int fail(subscript_rt_context* ctx, int code) {
    subscript_rt_ctx_release(ctx);
    return code;
}

static void call_entry(subscript_rt_context* ctx, subscript_main_entry entry) {
    subscript_rt_ctx_enter_script(ctx);
    entry(ctx);
    subscript_rt_ctx_exit_script(ctx);
}

int main(void) {
    subscript_rt_context* ctx = subscript_rt_ctx_new();
    if (ctx == NULL) return 2;
    struct observed_trap observed = {0};
    subscript_rt_ctx_set_trap_observer(ctx, observe, &observed);
    call_entry(ctx, subscript_init);
    call_entry(ctx, subscript_export_main);

    if (observed.calls != 1) return fail(ctx, 10);
    uint32_t kind = subscript_rt_ctx_trap_kind(ctx);
    uint32_t pos_id = subscript_rt_ctx_trap_pos_id(ctx);
    uint64_t message_len = 0;
    const uint8_t* message = subscript_rt_ctx_trap_message(ctx, &message_len);
    if (kind == 0) return fail(ctx, 11);
    if (observed.kind != kind || observed.pos_id != pos_id) return fail(ctx, 12);
    if (observed.message != message || observed.message_len != message_len) return fail(ctx, 13);
    if (memcmp(observed.message, message, (size_t)message_len) != 0) return fail(ctx, 14);

    const uint64_t live_before = subscript_rt_ctx_live_allocations(ctx);
    const uint64_t bytes_before = subscript_rt_ctx_live_bytes(ctx);
    const uint64_t reserved_before = subscript_rt_ctx_reserved_bytes(ctx);
    subscript_rt_ctx_enter_script(ctx);
    const int cleared_while_live = subscript_rt_ctx_clear_trap(ctx);
    subscript_rt_ctx_exit_script(ctx);
    if (cleared_while_live != 0) return fail(ctx, 15);
    if (subscript_rt_ctx_clear_trap(ctx) != 1) return fail(ctx, 16);
    if (subscript_rt_ctx_live_allocations(ctx) != live_before ||
        subscript_rt_ctx_live_bytes(ctx) != bytes_before ||
        subscript_rt_ctx_reserved_bytes(ctx) != reserved_before) return fail(ctx, 17);
    if (subscript_rt_ctx_trap_kind(ctx) != 0) return fail(ctx, 18);
    call_entry(ctx, subscript_export_main);
    if (subscript_rt_ctx_trap_kind(ctx) != 0) return fail(ctx, 19);
    if (observed.calls != 1) return fail(ctx, 20);

    uint64_t stdout_len = 0;
    const uint8_t* stdout_bytes = subscript_rt_ctx_stdout(ctx, &stdout_len);
    if (stdout_len > 0) fwrite(stdout_bytes, 1, (size_t)stdout_len, stdout);
    subscript_rt_ctx_release(ctx);

    subscript_rt_context* cleared_ctx = subscript_rt_ctx_new();
    if (cleared_ctx == NULL) return 3;
    struct observed_trap cleared = {0};
    subscript_rt_ctx_set_trap_observer(cleared_ctx, observe, &cleared);
    subscript_rt_ctx_set_trap_observer(cleared_ctx, NULL, NULL);
    call_entry(cleared_ctx, subscript_init);
    call_entry(cleared_ctx, subscript_export_main);
    if (subscript_rt_ctx_trap_kind(cleared_ctx) == 0) return fail(cleared_ctx, 21);
    if (cleared.calls != 0) return fail(cleared_ctx, 22);
    subscript_rt_ctx_release(cleared_ctx);
    return 0;
}
"#,
        &crate::emit_c_without_main(&check_program(&program).unwrap())
            .unwrap()
            .host_header,
    )
    .unwrap();
    let run = run_c_aot_with_entry(&program, &entry);
    assert!(
        run.status.success(),
        "ship host exited with {}: {}",
        run.status,
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        run.stdout, b"start:1\nstart:2\ndone\n",
        "the first call must unwind and the cleared second call must finish"
    );
}

#[test]
fn ship_c_corpus_output_is_byte_identical_with_an_observer_registered() {
    let source = include_str!("../../corpus/accept/a01-hello.ts");
    let program = [SourceFile::new("a01-hello.ts", source)];
    let without = run_c_aot(&program).expect("a01 without observer");
    let entry = host_entry(
        r#"
#include <stdio.h>

static void observe(
    void* userdata, uint32_t kind, uint32_t pos_id,
    const uint8_t* message, uint64_t message_len) {
    (void)kind;
    (void)pos_id;
    (void)message;
    (void)message_len;
    *(uint32_t*)userdata += 1;
}

int main(void) {
    subscript_rt_context* ctx = subscript_rt_ctx_new();
    if (ctx == NULL) return 2;
    uint32_t calls = 0;
    subscript_rt_ctx_set_trap_observer(ctx, observe, &calls);
    subscript_rt_ctx_enter_script(ctx);
    subscript_init(ctx);
    subscript_rt_ctx_exit_script(ctx);
    subscript_rt_ctx_enter_script(ctx);
    subscript_export_main(ctx);
    subscript_rt_ctx_exit_script(ctx);
    if (subscript_rt_ctx_trap_kind(ctx) != 0 || calls != 0) {
        subscript_rt_ctx_release(ctx);
        return 3;
    }
    uint64_t len = 0;
    const uint8_t* bytes = subscript_rt_ctx_stdout(ctx, &len);
    if (len > 0) fwrite(bytes, 1, (size_t)len, stdout);
    subscript_rt_ctx_release(ctx);
    return 0;
}
"#,
        &crate::emit_c_without_main(&check_program(&program).unwrap())
            .unwrap()
            .host_header,
    )
    .unwrap();
    let with = run_c_aot_with_entry(&program, &entry);
    assert!(
        with.status.success(),
        "ship observer host exited with {}: {}",
        with.status,
        String::from_utf8_lossy(&with.stderr)
    );
    assert_eq!(with.stdout, without, "observer changed a01 stdout bytes");
    assert_eq!(with.stdout, b"hello\n");
}

#[test]
fn host_memory_accounting_agrees_on_count_and_measures_tier_bytes() {
    let program = sources(
        "class Cell {\n\
               value: i32;\n\
               constructor(value: i32) { this.value = value; }\n\
             }\n\
             export function main(): void {\n\
               const first: Cell = new Cell(1);\n\
               const deleted: Cell = new Cell(2);\n\
               const last: Cell = new Cell(first.value + 2);\n\
               Context.free(deleted);\n\
               if (last.value === 0) { print(\"unreachable\"); }\n\
             }\n",
    );
    let dev = crate::jit::memory_accounting_after_run(&program).expect("dev accounting");
    let entry = host_entry(
        r#"
#include <stdio.h>

static void call_entry(subscript_rt_context* ctx, subscript_main_entry entry) {
    subscript_rt_ctx_enter_script(ctx);
    entry(ctx);
    subscript_rt_ctx_exit_script(ctx);
}

int main(void) {
    subscript_rt_context* ctx = subscript_rt_ctx_new();
    if (ctx == NULL) return 2;
    call_entry(ctx, subscript_init);
    if (subscript_rt_ctx_trap_kind(ctx) == 0) call_entry(ctx, subscript_export_main);
    if (subscript_rt_ctx_trap_kind(ctx) != 0) {
        subscript_rt_ctx_release(ctx);
        return 3;
    }
    printf("%llu %llu %llu\n",
        (unsigned long long)subscript_rt_ctx_live_allocations(ctx),
        (unsigned long long)subscript_rt_ctx_live_bytes(ctx),
        (unsigned long long)subscript_rt_ctx_reserved_bytes(ctx));
    subscript_rt_ctx_release(ctx);
    return 0;
}
"#,
        &crate::emit_c_without_main(&check_program(&program).unwrap())
            .unwrap()
            .host_header,
    )
    .unwrap();
    let run = run_c_aot_with_entry(&program, &entry);
    assert!(
        run.status.success(),
        "ship accounting host exited with {}: {}",
        run.status,
        String::from_utf8_lossy(&run.stderr)
    );
    let values: Vec<u64> = String::from_utf8(run.stdout)
        .expect("ship accounting output is UTF-8")
        .split_whitespace()
        .map(|value| value.parse().expect("ship accounting integer"))
        .collect();
    assert_eq!(values.len(), 3);
    let ship = (values[0], values[1], values[2]);

    assert_eq!(dev.0, 2, "three allocations minus one delete");
    assert_eq!(ship.0, dev.0, "tiers disagree on live allocation count");
    assert_ne!(ship.1, dev.1, "live bytes unexpectedly agree across tiers");
    assert_ne!(
        ship.2, dev.2,
        "reserved bytes unexpectedly agree across tiers"
    );
    eprintln!(
        "host memory accounting: dev=({}, {}, {}), ship=({}, {}, {})",
        dev.0, dev.1, dev.2, ship.0, ship.1, ship.2
    );
}

#[test]
fn collect_drops_dead_lir_temporaries_on_both_tiers() {
    let program = sources(
        "const COUNT: i32 = 20000;\n\
             let round: i32 = 0;\n\
             let state: i32 = 0x12345678;\n\
             let checksum: i32 = 0;\n\
             class Node {\n\
               value: i32; s9: string; s41: string; s105: string; s233: string;\n\
               next: Node | null;\n\
               constructor(value: i32, s9: string, s41: string, s105: string,\n\
                           s233: string, next: Node | null) {\n\
                 this.value = value; this.s9 = s9; this.s41 = s41;\n\
                 this.s105 = s105; this.s233 = s233; this.next = next;\n\
               }\n\
             }\n\
             export function main(): void {\n\
               if (round < 6) {\n\
                 let keep: Node | null = null;\n\
                 let dropped: Node | null = null;\n\
                 let suffix: string = \"\";\n\
                 let s9: string = \"\"; let s41: string = \"\";\n\
                 let s105: string = \"\"; let s233: string = \"\";\n\
                 for (let i: i32 = 0; i < COUNT; i += 1) {\n\
                   state = state * 1664525 + 1013904223;\n\
                   const uid: i32 = round * COUNT + i;\n\
                   suffix = `${uid}`;\n\
                   s9 = suffix.padStart(9, \"a\");\n\
                   s41 = suffix.padStart(41, \"b\");\n\
                   s105 = suffix.padStart(105, \"c\");\n\
                   s233 = suffix.padStart(233, \"d\");\n\
                   if ((state & 3) !== 0) {\n\
                     keep = new Node(state, s9, s41, s105, s233, keep);\n\
                   } else {\n\
                     dropped = new Node(state, s9, s41, s105, s233, dropped);\n\
                   }\n\
                 }\n\
                 dropped = null; suffix = \"\"; s9 = \"\"; s41 = \"\";\n\
                 s105 = \"\"; s233 = \"\";\n\
                 Context.collect();\n\
                 let cursor: Node | null = keep;\n\
                 while (cursor !== null) {\n\
                   checksum = checksum * 31 + cursor.value;\n\
                   checksum += cursor.s9.length + cursor.s41.length;\n\
                   checksum += cursor.s105.length + cursor.s233.length;\n\
                   cursor = cursor.next;\n\
                 }\n\
                 round += 1;\n\
               } else {\n\
                 Context.collect();\n\
               }\n\
             }\n",
    );
    let expected = vec![75_005, 75_005, 75_005, 75_005, 75_005, 75_005, 5];
    let dev = crate::jit::live_allocations_after_main_calls(&program, expected.len())
        .expect("dev collect allocation probe");

    let entry = host_entry(
        r#"
#include <stdio.h>

static void call_entry(subscript_rt_context* ctx, subscript_main_entry entry) {
    subscript_rt_ctx_enter_script(ctx);
    entry(ctx);
    subscript_rt_ctx_exit_script(ctx);
}

int main(void) {
    subscript_rt_context* ctx = subscript_rt_ctx_new();
    if (ctx == NULL) return 2;
    call_entry(ctx, subscript_init);
    for (int i = 0; i < 7; i++) {
        call_entry(ctx, subscript_export_main);
        if (subscript_rt_ctx_trap_kind(ctx) != 0) {
            subscript_rt_ctx_release(ctx);
            return 3;
        }
        printf("%llu%c",
            (unsigned long long)subscript_rt_ctx_live_allocations(ctx),
            i == 6 ? '\n' : ' ');
    }
    subscript_rt_ctx_release(ctx);
    return 0;
}
"#,
        &crate::emit_c_without_main(&check_program(&program).unwrap())
            .unwrap()
            .host_header,
    )
    .unwrap();
    let run = run_c_aot_with_entry(&program, &entry);
    assert!(
        run.status.success(),
        "ship collect host exited with {}: {}",
        run.status,
        String::from_utf8_lossy(&run.stderr)
    );
    let ship = String::from_utf8(run.stdout)
        .expect("ship counts are UTF-8")
        .split_whitespace()
        .map(|value| value.parse::<u64>().expect("ship count is an integer"))
        .collect::<Vec<_>>();
    assert_eq!(dev, expected, "dev live allocation counts changed");
    assert_eq!(ship, expected, "ship live allocation counts changed");
    eprintln!("collect live allocations: dev={dev:?}, ship={ship:?}");
}

#[test]
fn host_allocation_attribution_reports_known_sites_on_both_tiers() {
    let program = sources(
        "class Cell {\n\
               value: i32;\n\
               constructor(value: i32) { this.value = value; }\n\
             }\n\
             export function main(): void {\n\
               const cell: Cell = new Cell(7);\n\
               const values: i32[] = [];\n\
               values.push(cell.value);\n\
             }\n",
    );
    let (dev, dev_positions) =
        crate::jit::allocation_attribution_after_run(&program).expect("dev attribution");

    let entry = host_entry(
        r#"
#include "allocation-metadata.h"
#include <stdio.h>
#include <string.h>

struct triple {
    uint32_t class_id;
    uint32_t pos_id;
    uint64_t bytes;
};

struct observed {
    struct triple triples[8];
    uint64_t count;
};

static void visit(
    void* userdata, uint32_t class_id, uint32_t pos_id,
    uint64_t payload_bytes) {
    struct observed* observed = (struct observed*)userdata;
    if (observed->count < 8) {
        struct triple* triple = &observed->triples[observed->count];
        triple->class_id = class_id;
        triple->pos_id = pos_id;
        triple->bytes = payload_bytes;
    }
    observed->count += 1;
}

static const char* class_name(uint32_t class_id) {
    for (uint64_t i = 0; i < subscript_alloc_class_count; ++i) {
        if (subscript_alloc_classes[i].class_id == class_id) {
            return subscript_alloc_classes[i].name;
        }
    }
    return NULL;
}

static void call_entry(subscript_rt_context* ctx, subscript_main_entry entry) {
    subscript_rt_ctx_enter_script(ctx);
    entry(ctx);
    subscript_rt_ctx_exit_script(ctx);
}

int main(void) {
    subscript_rt_context* ctx = subscript_rt_ctx_new();
    if (ctx == NULL) return 2;
    call_entry(ctx, subscript_init);
    if (subscript_rt_ctx_trap_kind(ctx) == 0) call_entry(ctx, subscript_export_main);
    if (subscript_rt_ctx_trap_kind(ctx) != 0) {
        subscript_rt_ctx_release(ctx);
        return 3;
    }

    struct observed observed = {0};
    uint64_t visited =
        subscript_rt_ctx_visit_live_allocations(ctx, visit, &observed);
    if (visited != 3 || observed.count != 3 ||
        subscript_rt_ctx_live_allocations(ctx) != 3) {
        subscript_rt_ctx_release(ctx);
        return 4;
    }

    for (uint64_t i = 0; i < observed.count; ++i) {
        const struct triple* triple = &observed.triples[i];
        if (triple->pos_id >= subscript_alloc_position_count) {
            subscript_rt_ctx_release(ctx);
            return 5;
        }
        const char* name = class_name(triple->class_id);
        const subscript_alloc_position_info* pos =
            &subscript_alloc_positions[triple->pos_id];
        uint32_t expected_line = 0;
        if (triple->class_id == 1 && name != NULL &&
            strcmp(name, "Cell") == 0) {
            expected_line = 6;
        } else if (triple->class_id == 4294967042u && name != NULL &&
                   strcmp(name, "Array") == 0) {
            expected_line = 7;
        } else if (triple->class_id == 4294967043u && name != NULL &&
                   strcmp(name, "ArrayData") == 0) {
            expected_line = 8;
        } else {
            subscript_rt_ctx_release(ctx);
            return 6;
        }
        if (strcmp(pos->file, "test.ts") != 0 ||
            pos->line != expected_line) {
            subscript_rt_ctx_release(ctx);
            return 7;
        }
        printf("%u %u %llu\n", triple->class_id, triple->pos_id,
               (unsigned long long)triple->bytes);
    }
    subscript_rt_ctx_release(ctx);
    return 0;
}
"#,
        &crate::emit_c_without_main(&check_program(&program).unwrap())
            .unwrap()
            .host_header,
    )
    .unwrap();
    let run = run_c_aot_with_entry(&program, &entry);
    assert!(
        run.status.success(),
        "ship attribution host exited with {}: {}",
        run.status,
        String::from_utf8_lossy(&run.stderr)
    );
    let values: Vec<u64> = String::from_utf8(run.stdout)
        .expect("ship attribution output is UTF-8")
        .split_whitespace()
        .map(|value| value.parse().expect("ship attribution integer"))
        .collect();
    assert_eq!(values.len(), 9);
    let mut ship: Vec<(u32, u32, u64)> = values
        .chunks_exact(3)
        .map(|v| (v[0] as u32, v[1] as u32, v[2]))
        .collect();
    ship.sort_unstable();

    // §112 rule 1 reserves id 0 on each tier, so the first script
    // site of each table is id 1. LIR ids place class members before
    // free functions, so the constructor's lifetime-check position
    // takes dev slot 1 before `main` contributes its allocation
    // positions. The source-resolution assertion below pins the
    // semantic sites behind these local ids.
    assert_eq!(
        dev,
        vec![(1, 2, 4), (0xFFFF_FF02, 3, 40), (0xFFFF_FF03, 6, 16),],
        "dev attribution triples changed"
    );
    assert_eq!(
        ship,
        vec![(1, 1, 16), (0xFFFF_FF02, 2, 48), (0xFFFF_FF03, 3, 16),],
        "ship attribution triples changed"
    );
    let dev_sites: Vec<(u32, &str, u32)> = dev
        .iter()
        .map(|&(class_id, pos_id, _)| {
            let pos = dev_positions
                .get(pos_id)
                .expect("the dev table holds every recorded id");
            (class_id, pos.file.as_str(), pos.line)
        })
        .collect();
    assert_eq!(
        dev_sites,
        vec![
            (1, "test.ts", 6),
            (0xFFFF_FF02, "test.ts", 7),
            (0xFFFF_FF03, "test.ts", 8),
        ],
        "dev position table did not resolve the known sites"
    );
    eprintln!("allocation attribution: dev={dev:?}, ship={ship:?}");
}

#[test]
fn allocation_corpus_object_request_counts_match_across_tiers() {
    let cases = [
        (
            "t26-allocation-failure-new",
            include_str!("../../corpus/trap/t26-allocation-failure-new.ts"),
            3,
        ),
        (
            "t28-allocation-failure-array-literal",
            include_str!("../../corpus/trap/t28-allocation-failure-array-literal.ts"),
            4,
        ),
        (
            "t29-allocation-failure-push-grow",
            include_str!("../../corpus/trap/t29-allocation-failure-push-grow.ts"),
            4,
        ),
        (
            "t30-allocation-failure-string-concat",
            include_str!("../../corpus/trap/t30-allocation-failure-string-concat.ts"),
            4,
        ),
        (
            "t31-allocation-failure-template",
            include_str!("../../corpus/trap/t31-allocation-failure-template.ts"),
            6,
        ),
        (
            "t32-allocation-failure-generator-frame",
            include_str!("../../corpus/trap/t32-allocation-failure-generator-frame.ts"),
            3,
        ),
        (
            "t33-allocation-failure-json-raw-new",
            include_str!("../../corpus/trap/t33-allocation-failure-json-raw-new.ts"),
            5,
        ),
    ];

    for (id, source, expected) in cases {
        let files = vec![SourceFile::new(format!("{id}.ts"), source)];
        let entry = host_entry(
            r#"
#include <stdio.h>

static void call_entry(subscript_rt_context* ctx, subscript_main_entry entry) {
    subscript_rt_ctx_enter_script(ctx);
    entry(ctx);
    subscript_rt_ctx_exit_script(ctx);
}

int main(void) {
    subscript_rt_context* ctx = subscript_rt_ctx_new();
    if (ctx == NULL) return 2;
    call_entry(ctx, subscript_init);
    if (subscript_rt_ctx_trap_kind(ctx) == 0) call_entry(ctx, subscript_export_main);
    if (subscript_rt_ctx_trap_kind(ctx) != 0) {
        subscript_rt_ctx_release(ctx);
        return 3;
    }
    printf("%llu\n",
        (unsigned long long)subscript_rt_ctx_live_allocations(ctx));
    subscript_rt_ctx_release(ctx);
    return 0;
}
"#,
            &crate::emit_c_without_main(&check_program(&files).unwrap())
                .unwrap()
                .host_header,
        )
        .unwrap();
        let dev = crate::jit::memory_accounting_after_run(&files)
            .expect("dev allocation count")
            .0;
        let run = run_c_aot_with_entry(&files, &entry);
        assert!(
            run.status.success(),
            "{id}: ship allocation-count host exited with {}: {}",
            run.status,
            String::from_utf8_lossy(&run.stderr)
        );
        let ship: u64 = String::from_utf8(run.stdout)
            .expect("ship count output is UTF-8")
            .trim()
            .parse()
            .expect("ship allocation count");
        eprintln!("{id}: object allocation requests dev={dev}, ship={ship}");
        assert_eq!(dev, expected, "{id}: dev exact allocation count changed");
        assert_eq!(ship, expected, "{id}: ship exact allocation count changed");
    }
}

#[test]
fn the_runtime_staticlib_resolves_to_a_file() {
    let path = runtime_staticlib_path().expect("runtime static library");
    assert!(path.is_file(), "{} must exist", path.display());
}

#[test]
fn trap_line_parsing_is_total() {
    let table = PositionTable::new();
    assert!(parse_trap(b"", &table, b"").is_none());
    assert!(parse_trap(b"something else\n", &table, b"").is_none());
    assert!(parse_trap(b"trap 999 0 unknown\n", &table, b"").is_none());
    let r = parse_trap(b"trap 2 0 pop() on an empty array\n", &table, b"before\n").expect("parsed");
    assert_eq!(r.rule, TrapKind::EmptyPop);
    // §112 rule 1: id 0 is the reserved entry of the table this
    // program carries.
    assert_eq!(r.pos, crate::position_table::no_script_site());
    assert!(r.message.contains("empty"));
    assert_eq!(r.stdout, b"before\n");
}
