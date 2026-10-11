//! Function reference map reads and field calls agree across engines (compiler.md §123).

// compiler.md §190.1 rule 3: a test function that the phase list does
// not name fails the build.
#![deny(dead_code)]

#[path = "support/main_thread.rs"]
mod main_thread;

#[cfg(not(all(windows, target_env = "msvc")))]
#[path = "support/native_fixture.rs"]
mod native_fixture;

#[path = "../../compiler/tests/corpus/interop.rs"]
#[allow(dead_code)]
mod interop;

use subscript_codegen::{
    interpreter::interpret, lir::lower_module, run_c_aot, run_c_aot_with_native_libraries, run_jit,
    run_jit_with_native_libraries, NativeLibrary,
};
use subscript_compiler::{check_program, SourceFile};

fn nullable_map_values_and_field_reads_keep_their_evaluation_order() {
    let files = [SourceFile::new(
        "test.ts",
        r#"
        function first(x: i32): i32 { return x + 10; }
        function second(x: i32): i32 { return x + 100; }
        class H { cb: (x: i32) => i32 = first; }
        function replace(h: H): i32 { h.cb = second; return 3; }
        function read(m: Map<string, ((x: i32) => i32) | null>, key: string): void {
            const f = m.get(key);
            print(`${f === null} ${f !== null ? f(2) : -1}`);
        }
        export function main(): void {
            const h = new H();
            print(`${h.cb(replace(h))} ${h.cb(3)}`);
            const m = new Map<string, ((x: i32) => i32) | null>();
            m.set("hit", first);
            m.set("null", null);
            read(m, "hit");
            read(m, "miss");
            read(m, "null");
            const fallback = m.getOr("miss", second);
            print(`${fallback !== null ? fallback(2) : -1}`);
            const keys: string[] = ["hit", "miss", "null", "hit"];
            for (const key of keys) {
                const f = m.get(key);
                print(`${f === null} ${f !== null ? f(2) : -1}`);
            }
        }
        "#,
    )];
    let expected =
        b"13 103\nfalse 12\ntrue -1\ntrue -1\n102\nfalse 12\ntrue -1\ntrue -1\nfalse 12\n";
    let module =
        lower_module(&check_program(&files).expect("checked source")).expect("lowered source");
    assert_eq!(interpret(&module).expect("interpreter"), expected);
    assert_eq!(run_jit(&files).expect("dev JIT"), expected);
    assert_eq!(run_c_aot(&files).expect("ship C"), expected);
}

#[cfg(not(all(windows, target_env = "msvc")))]
fn mirror_program(source: &str) -> Vec<SourceFile> {
    vec![
        interop::mirror("interop.generated.d.ts", SourceFile::ambient),
        SourceFile::new("test.ts", source),
    ]
}

fn assert_three_engines(files: &[SourceFile], expected: &[u8], libraries: &[NativeLibrary]) {
    let module =
        lower_module(&check_program(files).expect("checked source")).expect("lowered source");
    assert_eq!(interpret(&module).expect("interpreter"), expected);
    assert_eq!(
        run_jit_with_native_libraries(files, libraries).expect("dev JIT"),
        expected
    );
    assert_eq!(
        run_c_aot_with_native_libraries(files, libraries).expect("ship C"),
        expected
    );
}

#[cfg(not(all(windows, target_env = "msvc")))]
fn boundary_function_field_reads_the_same_pair_as_a_local_copy() {
    let files = mirror_program(
        r#"
        export function main(): void {
            const info = new SubRequestInfo((message, a, b) => { print(message); }, null, null);
            info.callback = (message, a, b) => { print(`updated ${message}`); };
            const cb = info.callback;
            cb("local", null, null);
            info.callback("field", null, null);
        }
    "#,
    );
    let libraries = [native_fixture::fixture()
        .expect("interop fixture")
        .library()];
    assert_three_engines(&files, b"updated local\nupdated field\n", &libraries);
}

fn static_nullable_function_path_calls_and_copies() {
    let files = [SourceFile::new(
        "test.ts",
        r#"
        function double(x: i32): i32 { return x * 2; }
        class S { static opt: ((x: i32) => i32) | null = double; }
        function read(): void {
            if (S.opt !== null) {
                print(`${S.opt(5)}`);
                if (S.opt !== null) {
                    const f = S.opt;
                    print(`${f(6)}`);
                }
            } else { print("null"); }
        }
        export function main(): void { read(); S.opt = null; read(); }
    "#,
    )];
    assert_three_engines(&files, b"10\n12\nnull\n", &[]);
}

fn nullable_reference_map_get_or_accepts_a_null_default() {
    let files = [SourceFile::new(
        "test.ts",
        r#"
        class C { value: i32 = 7; }
        export function main(): void {
            const m = new Map<i32, C | null>();
            m.set(1, new C()); m.set(2, null);
            const hit = m.getOr(1, null);
            print(`${hit !== null ? hit.value : -1} ${m.getOr(2, null) === null} ${m.getOr(3, null) === null}`);
        }
    "#,
    )];
    assert_three_engines(&files, b"7 true true\n", &[]);
}

#[cfg(not(all(windows, target_env = "msvc")))]
fn nullable_boundary_map_get_or_accepts_a_null_default() {
    let files = mirror_program(
        r#"
        export function main(): void {
            const m = new Map<i32, SubByValueI64Pair | null>();
            m.set(1, new SubByValueI64Pair(7, 8)); m.set(2, null);
            const hit = m.getOr(1, null);
            print(`${hit !== null} ${hit !== null ? hit.a : -1 as i64} ${hit !== null ? hit.b : -1 as i64} ${m.getOr(2, null) === null} ${m.getOr(3, null) === null}`);
        }
    "#,
    );
    let libraries = [native_fixture::fixture()
        .expect("interop fixture")
        .library()];
    assert_three_engines(&files, b"true 7 8 true true\n", &libraries);
}

// compiler.md §190.1 rule 3: phase 1 runs in parallel; each phase 2 test
// starts a dev run with a native library or a file provider and runs in
// its own process, on that process's main thread.
fn main() -> std::process::ExitCode {
    main_thread::run(&main_thread_tests![
        parallel: [
            nullable_map_values_and_field_reads_keep_their_evaluation_order,
            static_nullable_function_path_calls_and_copies,
            nullable_reference_map_get_or_accepts_a_null_default,
        ],
        main_thread: [
            #[cfg(not(all(windows, target_env = "msvc")))]
            boundary_function_field_reads_the_same_pair_as_a_local_copy,
            #[cfg(not(all(windows, target_env = "msvc")))]
            nullable_boundary_map_get_or_accepts_a_null_default,
        ],
    ])
}
