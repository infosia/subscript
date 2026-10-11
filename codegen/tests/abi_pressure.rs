//! Native ABI gate: one script module per tier and one build-time C archive.
#![cfg(not(all(windows, target_env = "msvc")))]
// This target uses only part of the shared native fixture helpers.
#[allow(dead_code)]
#[path = "support/native_fixture.rs"]
mod native_fixture;
use subscript_codegen::{run_c_aot_with_native_libraries, run_jit_with_native_libraries};
use subscript_compiler::SourceFile;

fn class_library() -> subscript_codegen::NativeLibrary {
    let directory = std::path::PathBuf::from(subscript_interop_fixture::CLASS_DIRECTORY);
    // SAFETY: the fixture archive links these C callees with static lifetime.
    unsafe {
        subscript_codegen::NativeLibrary::new(
            vec![directory.clone()],
            vec![directory.join("libsubscript_interop_fixture.a")],
            subscript_interop_fixture::class_symbols(),
        )
    }
}

fn boundary_gate_in_both_tiers() {
    // Isolated Apple arm64 cost: 4.284 s for both tiers, excluding the Rust build.
    // The 459 allocator checks and 90 boundary checks share one module per tier.
    // Native completion bytes require three readers and real C producers.
    // compiler.md §179.2, acceptance 2 requires independent rule-removal controls.
    let files = [
        SourceFile::ambient(
            "interop.d.ts",
            include_str!("../../corpus/interop/interop.generated.d.ts"),
        ),
        SourceFile::ambient(
            "boundary.d.ts",
            include_str!("../../corpus/interop/boundary-values.generated.d.ts"),
        ),
        SourceFile::ambient(
            "class.d.ts",
            format!(
                "{}\n{}",
                subscript_interop_fixture::CLASS_MIRROR,
                subscript_interop_fixture::GATE_MIRROR
            ),
        ),
        SourceFile::new("gate.ts", subscript_interop_fixture::GATE_SCRIPT),
    ];
    let libraries = [
        native_fixture::fixture()
            .expect("fixture")
            .archive_library(),
        class_library(),
    ];
    let count: usize = subscript_interop_fixture::GATE_COUNT
        .parse()
        .expect("gate count");
    assert_eq!(count, 549, "required allocator and boundary witnesses");
    let expected = "1\n".repeat(count).into_bytes();
    // Run both tiers before assertions, so each negative control reports both results.
    let outputs = [
        run_jit_with_native_libraries(&files, &libraries),
        run_c_aot_with_native_libraries(&files, &libraries),
    ];
    let mut failures = Vec::new();
    for (tier, output) in ["JIT", "C AOT"].into_iter().zip(outputs) {
        match output {
            Ok(output) => {
                let wrong = String::from_utf8_lossy(&output)
                    .lines()
                    .filter(|line| *line != "1")
                    .count();
                eprintln!("{tier}: {wrong} wrong checks / {count}");
                if output != expected {
                    failures.push(format!("{tier}: {wrong} wrong checks"));
                }
            }
            Err(error) => failures.push(format!("{tier}: module refused: {error}")),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn raw_bool_bytes_are_canonical_in_both_tiers() {
    // One module per tier tests raw, nested, and fixed-array bool storage (§179.1 rule 6).
    // Isolated Apple arm64 cost: 0.567 s for both tiers, excluding the Rust build.
    let files = [SourceFile::new(
        "raw-bools.ts",
        r#"
@ValueType
class B2 { a: boolean; b: boolean; constructor(a: boolean, b: boolean) { this.a = a; this.b = b; } }
@ValueType
class Nested { head: u32; inner: B2; tail: u8; constructor(head: u32, inner: B2, tail: u8) { this.head = head; this.inner = inner; this.tail = tail; } }
@ValueType
class ArrayBools { head: u16; values: FixedArray<B2, 2>; constructor(head: u16, values: FixedArray<B2, 2>) { this.head = head; this.values = values; } }
export function main(): void {
    const v = Context.fromBytes<B2>([2, 255], 0);
    print(`${v.a} ${!v.a} ${v.a == true} ${v.a ? 10 : 20}`);
    print(Context.bytesOf<B2>(v).join(","));
    const control = Context.fromBytes<B2>([0, 1], 0);
    print(`${control.a} ${!control.a} ${control.a == true} ${control.a ? 10 : 20}`);
    print(Context.bytesOf<B2>(control).join(","));
    const nested = Context.fromBytes<Nested>([7, 0, 0, 0, 2, 255, 9, 0], 0);
    print(`${nested.head} ${nested.inner.a} ${nested.inner.b} ${nested.tail}`);
    print(Context.bytesOf<Nested>(nested).join(","));
    const array = Context.fromBytes<FixedArray<B2, 2>>([2, 255, 0, 1], 0);
    print(`${array[0].a} ${array[0].b} ${array[1].a} ${array[1].b}`);
    print(Context.bytesOf<FixedArray<B2, 2>>(array).join(","));
    const enclosed = Context.fromBytes<ArrayBools>([7, 0, 2, 255, 0, 1], 0);
    print(`${enclosed.head} ${enclosed.values[0].a} ${enclosed.values[1].a}`);
    print(Context.bytesOf<ArrayBools>(enclosed).join(","));
}
"#,
    )];
    let expected = b"true false true 10\n1,1\nfalse true false 20\n0,1\n7 true true 9\n7,0,0,0,1,1,9,0\ntrue true false true\n1,1,0,1\n7 true false\n7,0,1,1,0,1\n";
    let outputs = [
        run_jit_with_native_libraries(&files, &[]),
        run_c_aot_with_native_libraries(&files, &[]),
    ];
    let mut failures = Vec::new();
    for (tier, output) in ["JIT", "C AOT"].into_iter().zip(outputs) {
        let output = output.expect(tier);
        if output != expected {
            failures.push(format!("{tier}: {}", String::from_utf8_lossy(&output)));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn header_only_boundary_value_runs_in_both_tiers() {
    // Isolated Apple arm64 cost: 0.452539 s, excluding the Rust build.
    // Cost: one small module per tier and one C compile; no native archive build.
    let directory =
        std::env::temp_dir().join(format!("subscript-header-only-{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("header directory");
    std::fs::write(
        directory.join("only-value.h"),
        "#include <stdbool.h>\ntypedef struct { bool a; bool b; } OnlyValue;\n",
    )
    .expect("header without functions");
    let files = [
        SourceFile::ambient(
            "only-value.d.ts",
            "// @subscript-c-header include=\"only-value.h\"\ndeclare class OnlyValue { a: boolean; b: boolean; constructor(a: boolean, b: boolean); }",
        ),
        SourceFile::new(
            "only-value.ts",
            "export function main(): void { const value = new OnlyValue(true, false); print(`${value.a}`); }",
        ),
    ];
    let hir = subscript_compiler::check_program(&files).expect("header-only mirror");
    let class = hir
        .classes
        .iter()
        .find(|class| class.name == "OnlyValue")
        .expect("boundary class");
    assert_eq!(class.boundary_header.as_deref(), Some("only-value.h"));
    let lir = subscript_codegen::lir::lower_module(&hir).expect("header identity lowers");
    assert!(lir.foreign_functions.is_empty());
    let class = lir
        .classes
        .iter()
        .find(|class| class.source_name == "OnlyValue")
        .expect("LIR boundary class");
    assert_eq!(class.boundary_header.as_deref(), Some("only-value.h"));
    let text = subscript_compiler::lir_text::print_module(&lir);
    assert!(text.contains("header=\"only-value.h\""));
    // SAFETY: this header-only library supplies no symbols or archives.
    let library =
        unsafe { subscript_codegen::NativeLibrary::new(vec![directory.clone()], vec![], vec![]) };
    let libraries = [library];
    let outputs = [
        run_jit_with_native_libraries(&files, &libraries),
        run_c_aot_with_native_libraries(&files, &libraries),
    ];
    std::fs::remove_dir_all(directory).expect("remove header directory");
    for (tier, output) in ["JIT", "C AOT"].into_iter().zip(outputs) {
        assert_eq!(output.expect(tier), b"true\n", "{tier}");
    }
}

fn headerless_boundary_value_runs_in_both_tiers() {
    // Isolated Apple arm64 cost: 0.464 s, excluding the Rust build.
    // Cost: one small module per tier and one C compile; no native archive build.
    let files = [
        SourceFile::ambient("pt.d.ts", "declare class Pt { a: boolean; b: u8; }"),
        SourceFile::new(
            "pt.ts",
            "function field(p: Pt): u8 { return p.b; } export function main(): void { print(\"headerless\"); }",
        ),
    ];
    let hir = subscript_compiler::check_program(&files).expect("headerless declaration checks");
    let program = subscript_codegen::emit_c(&hir).expect("headerless class emits");
    assert!(program.source.contains("\"value class size\""));
    assert!(!program.source.contains("\"boundary class size\""));
    let outputs = [
        run_jit_with_native_libraries(&files, &[]),
        run_c_aot_with_native_libraries(&files, &[]),
    ];
    for (tier, output) in ["JIT", "C AOT"].into_iter().zip(outputs) {
        assert_eq!(output.expect(tier), b"headerless\n", "{tier}");
    }
}

fn fixed_array_boundary_pointer_compares_host_layout() {
    // Isolated Apple arm64 cost: 0.097 s, excluding the Rust build.
    // Cost: two C compiles compare an exact mirror and an independently stale mirror.
    let directory = std::path::PathBuf::from(subscript_interop_fixture::CLASS_DIRECTORY);
    for (count, succeeds) in [(4, true), (3, false)] {
        let files = [
            SourceFile::ambient("mat.d.ts", format!("// @subscript-c-header include=\"class-sweep.h\"\ndeclare class Mat {{ m: FixedArray<f32, {count}>; on: boolean; }} declare function gateMat(value: Mat | null): i32;")),
            SourceFile::new("mat.ts", "export function main(): void { gateMat(null); }"),
        ];
        let hir = subscript_compiler::check_program(&files).expect("mirror checks");
        let program = subscript_codegen::emit_c(&hir).expect("emit mirror");
        assert!(
            program.source.contains("\"boundary class size\""),
            "{}",
            program.source
        );
        assert!(
            program.source.contains("offsetof(Mat, on)"),
            "{}",
            program.source
        );
        let scratch =
            std::env::temp_dir().join(format!("subscript-mat-{count}-{}", std::process::id()));
        std::fs::create_dir_all(&scratch).expect("C directory");
        std::fs::write(scratch.join("program.c"), &program.source).expect("C source");
        std::fs::write(scratch.join("program.h"), &program.host_header).expect("C header");
        let compiler = subscript_codegen::host_c_compiler().expect("C compiler");
        let output = compiler
            .command()
            .args(["-std=c11", "-c"])
            .arg(scratch.join("program.c"))
            .arg("-I")
            .arg(&directory)
            .arg("-I")
            .arg(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../corpus/interop"))
            .arg("-o")
            .arg(scratch.join("program.o"))
            .output()
            .expect("C compile");
        std::fs::remove_dir_all(scratch).expect("remove C directory");
        let error = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.success(), succeeds, "{error}");
        if !succeeds {
            assert!(error.contains("boundary class size"), "{error}");
            assert!(error.contains("boundary class field offset"), "{error}");
        }
    }
}

// compiler.md §190.1 rule 3: phase 1 runs in parallel; each phase 2 test
// starts a dev run with a native library or a file provider and runs in
// its own process, on that process's main thread.
pub(super) fn main() -> std::process::ExitCode {
    crate::main_thread::run(&crate::main_thread_tests![
        parallel: [
            raw_bool_bytes_are_canonical_in_both_tiers,
            headerless_boundary_value_runs_in_both_tiers,
            fixed_array_boundary_pointer_compares_host_layout,
        ],
        main_thread: [
            boundary_gate_in_both_tiers,
            header_only_boundary_value_runs_in_both_tiers,
        ],
    ])
}
