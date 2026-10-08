//! Native C callees verify every register count, both struct types, and the following argument.
#![cfg(not(all(windows, target_env = "msvc")))]
#[path = "support/native_fixture.rs"]
mod native_fixture;
use subscript_codegen::{run_c_aot_with_native_libraries, run_jit_with_native_libraries};
use subscript_compiler::SourceFile;

#[test]
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
    let library = [native_fixture::fixture().expect("native fixture").library()];
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

#[test]
fn completion_after_seven_integer_arguments_in_both_tiers() {
    let files = [
        SourceFile::ambient(
            "interop.d.ts",
            include_str!("../../corpus/interop/interop.generated.d.ts"),
        ),
        SourceFile::ambient(
            "completion.d.ts",
            include_str!("../../corpus/interop/host-completion.generated.d.ts"),
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
    let libraries = [native_fixture::fixture().expect("fixture").library()];
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

#[test]
fn native_argument_kind_general_and_simd_pressure_sweep() {
    // 306 callees share the build-time archive. Each checks every argument.
    // SIMD kinds use general counts 0, 7, 8: empty, last available, and exhausted.
    // The NGRN-blind control fails two checks; the NSRN-blind control fails 36.
    // Isolated warm Apple arm64 cost: 1.938 s for both tiers, excluding the Rust build.
    let files = [
        SourceFile::ambient("pressure.d.ts", subscript_interop_fixture::CLASS_MIRROR),
        SourceFile::new("pressure.ts", subscript_interop_fixture::CLASS_SCRIPT),
    ];
    let libraries = [
        native_fixture::fixture().expect("fixture").library(),
        class_library(),
    ];
    let count: usize = subscript_interop_fixture::CLASS_COUNT
        .parse()
        .expect("count");
    assert_eq!(count, 306);
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

#[test]
fn native_completion_result_type_sweep_in_both_tiers() {
    // Fifteen result types cross four producers and three readers in one module per tier.
    // Each aggregate reads two elements, including a false boolean control.
    // Four producers and three readers cover separate completion storage and access paths.
    // Isolated warm Apple arm64 cost: 5.745 s for both tiers, excluding the Rust build.
    let files = [
        SourceFile::ambient(
            "interop.d.ts",
            include_str!("../../corpus/interop/interop.generated.d.ts"),
        ),
        SourceFile::ambient("results.d.ts", subscript_interop_fixture::RESULT_MIRROR),
        SourceFile::new("results.ts", subscript_interop_fixture::RESULT_SCRIPT),
    ];
    let libraries = [
        native_fixture::fixture().expect("fixture").library(),
        class_library(),
    ];
    let expected = "true\n".repeat(241).into_bytes();
    assert_eq!(
        run_jit_with_native_libraries(&files, &libraries).expect("dev JIT"),
        expected
    );
    assert_eq!(
        run_c_aot_with_native_libraries(&files, &libraries).expect("C AOT"),
        expected
    );
}
