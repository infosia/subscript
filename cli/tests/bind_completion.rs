//! Completion CLI gates (§178). The output gate uses two CLI processes.
//! Each rejection case uses one CLI process. No gate starts tsc or a C compiler.

use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

struct TestDir(PathBuf);

impl TestDir {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "subscript-cli-completion-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).expect("temporary directory");
        std::fs::write(
            path.join("host.h"),
            "void read(int id, subscript_rt_completion e); void done(subscript_rt_completion e);",
        )
        .expect("header");
        Self(path)
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_subscript"));
        command.arg("bind").arg(self.0.join("host.h"));
        command
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn check(output: &Output, code: i32) {
    assert_eq!(
        output.status.code(),
        Some(code),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn repeated_options_reach_stdout_and_file() {
    let directory = TestDir::new();
    let output = directory
        .command()
        .args(["--completion", "read=int32_t", "--completion", "done=void"])
        .output()
        .expect("bind stdout");
    check(&output, 0);
    let mirror = String::from_utf8(output.stdout.clone()).expect("UTF-8 mirror");
    assert!(mirror.contains("declare function read(id: i32): Promise<i32>;"));
    assert!(mirror.contains("declare function done(): Promise<void>;"));
    assert!(mirror.contains("// @subscript-c-completion function=\"read\" result=\"int32_t\""));
    let path = directory.0.join("mirror.d.ts");
    let file = directory
        .command()
        .args([
            "--completion",
            "done=void",
            "--completion",
            "read=int32_t",
            "-o",
        ])
        .arg(&path)
        .output()
        .expect("bind file");
    check(&file, 0);
    assert!(file.stdout.is_empty());
    assert_eq!(std::fs::read(path).expect("mirror file"), output.stdout);
}

#[test]
fn invalid_selections_exit_one_without_output() {
    let directory = TestDir::new();
    let path = directory.0.join("mirror.d.ts");
    for selections in [
        vec!["read=SubAbsent", "done=void"],
        vec!["read=int32_t", "read=void", "done=void"],
        vec!["done=void"],
    ] {
        let mut command = directory.command();
        for selection in selections {
            command.args(["--completion", selection]);
        }
        let output = command
            .arg("-o")
            .arg(&path)
            .output()
            .expect("bind rejection");
        check(&output, 1);
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("read"));
        assert!(!path.exists());
    }
}

#[test]
fn missing_or_malformed_value_exits_two() {
    let directory = TestDir::new();
    for arguments in [
        vec!["--completion"],
        vec!["--completion", "read"],
        vec!["--completion", "=void"],
        vec!["--completion", "read="],
    ] {
        let output = directory
            .command()
            .args(arguments)
            .output()
            .expect("bind usage");
        check(&output, 2);
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("--completion requires"));
    }
}

#[test]
fn binder_results_and_checker_results_agree() {
    use subscript_bindgen::{generate_with_options, BindOptions};
    use subscript_compiler::{check_program, RuleCode, SourceFile};
    // Each result uses one libclang parse and one in-memory checker invocation.
    let types = r#"
#include <stdint.h>
#include <stdbool.h>
typedef uint32_t Count;
typedef enum { Idle = 0, Active = 1 } Mode;
typedef struct { int32_t x; double y; } Pair;
typedef struct { uint8_t x; double y; } Padded;
typedef struct { Padded inner; Pair pair; } Nested;
typedef struct { uint8_t a; int32_t b; uint8_t c; } ByteFields;
typedef struct { ByteFields inner; } NestedBytes;
"#;
    for result in [
        "int8_t",
        "uint8_t",
        "int16_t",
        "uint16_t",
        "int32_t",
        "uint32_t",
        "int64_t",
        "uint64_t",
        "bool",
        "_Float16",
        "float",
        "double",
        "Mode",
        "Count",
        "Pair",
        "Padded",
        "Nested",
        "ByteFields",
        "NestedBytes",
        "void",
    ] {
        let header = format!("{types}\nvoid read(subscript_rt_completion endpoint);");
        let mirror = generate_with_options(
            &header,
            "host.h",
            &BindOptions::new().with_completion("read", result),
        )
        .expect(result);
        check_program(&[
            SourceFile::ambient("host.d.ts", mirror.clone()),
            SourceFile::new(
                "main.ts",
                "export async function main(): Promise<void> { await read(); }",
            ),
        ])
        .unwrap_or_else(|errors| panic!("{result}: {errors:?}\n{mirror}"));
    }
    for (result, declarations, mirror_declarations, script_type) in [
        ("Handle", "typedef struct Handle_T *Handle;", "declare class Handle { private __opaque: never; }", "Handle"),
        ("Pointer", "typedef int32_t *Pointer;", "declare class Pointer { private __opaque: never; }", "Pointer | null"),
        ("const char *", "", "", "string"),
        ("View", "typedef struct { const char *data; size_t length; } View;", "declare class View { data: string; }", "View"),
        ("Array", "typedef struct { const int32_t *data; size_t count; } Array;", "declare class Array { data: i32[]; }", "Array"),
        ("Callback", "typedef void (*Callback)(int32_t);", "// @subscript-c-callback typedef=\"Callback\"\ntype Callback = (x: i32) => void;", "Callback"),
        ("Wire", "typedef int32_t Wire; /* @subscript-cenum Wire ScriptWire */", "// @subscript-c-cenum typedef=\"Wire\" alias=\"ScriptWire\"\ntype ScriptWire = CEnum<{ idle: 0; active: 1 }>;", "ScriptWire"),
        ("PointerField", "typedef struct { int32_t *value; int32_t extra; } PointerField;", "declare class PointerField { value: object | null; extra: i32; }", "PointerField"),
        ("CallbackField", "typedef void (*Callback)(int32_t); typedef struct { Callback value; int32_t extra; } CallbackField;", "// @subscript-c-callback typedef=\"Callback\"\ntype Callback = (x: i32) => void; declare class CallbackField { value: Callback; extra: i32; }", "CallbackField"),
        ("WireField", "typedef int32_t Wire; /* @subscript-cenum Wire ScriptWire */ typedef struct { Wire value; } WireField;", "// @subscript-c-cenum typedef=\"Wire\" alias=\"ScriptWire\"\ntype ScriptWire = CEnum<{ idle: 0; active: 1 }>; declare class WireField { value: ScriptWire; }", "WireField"),
        ("NestedView", "typedef struct { const char *data; size_t length; } View; typedef struct { View inner; } NestedView;", "declare class View { data: string; } declare class NestedView { inner: View; }", "NestedView"),
        ("BoolPair", "typedef struct { bool a; bool b; } BoolPair;", "declare class BoolPair { a: boolean; b: boolean; }", "BoolPair"),
        ("BoolPadded", "typedef struct { bool a; int32_t b; bool c; } BoolPadded;", "declare class BoolPadded { a: boolean; b: i32; c: boolean; }", "BoolPadded"),
        ("AliasBool", "typedef bool Flag; typedef struct { Flag leaf; } AliasBool;", "type Flag = boolean; declare class AliasBool { leaf: Flag; }", "AliasBool"),
        ("NestedBool", "typedef struct { bool leaf; } BoolLeaf; typedef struct { BoolLeaf inner; } NestedBool;", "declare class BoolLeaf { leaf: boolean; } declare class NestedBool { inner: BoolLeaf; }", "NestedBool"),
        ("Absent", "", "type Absent = string;", "Absent"),
        ("subscript_rt_completion", "", "declare class subscript_rt_completion { context_id: u64; operation_id: u64; }", "subscript_rt_completion"),
    ] {
        let header = format!("#include <stddef.h>\n{types}\n{declarations}\nvoid read(subscript_rt_completion endpoint);");
        let binder_error = generate_with_options(&header, "host.h", &BindOptions::new().with_completion("read", result)).expect_err(result);
        let mirror = format!("// @subscript-c-header include=\"host.h\"\n// @subscript-c-completion function=\"read\" result=\"{result}\"\n{mirror_declarations}\ndeclare function read(): Promise<{script_type}>;");
        let errors = check_program(&[
            SourceFile::ambient("host.d.ts", mirror),
            SourceFile::new("main.ts", "export async function main(): Promise<void> { await read(); }"),
        ]).expect_err(result);
        assert!(errors.iter().any(|error| error.code == RuleCode::S100), "{result}: {errors:?}");
        if let Some((class, field)) = match result {
            "BoolPair" => Some(("BoolPair", "a")),
            "BoolPadded" => Some(("BoolPadded", "a")),
            "NestedBool" => Some(("BoolLeaf", "leaf")),
            "AliasBool" => Some(("AliasBool", "leaf")),
            _ => None,
        } {
            for fragment in ["function `read`".to_string(), format!("struct `{class}`"), format!("field `{field}`")] {
                assert!(binder_error.0.contains(&fragment), "{binder_error:?}");
                assert!(errors.iter().any(|error| error.message.contains(&fragment)), "{errors:?}");
            }
        }
    }
}
