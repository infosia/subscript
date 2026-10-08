//! Native C callees verify every register count, both struct types, and the following argument.
// This target uses only part of the shared native fixture helpers.
#[allow(dead_code)]
#[path = "../../tests/support/native_fixture.rs"]
mod native_fixture;
use subscript_codegen::{run_c_aot_with_native_libraries, run_jit_with_native_libraries};
use subscript_compiler::SourceFile;

fn native_composites_obey_register_pressure_for_zero_through_eight_arguments() {
    let mut mirror = String::from(
        "// @subscript-c-header include=\"abi-pressure.h\"\n\
declare class subscript_rt_completion { context_id: u64; operation_id: u64; constructor(context_id: u64, operation_id: u64); }\n\
declare class SubPressureValue { x: u64; y: u64; constructor(x: u64, y: u64); }\n",
    );
    let mut script = String::from("export function main(): void {\n");
    for (kind, ty) in [
        ("Endpoint", "subscript_rt_completion"),
        ("Value", "SubPressureValue"),
    ] {
        for count in 0..=8 {
            let mut params = (0..count).map(|i| format!("a{i}: i32")).collect::<Vec<_>>();
            params.extend([format!("value: {ty}"), "tail: i32".into()]);
            mirror.push_str(&format!(
                "declare function subPressure{kind}{count}({}): i32;\n",
                params.join(", ")
            ));
            let mut args = (0..count).map(|i| (i + 11).to_string()).collect::<Vec<_>>();
            args.extend([
                format!("new {ty}(1234567890123, 9876543210123)"),
                "97".into(),
            ]);
            script.push_str(&format!(
                "print(`${{subPressure{kind}{count}({})}}`);\n",
                args.join(", ")
            ));
        }
    }
    mirror.push_str("declare class SubPressureNarrow { x: i32; y: i32; z: i32; constructor(x: i32, y: i32, z: i32); }\n");
    for count in [8, 9] {
        let mut params = (0..count).map(|i| format!("a{i}: i32")).collect::<Vec<_>>();
        params.extend(["value: SubPressureNarrow".into(), "tail: i32".into()]);
        mirror.push_str(&format!(
            "declare function subPressureNarrow{count}({}): i32;\n",
            params.join(", ")
        ));
        let mut args = (0..count).map(|i| (i + 11).to_string()).collect::<Vec<_>>();
        args.extend(["new SubPressureNarrow(31, 37, 41)".into(), "97".into()]);
        script.push_str(&format!(
            "print(`${{subPressureNarrow{count}({})}}`);\n",
            args.join(", ")
        ));
    }
    script.push_str("}\n");
    let files = [
        SourceFile::ambient("pressure.d.ts", mirror),
        SourceFile::new("pressure.ts", script),
    ];
    let library = [native_fixture::fixture()
        .expect("native fixture")
        .archive_library()];
    let expected = "1\n".repeat(20).into_bytes();
    assert_eq!(
        run_jit_with_native_libraries(&files, &library).expect("dev JIT"),
        expected
    );
    assert_eq!(
        run_c_aot_with_native_libraries(&files, &library).expect("C AOT"),
        expected
    );
}

fn completion_after_seven_integer_arguments_in_both_tiers() {
    let files = [
        SourceFile::ambient(
            "interop.d.ts",
            include_str!("../../../corpus/interop/interop.generated.d.ts"),
        ),
        SourceFile::ambient(
            "completion.d.ts",
            include_str!("../../../corpus/interop/host-completion.generated.d.ts"),
        ),
        SourceFile::new(
            "completion.ts",
            r#"
export async function main(): Promise<void> {
  const device = subDeviceCreate(null);
  subRequestStart(device, 0, 1, new SubRequestInfo((message, a, b) => {}, null, null));
  const pending = subCompletionI32(device, 91);
  print(`${await subCompletionSeven(1, 2, 3, 4, 5, 6, 7)}`);
  print(`${subCompletionPump(device, 91, 0)}`);
  print(`${await pending}`);
  subDeviceRelease(device);
}
"#,
        ),
    ];
    let libraries = [native_fixture::fixture()
        .expect("fixture")
        .archive_library()];
    for output in [
        run_jit_with_native_libraries(&files, &libraries),
        run_c_aot_with_native_libraries(&files, &libraries),
    ] {
        assert_eq!(output.expect("native completion"), b"140\n0\n91\n");
    }
}

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

fn native_argument_kind_general_and_simd_pressure_sweep() {
    // 488 checks share the build-time archive. Each argument callee checks every argument.
    // SIMD kinds use general counts 0, 7, 8: empty, last available, and exhausted.
    // The contract requires rule-removal controls in compiler.md §179.2, acceptance 2.
    // Isolated warm Apple arm64 cost: 2.54 s for both tiers, excluding the Rust build.
    let files = [
        SourceFile::ambient("pressure.d.ts", subscript_interop_fixture::CLASS_MIRROR),
        SourceFile::new("pressure.ts", subscript_interop_fixture::CLASS_SCRIPT),
    ];
    let libraries = [
        native_fixture::fixture()
            .expect("fixture")
            .archive_library(),
        class_library(),
    ];
    let count: usize = subscript_interop_fixture::CLASS_COUNT
        .parse()
        .expect("count");
    assert_eq!(count, 488);
    let expected = "1\n".repeat(count).into_bytes();
    let jit = run_jit_with_native_libraries(&files, &libraries).expect("dev JIT");
    let aot = run_c_aot_with_native_libraries(&files, &libraries).expect("C AOT");
    for (tier, output) in [("dev JIT", jit), ("C AOT", aot)] {
        let lines = String::from_utf8(output.clone()).expect("UTF-8");
        let failures: Vec<_> = lines
            .lines()
            .enumerate()
            .filter(|(_, value)| *value != "1")
            .collect();
        assert_eq!(output, expected, "{tier} failures: {failures:?}");
    }
}

fn native_completion_result_type_sweep_in_both_tiers() {
    // Eighteen types cross four producers and three readers; four native half HFAs add three readers.
    // Each aggregate reads two elements, including a false boolean control.
    // Four producers and three readers cover separate completion storage and access paths.
    // Isolated warm Apple arm64 cost: 9.56 s for both tiers, excluding the Rust build.
    let files = [
        SourceFile::ambient(
            "interop.d.ts",
            include_str!("../../../corpus/interop/interop.generated.d.ts"),
        ),
        SourceFile::ambient("results.d.ts", subscript_interop_fixture::RESULT_MIRROR),
        SourceFile::new("results.ts", subscript_interop_fixture::RESULT_SCRIPT),
    ];
    let libraries = [
        native_fixture::fixture()
            .expect("fixture")
            .archive_library(),
        class_library(),
    ];
    let expected = "true\n".repeat(305).into_bytes();
    assert_eq!(
        run_jit_with_native_libraries(&files, &libraries).expect("dev JIT"),
        expected
    );
    assert_eq!(
        run_c_aot_with_native_libraries(&files, &libraries).expect("C AOT"),
        expected
    );
}

fn native_bool_storage_and_script_value_layout_in_both_tiers() {
    // One module per tier covers dirty native padding, writes, copies, arrays, nesting, and JSON.
    // The emitted assertions prove that ScriptBools has size 2 and alignment 1.
    // Isolated warm Apple arm64 cost: 1.15 s for both tiers, excluding the Rust build.
    let files = [
        SourceFile::ambient("bool.d.ts", subscript_interop_fixture::CLASS_MIRROR),
        SourceFile::new(
            "bool.ts",
            r#"
@ValueType
class ScriptBools {
    a: boolean; b: boolean;
    constructor(a: boolean, b: boolean) { this.a = a; this.b = b; }
}
@ValueType
class NestedBools {
    inner: ScriptBools;
    constructor(inner: ScriptBools) { this.inner = inner; }
}
export function main(): void {
    const native = sweepBoolReturn();
    print(`${native.a}:${native.b}:${native.c}`);
    const written = new SweepBoolPadded(true, 0, true);
    sweepBoolWrite(written);
    print(`${written.a}:${written.b}:${written.c}`);
    const value = new ScriptBools(false, true);
    const copy = value;
    value.a = true;
    const nested = new NestedBools(copy);
    const values = [copy, new ScriptBools(true, false)];
    print(`${nested.inner.a}:${nested.inner.b}:${values[0].a}:${values[1].b}`);
    print(JSON.stringify(copy));
    const stored = new SweepBoolPair(true, false);
    stored.a = !stored.a; stored.b = !stored.b;
    print(`${sweepBoolPairBytes(stored)}`);
}
"#,
        ),
    ];
    let libraries = [
        native_fixture::fixture()
            .expect("fixture")
            .archive_library(),
        class_library(),
    ];
    let expected =
        b"false:37:false\nfalse:37:false\nfalse:true:false:false\n{\"a\":false,\"b\":true}\n1\n";
    assert_eq!(
        run_jit_with_native_libraries(&files, &libraries).expect("dev JIT"),
        expected
    );
    assert_eq!(
        run_c_aot_with_native_libraries(&files, &libraries).expect("C AOT"),
        expected
    );
}

pub(super) fn run() {
    native_composites_obey_register_pressure_for_zero_through_eight_arguments();
    completion_after_seven_integer_arguments_in_both_tiers();
    native_argument_kind_general_and_simd_pressure_sweep();
    native_completion_result_type_sweep_in_both_tiers();
    native_bool_storage_and_script_value_layout_in_both_tiers();
    println!("abi_wide: 821 checks per tier, 0 failures");
}
