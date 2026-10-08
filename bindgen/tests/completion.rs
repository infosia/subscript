//! Completion gates (§178). Each rejection compares one accepted header of the same shape.
//! Each ordinary rejection uses two in-memory libclang parses and no child process.
//! The tracking note states the parse count and measured cost of each gate.

use subscript_bindgen::{generate_for_header, generate_with_options, BindOptions};

const TYPES: &str = "
#include <stdint.h>
#include <stddef.h>
typedef struct { uint64_t context_id; uint64_t operation_id; } subscript_rt_completion;
typedef struct { int32_t x; double y; } SubValue;
typedef struct SubHandle_T *SubHandle;
typedef struct { const char *data; size_t length; } SubString;
typedef struct { const int32_t *data; size_t count; } SubArray;
typedef uint32_t SubScalar;
typedef enum { SubZero = 0 } SubEnum;
typedef int32_t *SubPointer;
";
const FUNCTION: &str = "void read(int32_t id, subscript_rt_completion endpoint);";

fn options(result: &str) -> BindOptions {
    BindOptions::new().with_completion("read", result)
}

fn header(function: &str) -> String {
    format!("{TYPES}\n{function}")
}

fn accepted() {
    generate_with_options(&header(FUNCTION), "host.h", &options("int32_t")).expect("accepted twin");
}

fn rejects(function: &str, selections: &BindOptions, message: &str) {
    accepted();
    let error =
        generate_with_options(&header(function), "host.h", selections).expect_err("reject twin");
    assert!(error.0.contains("read"), "{}", error.0);
    assert!(error.0.contains(message), "{}", error.0);
}

#[test]
fn scalar_struct_and_void_mirrors_carry_exact_provenance() {
    for (c, ts) in [
        ("int32_t", "i32"),
        ("SubValue", "SubValue"),
        ("void", "void"),
        ("double", "f64"),
        ("SubScalar", "SubScalar"),
        ("SubEnum", "SubEnum"),
    ] {
        let mirror =
            generate_with_options(&header(FUNCTION), "host.h", &options(c)).expect("mirror");
        assert!(
            mirror.contains(&format!("declare function read(id: i32): Promise<{ts}>;")),
            "{mirror}"
        );
        assert_eq!(
            mirror
                .lines()
                .filter(|line| line.starts_with("// @subscript-c-completion"))
                .collect::<Vec<_>>(),
            [format!(
                "// @subscript-c-completion function=\"read\" result=\"{c}\""
            )]
        );
        assert!(!mirror.contains("declare class subscript_rt_completion"));
        assert!(mirror.contains("declare class SubValue"));
    }
}

#[test]
fn endpoint_without_selection_is_rejected() {
    rejects(FUNCTION, &BindOptions::new(), "no `--completion` selection");
}

#[test]
fn absent_function_is_rejected() {
    rejects(
        "void other(int32_t id);",
        &options("int32_t"),
        "no such function",
    );
}

#[test]
fn missing_endpoint_is_rejected() {
    rejects(
        "void read(int32_t id);",
        &options("int32_t"),
        "trailing by-value",
    );
}

#[test]
fn non_void_return_is_rejected() {
    rejects(
        "int32_t read(int32_t id, subscript_rt_completion endpoint);",
        &options("int32_t"),
        "must return `void`",
    );
}

#[test]
fn non_trailing_endpoint_is_rejected() {
    for selections in [BindOptions::new(), options("int32_t")] {
        rejects(
            "void read(subscript_rt_completion endpoint, int32_t id);",
            &selections,
            "must be last",
        );
    }
}

#[test]
fn two_endpoints_are_rejected() {
    rejects(
        "void read(subscript_rt_completion first, subscript_rt_completion last);",
        &options("int32_t"),
        "must be last",
    );
}

#[test]
fn pointer_endpoint_is_rejected() {
    rejects(
        "void read(int32_t id, subscript_rt_completion *endpoint);",
        &options("int32_t"),
        "trailing by-value",
    );
}

#[test]
fn invalid_results_are_rejected() {
    accepted();
    for result in [
        "SubHandle",
        "int32_t*",
        "const char *",
        "string",
        "SubString",
        "SubArray",
        "SubPointer",
        "Absent",
        "subscript_rt_completion",
    ] {
        let error = generate_with_options(&header(FUNCTION), "host.h", &options(result))
            .expect_err("invalid result");
        assert!(
            error.0.contains("read") && error.0.contains("outside §178 rule 6"),
            "{}",
            error.0
        );
    }
}

#[test]
fn external_result_does_not_map_to_a_boundary_class() {
    let source = format!(
        "{}\n/* @subscript-external SubExternal */\nvoid useExternal(SubExternal value);",
        header(FUNCTION)
    );
    generate_with_options(&source, "host.h", &options("SubValue")).expect("accepted twin");
    let error = generate_with_options(&source, "host.h", &options("SubExternal"))
        .expect_err("external result");
    assert!(
        error.0.contains("read") && error.0.contains("outside §178 rule 6"),
        "{}",
        error.0
    );
}

#[test]
fn duplicate_selection_is_rejected() {
    rejects(
        FUNCTION,
        &options("int32_t").with_completion("read", "void"),
        "duplicate",
    );
}

#[test]
fn unknown_runtime_endpoint_resolves_but_other_unknown_types_fail() {
    let source = "void read(int id, subscript_rt_completion endpoint);";
    let mirror = generate_with_options(source, "host.h", &options("int32_t"))
        .expect("runtime forward declaration");
    assert!(mirror.contains("declare function read(id: i32): Promise<i32>;"));
    let error = generate_for_header(source, "host.h").expect_err("selection required");
    assert!(
        error.0.contains("read") && error.0.contains("no `--completion` selection"),
        "{}",
        error.0
    );
    let error = generate_with_options(
        "void read(Unknown id, subscript_rt_completion endpoint);",
        "host.h",
        &options("int32_t"),
    )
    .expect_err("unknown host type");
    assert!(error.0.contains("Unknown"), "{}", error.0);
}

#[test]
fn emitted_mirror_type_checks_with_stock_tsc_and_project_prelude() {
    let source = header("void scalar(int32_t id, subscript_rt_completion e); void value(subscript_rt_completion e); void done(subscript_rt_completion e);");
    let selections = BindOptions::new()
        .with_completion("scalar", "int32_t")
        .with_completion("value", "SubValue")
        .with_completion("done", "void");
    let mirror = generate_with_options(&source, "host.h", &selections).expect("mirror");
    let directory =
        std::env::temp_dir().join(format!("subscript-completion-tsc-{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("temporary directory");
    let path = directory.join("mirror.d.ts");
    std::fs::write(&path, mirror).expect("write mirror");
    let prelude = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../prelude/lang.d.ts");
    let output = std::process::Command::new(
        std::env::var_os("TSC")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../node_modules/.bin/tsc")
            }),
    )
    .args([
        "--noEmit", "--strict", "--target", "ES2020", "--lib", "ES2020",
    ])
    .arg(prelude)
    .arg(&path)
    .output()
    .expect("stock tsc");
    std::fs::remove_dir_all(&directory).expect("remove temporary directory");
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn committed_host_completion_mirror_matches_regeneration() {
    let options = BindOptions::new()
        .with_completion("subCompletionI32", "int32_t")
        .with_completion("subCompletionStruct", "SubCompletionValue")
        .with_completion("subCompletionVoid", "void")
        .with_completion("subCompletionImmediate", "int32_t")
        .with_completion("subCompletionSeven", "int32_t");
    let actual = subscript_bindgen::generate_with_options(
        include_str!("../../corpus/interop/host-completion.h"),
        "host-completion.h",
        &options,
    )
    .expect("generate completion mirror");
    assert_eq!(
        actual,
        include_str!("../../corpus/interop/host-completion.generated.d.ts")
    );
}

#[test]
fn completion_struct_fields_require_recursive_scalar_layouts() {
    for (bad, good, field) in [
        ("typedef struct { SubString text; int32_t value; } Result;", "typedef struct { SubValue text; int32_t value; } Result;", "text"),
        ("typedef struct { const int32_t *values; size_t count; int32_t value; } Result;", "typedef struct { int32_t values; size_t count; int32_t value; } Result;", "values"),
        ("typedef struct { int32_t *value; int32_t extra; } Result;", "typedef struct { int32_t value; int32_t extra; } Result;", "value"),
        ("typedef struct { SubString text; int32_t value; } Inner; typedef struct { Inner nested; } Result;", "typedef struct { SubValue text; int32_t value; } Inner; typedef struct { Inner nested; } Result;", "text"),
    ] {
        let make = |decl: &str| format!("{TYPES}\n{decl}\n{FUNCTION}");
        generate_with_options(&make(good), "host.h", &options("Result")).expect("scalar layout control");
        let error = generate_with_options(&make(bad), "host.h", &options("Result")).expect_err("absorbed field");
        assert!(error.0.contains("read") && error.0.contains("struct") && error.0.contains(field), "{}", error.0);
    }
}

#[test]
fn completion_rejects_callbacks_and_wire_aliases_with_scalar_controls() {
    let callback = "typedef void (*Callback)(int32_t value); typedef struct { Callback value; int32_t extra; } Result;";
    let scalar =
        "typedef int32_t Callback; typedef struct { Callback value; int32_t extra; } Result;";
    generate_with_options(
        &format!("{TYPES}\n{scalar}\n{FUNCTION}"),
        "host.h",
        &options("Result"),
    )
    .expect("scalar control");
    let error = generate_with_options(
        &format!("{TYPES}\n{callback}\n{FUNCTION}"),
        "host.h",
        &options("Result"),
    )
    .expect_err("callback field");
    assert!(error.0.contains("read") && error.0.contains("Result") && error.0.contains("value"));
    let scalar = "typedef int32_t Mode; void useMode(Mode mode);";
    generate_with_options(
        &format!("{TYPES}\n{scalar}\n{FUNCTION}"),
        "host.h",
        &options("Mode"),
    )
    .expect("scalar alias control");
    let wire = format!("{TYPES}\n{scalar}\n/* @subscript-cenum Mode ScriptMode */\n{FUNCTION}");
    assert!(generate_with_options(&wire, "host.h", &options("Mode"))
        .expect_err("wire result")
        .0
        .contains("outside §178 rule 6"));
}
