//! §178 checker gates. Each case checks one mirror and one small script in memory.

use subscript_compiler::{check_program, hir, Diagnostic, RuleCode, SourceFile};

const HEADER: &str = "// @subscript-c-header include=\"completion.h\"\n";
const DIRECTIVE: &str = "// @subscript-c-completion function=\"read\" result=\"int32_t\"\n";
const DECL: &str = "declare function read(value: i32): Promise<i32>;\n";

fn check(mirror: &str, script: &str) -> Result<hir::Module, Vec<Diagnostic>> {
    check_program(&[
        SourceFile::ambient("completion.d.ts", format!("{HEADER}{mirror}")),
        SourceFile::new("completion.ts", script),
    ])
}

#[test]
fn direct_await_and_held_handle_are_async_origins() {
    for script in [
        "export async function main(): Promise<void> { const value = await read(1); print(`${value}`); }",
        "export async function main(): Promise<void> { const h = read(1); const value = await h; print(`${value}`); }",
        "export async function main(): Promise<void> { const h = read(1); await h; }",
    ] {
        check(&format!("{DIRECTIVE}{DECL}"), script).expect("observed or held completion");
    }
}

#[test]
fn dropped_call_has_s70_diagnostic_with_held_control() {
    let mirror = format!("{DIRECTIVE}{DECL}");
    check(
        &mirror,
        "export async function main(): Promise<void> { const h = read(1); await h; }",
    )
    .expect("held control");
    let errors = check(
        &mirror,
        "export async function main(): Promise<void> { read(1); }",
    )
    .expect_err("drop");
    assert!(errors.iter().any(|d| d.code == RuleCode::S013));
}

#[test]
fn promise_return_requires_completion_directive_with_same_signature_control() {
    let script = "export function main(): void {}";
    check(&format!("{DIRECTIVE}{DECL}"), script).expect("directive control");
    let errors = check(DECL, script).expect_err("missing source");
    assert!(errors.iter().any(|d| {
        d.code == RuleCode::S100 && d.message ==
        "foreign function `read` returns Promise<T> without a `@subscript-c-completion` directive"
    }));
}

fn invalid_mirrors() -> Vec<String> {
    vec![
        DIRECTIVE.to_string(),
        format!("{DIRECTIVE}{DIRECTIVE}{DECL}"),
        format!("{DIRECTIVE}declare function read(value: i32): i32;"),
        format!("{DIRECTIVE}declare function read(value: i32): Promise<u32>;"),
        "// @subscript-c-completion function=\"read\" result=\"char*\"\ndeclare function read(value: i32): Promise<string>;".into(),
        "// @subscript-c-completion function=\"read\" result=\"Opaque\"\ndeclare class Opaque { private __opaque: never; }\ndeclare function read(value: i32): Promise<Opaque>;".into(),
        "// @subscript-c-completion function=\"read\" result=\"ModeC\"\n// @subscript-c-cenum typedef=\"ModeC\" alias=\"Mode\"\ntype Mode = CEnum<{ idle: 0; active: 1 }>;\ndeclare function read(): Promise<Mode>;".into(),
    ]
}

#[test]
fn directives_reject_absent_duplicate_and_mismatched_results() {
    let script = "export function main(): void {}";
    check(&format!("{DIRECTIVE}{DECL}"), script).expect("valid control");
    for mirror in invalid_mirrors() {
        let errors = check(&mirror, script).expect_err(&mirror);
        assert!(
            errors.iter().any(|d| d.code == RuleCode::S100),
            "{errors:?}"
        );
    }
}

fn valid_mirrors() -> Vec<&'static str> {
    vec![
        "// @subscript-c-completion function=\"read\" result=\"void\"\ndeclare function read(): Promise<void>;",
        "// @subscript-c-completion function=\"read\" result=\"string\"\ndeclare function read(): Promise<string>;",
        "// @subscript-c-completion function=\"read\" result=\"u8[]\"\ndeclare function read(): Promise<u8[]>;",
        "// @subscript-c-completion function=\"read\" result=\"Pair\"\ndeclare class Pair { x: i32; y: f64; }\ndeclare function read(): Promise<Pair>;",
        "// @subscript-c-completion function=\"read\" result=\"Count\"\ntype Count = u32;\ndeclare function read(): Promise<Count>;",
        "// @subscript-c-completion function=\"read\" result=\"_Float16\"\ndeclare function read(): Promise<f16>;",
    ]
}

#[test]
fn scalar_alias_struct_and_void_results_match_the_directive() {
    for mirror in valid_mirrors() {
        check(
            mirror,
            "export async function main(): Promise<void> { const h = read(); await h; }",
        )
        .expect(mirror);
    }
}

#[path = "support/tsc.rs"]
mod tsc;

#[test]
fn stock_tsc_checks_every_checker_input_with_the_prelude() {
    // One stock tsc process checks every input in separate modules.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("root");
    let dir = std::env::temp_dir().join(format!("subscript-completion-tsc-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("directory");
    let scripts = [
        "export async function main(): Promise<void> { const value = await read(1); print(`${value}`); }",
        "export async function main(): Promise<void> { const h = read(1); const value = await h; print(`${value}`); }",
        "export async function main(): Promise<void> { const h = read(1); await h; }",
        "export async function main(): Promise<void> { read(1); }",
        "export function main(): void {}",
    ];
    let mut files = vec![root.join("prelude/lang.d.ts")];
    let mut cases: Vec<_> = scripts
        .iter()
        .map(|s| (format!("{DIRECTIVE}{DECL}"), s.to_string()))
        .collect();
    cases.push((
        DECL.to_string(),
        "export function main(): void {}".to_string(),
    ));
    cases.extend(
        invalid_mirrors()
            .into_iter()
            .map(|m| (m, "export function main(): void {}".to_string())),
    );
    cases.extend(valid_mirrors().into_iter().map(|m| {
        (
            m.to_string(),
            "export async function main(): Promise<void> { const h = read(); await h; }"
                .to_string(),
        )
    }));
    for (index, (mirror, script)) in cases.iter().enumerate() {
        let path = dir.join(format!("case{index}.ts"));
        std::fs::write(&path, format!("{HEADER}{mirror}\n{script}\nexport {{}};")).expect("source");
        files.push(path);
    }
    let config = dir.join("tsconfig.json");
    std::fs::write(&config, tsc::tsconfig(&files)).expect("config");
    let output = std::process::Command::new(tsc::tsc_binary(root))
        .args(["--project"])
        .arg(&config)
        .output()
        .expect("tsc");
    std::fs::remove_dir_all(&dir).expect("cleanup");
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn completion_struct_fields_require_recursive_scalar_layouts() {
    for (bad, good, field) in [
        ("// @subscript-c-callback typedef=\"Callback\"\ntype Callback = (value: i32) => void; declare class Result { value: Callback; extra: i32; }", "declare class Result { value: i32; extra: i32; }", "value"),
        ("declare class Result { text: string; value: i32; }", "declare class Result { text: i64; value: i32; }", "text"),
        ("declare class Result { values: i32[]; value: i32; }", "declare class Result { values: i64; value: i32; }", "values"),
        ("declare class Opaque { private __opaque: never; } declare class Result { value: Opaque | null; extra: i32; }", "declare class Result { value: i32; extra: i32; }", "value"),
        ("declare class Inner { text: string; } declare class Result { nested: Inner; }", "declare class Inner { text: i32; } declare class Result { nested: Inner; }", "text"),
        ("type Mode = CEnum<{ idle: 0; active: 1 }>; declare class Result { value: Mode; }", "declare class Result { value: i32; }", "value"),
    ] {
        let make = |decl: &str| format!("// @subscript-c-completion function=\"read\" result=\"Result\"\n{decl}\ndeclare function read(): Promise<Result>;");
        check(&make(good), "export function main(): void {}").expect("scalar layout control");
        let errors = check(&make(bad), "export function main(): void {}").expect_err("absorbed field");
        assert!(errors.iter().any(|e| e.message.contains("read") && e.message.contains("struct") && e.message.contains(field)), "{errors:?}");
    }
}
