use subscript_codegen::{
    interpreter::{interpret, InterpretError},
    lir::lower_module,
    run_c_aot, run_jit, RunError,
};
use subscript_compiler::{check_program, SourceFile};
use subscript_runtime::TrapKind;

#[test]
fn date_additions_match_the_golden_on_three_engines() {
    let source = include_str!("../../corpus/accept/a266-date-es2022-additions.ts");
    let expected = include_bytes!("../../corpus/accept/a266-date-es2022-additions.expected");
    let files = [SourceFile::new("test.ts", source)];
    let module = check_program(&files).unwrap();
    let lir = lower_module(&module).unwrap();
    assert_eq!(interpret(&lir).unwrap(), expected);
    assert_eq!(run_jit(&files).unwrap(), expected);
    assert_eq!(run_c_aot(&files).unwrap(), expected);
}

#[test]
fn date_to_json_keeps_the_iso_year_range_on_three_engines() {
    for (ms, year) in [(253_402_300_800_000_i64, 10000), (-62_198_755_200_000, -1)] {
        let source = format!("export function main(): void {{ print(new Date({ms}).toJSON()); }}");
        let files = [SourceFile::new("test.ts", source)];
        let message = format!("toISOString requires a year in 0000-9999, got year {year}");
        for result in [run_jit(&files), run_c_aot(&files)] {
            let Err(RunError::Trap(report)) = result else {
                panic!("expected DateRange, got {result:?}")
            };
            assert_eq!(report.rule, TrapKind::DateRange);
            assert_eq!(report.message, message);
            assert_eq!(report.pos.line, 1);
        }
        let module = check_program(&files).unwrap();
        let lir = lower_module(&module).unwrap();
        let error = interpret(&lir).unwrap_err();
        let InterpretError::Execution { source, .. } = error else {
            panic!("expected execution error")
        };
        let InterpretError::Trap {
            kind,
            message: actual,
            ..
        } = *source
        else {
            panic!("expected DateRange")
        };
        assert_eq!(kind, TrapKind::DateRange.to_string());
        assert_eq!(actual, message);
    }
}

#[test]
fn infinity_can_be_shadowed_and_date_copy_evaluates_once() {
    let files = [SourceFile::new(
        "test.ts",
        r#"
        function date(): Date { print("once"); return new Date(42); }
        function local(): void { const Infinity: i32 = 7; print(`${Infinity}`); }
        export function main(): void {
            print(`${new Date(date()).valueOf()}`);
            local();
            print(`${Infinity} ${-Infinity}`);
        }
    "#,
    )];
    let expected = b"once\n42\n7\nInfinity -Infinity\n";
    let module = check_program(&files).unwrap();
    let lir = lower_module(&module).unwrap();
    assert_eq!(interpret(&lir).unwrap(), expected);
    assert_eq!(run_jit(&files).unwrap(), expected);
    assert_eq!(run_c_aot(&files).unwrap(), expected);
}

#[test]
fn utc_string_allocation_failure_stops_the_call() {
    use subscript_codegen::{run_c_aot_with_alloc_failure, run_jit_with_alloc_failure};
    let files = [SourceFile::new(
        "test.ts",
        r#"
        export function main(): void {
            print(new Date(0).toUTCString());
            unreachable();
        }
    "#,
    )];
    for result in [
        run_jit_with_alloc_failure(&files, 1),
        run_c_aot_with_alloc_failure(&files, 1),
    ] {
        let Err(RunError::Trap(report)) = result else {
            panic!("expected allocation trap: {result:?}")
        };
        assert_eq!(report.rule, TrapKind::AllocationFailure);
        assert_eq!(report.pos.line, 3);
        assert!(report.stdout.is_empty());
    }
    for result in [run_jit(&files), run_c_aot(&files)] {
        let Err(RunError::Trap(report)) = result else {
            panic!("expected firing control: {result:?}")
        };
        assert_eq!(report.stdout, b"Thu, 01 Jan 1970 00:00:00 GMT\n");
        assert_eq!(report.rule, TrapKind::UnreachableReached);
    }
}
