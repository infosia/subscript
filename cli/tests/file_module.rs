//! Contract witnesses for the enabled file module and its absent provider.

use std::ffi::OsString;
use std::path::PathBuf;

fn corpus(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../corpus")
        .join(name)
}

fn invoke(args: Vec<OsString>) -> (u8, String, String) {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = subscript_cli::execute(args, &mut stdout, &mut stderr);
    (
        code,
        String::from_utf8(stdout).expect("UTF-8 output"),
        String::from_utf8(stderr).expect("UTF-8 diagnostics"),
    )
}

#[test]
fn enabled_file_corpus_checks() {
    let (code, _, stderr) = invoke(vec![
        "check".into(),
        corpus("accept/a355-file-module.ts").into_os_string(),
        "--enable-module".into(),
        "node:fs/promises".into(),
    ]);
    assert_eq!(code, 0, "{stderr}");
}

#[test]
fn disabled_module_names_the_build_option() {
    rejects(
        "r399-file-module-disabled.ts",
        false,
        "--enable-module node:fs/promises",
    );
}

fn rejects(name: &str, enabled: bool, required: &str) {
    let mut args = vec![
        "check".into(),
        corpus(&format!("reject/{name}")).into_os_string(),
    ];
    if enabled {
        args.extend(["--enable-module".into(), "node:fs/promises".into()]);
    }
    let (code, _, stderr) = invoke(args);
    assert_eq!(code, 1, "{name}: {stderr}");
    assert!(stderr.contains(required), "{name}: {stderr}");
}

#[test]
fn options_object_names_the_accepted_forms() {
    rejects("r400-file-module-options-object.ts", true, "readFile(path");
}

#[test]
fn another_member_names_the_accepted_forms() {
    rejects("r401-file-module-other-member.ts", true, "readFile(path");
}

#[test]
fn no_file_provider_completes_with_a_caught_error() {
    let path = std::env::temp_dir().join(format!(
        "subscript-file-no-provider-{}.ts",
        std::process::id()
    ));
    std::fs::write(&path, concat!(
        "import { readFile } from 'node:fs/promises';\n",
        "export async function main(): Promise<void> {\n",
        "  try { await readFile('absent.txt', 'utf8'); }\n",
        "  catch (error) { if (error instanceof Error) { print(`${error.name}:${error.message}`); } }\n",
        "}\n",
    )).expect("write no-provider program");
    let (code, stdout, stderr) = invoke(vec![
        "run".into(),
        path.clone().into_os_string(),
        "--enable-module".into(),
        "node:fs/promises".into(),
    ]);
    std::fs::remove_file(path).expect("remove no-provider program");
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.starts_with("Error:"), "{stdout}");
    assert!(stdout.contains("provider"), "{stdout}");
}

const NO_PROVIDER: &str = concat!(
    "import { readFile } from 'node:fs/promises';\n",
    "export async function main(): Promise<void> {\n",
    "  try { await readFile('absent.txt', 'utf8'); }\n",
    "  catch (error) { if (error instanceof Error) { print(`${error.name}:${error.message}`); } }\n",
    "}\n",
);

/// A scratch directory with one entry file, removed on drop.
struct Scratch(PathBuf);
impl Scratch {
    fn new(label: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("subscript-file-{label}-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create scratch directory");
        std::fs::write(root.join("main.ts"), NO_PROVIDER).expect("write entry");
        Self(root)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// `emit` passes `--enable-module` to the checker and the C emitter.
#[test]
fn emit_accepts_the_enable_module_option() {
    let scratch = Scratch::new("emit");
    let output = scratch.0.join("out");
    let emit = |enabled: bool| {
        let mut args: Vec<OsString> = vec![
            "emit".into(),
            scratch.0.join("main.ts").into_os_string(),
            "-o".into(),
            output.clone().into_os_string(),
        ];
        if enabled {
            args.extend(["--enable-module".into(), "node:fs/promises".into()]);
        }
        invoke(args)
    };
    let (code, _, stderr) = emit(true);
    assert_eq!(code, 0, "{stderr}");
    let source = std::fs::read_to_string(output.join("program.c")).expect("emitted C");
    assert!(source.contains("subscript_rt_file_operation"));
    // The firing control: without the option the import is rejected.
    let (code, _, stderr) = emit(false);
    assert_eq!(code, 1, "{stderr}");
    assert!(
        stderr.contains("--enable-module node:fs/promises"),
        "{stderr}"
    );
}

/// `build --run` passes `--enable-module` through; the generated entry
/// installs no provider, so the call completes with the rule 5 Error.
#[test]
fn build_accepts_the_enable_module_option() {
    let scratch = Scratch::new("build");
    let runtime = subscript_codegen::runtime_staticlib_path().expect("runtime library");
    let include = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../runtime/include");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_subscript"))
        .current_dir(&scratch.0)
        .args([
            "build",
            "--source",
            "main.ts",
            "-o",
            "out",
            "--run",
            "--enable-module",
            "node:fs/promises",
        ])
        .arg("--runtime-lib")
        .arg(&runtime)
        .arg("--runtime-include")
        .arg(&include)
        .output()
        .expect("run build");
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"Error:missing file provider\n");
}

#[test]
fn an_unknown_module_name_is_a_usage_error() {
    let (code, _, stderr) = invoke(vec![
        "check".into(),
        corpus("accept/a355-file-module.ts").into_os_string(),
        "--enable-module".into(),
        "node:fs".into(),
    ]);
    assert_eq!(code, 2, "{stderr}");
    assert!(stderr.contains("node:fs/promises"), "{stderr}");
}
