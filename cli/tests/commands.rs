//! End-to-end clean and contracted-error paths for every CLI subcommand.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use subscript_compiler::SourceFile;

struct TestDir(PathBuf);

impl TestDir {
    fn new() -> Result<Self, String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "subscript-cli-commands-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path)
            .map_err(|error| format!("create {}: {error}", path.display()))?;
        Ok(Self(path))
    }

    fn write(&self, relative: &str, bytes: &[u8]) -> Result<PathBuf, String> {
        let path = self.0.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("create {}: {error}", parent.display()))?;
        }
        std::fs::write(&path, bytes)
            .map_err(|error| format!("write {}: {error}", path.display()))?;
        Ok(path)
    }

    fn directory(&self, relative: &str) -> Result<PathBuf, String> {
        let path = self.0.join(relative);
        std::fs::create_dir_all(&path)
            .map_err(|error| format!("create {}: {error}", path.display()))?;
        Ok(path)
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn subscript() -> Command {
    Command::new(env!("CARGO_BIN_EXE_subscript"))
}

fn output(command: &mut Command) -> Result<Output, String> {
    command
        .output()
        .map_err(|error| format!("run subscript: {error}"))
}

fn assert_code(result: &Output, code: i32) {
    assert_eq!(
        result.status.code(),
        Some(code),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}

fn s007_output(path: &Path) -> Vec<u8> {
    format!(
        concat!(
            "error[S007]: bare `number` is rejected; there is no default numeric type — ",
            "use a sized type (i8, u8, i16, u16, i32, u32, i64, u64, f16, f32, f64)\n",
            " --> {}:1:14\n",
            "  |\n",
            "1 | const value: number = 1;\n",
            "  |              ^\n",
            "  = rule: Bare `number` is rejected; sized numeric types are mandatory.\n",
            "error: 1 error(s)\n",
        ),
        path.display()
    )
    .into_bytes()
}

fn w001_source() -> &'static [u8] {
    concat!(
        "class Token {\n",
        "  value: i32;\n",
        "  constructor(value: i32) {\n",
        "    this.value = value;\n",
        "  }\n",
        "}\n",
        "export function main(): void {\n",
        "  for (let i: i32 = 0; i < 2; i += 1) {\n",
        "    const token: Token = new Token(i);\n",
        "    print(`${token.value}`);\n",
        "  }\n",
        "}\n",
    )
    .as_bytes()
}

fn w001_output(path: &Path) -> Vec<u8> {
    format!(
        concat!(
            "warning[W001]: `token` is allocated in each loop iteration but neither escapes the iteration nor is released\n",
            " --> {}:9:26\n",
            "  |\n",
            "9 |     const token: Token = new Token(i);\n",
            "  |                          ^\n",
            "  = rule: A reference-class allocation repeated by a loop should escape the iteration or be released.\n",
            "warning: 1 warning(s)\n",
        ),
        path.display()
    )
    .into_bytes()
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn unresolved_import_output(specifier: &str) -> Vec<u8> {
    format!(
        concat!(
            "error[S100]: imported module `{0}` is not among the program's files\n",
            " --> main.ts:1:24\n",
            "  |\n",
            "1 | import {{ absent }} from \"{0}\";\n",
            "  |                        ^\n",
            "  = rule: Constructs outside the decided language surface are rejected.\n",
            "error: 1 error(s)\n",
        ),
        specifier
    )
    .into_bytes()
}

#[test]
fn a19_check_and_run_match_the_committed_golden() -> Result<(), String> {
    let root = workspace_root();
    let entry = Path::new("corpus/accept/a19-modules/main.ts");

    let checked = output(subscript().current_dir(&root).arg("check").arg(entry))?;
    assert_code(&checked, 0);
    assert!(checked.stdout.is_empty());
    assert_eq!(
        checked.stderr,
        b"check: corpus/accept/a19-modules/main.ts: no errors\n"
    );

    let run = output(subscript().current_dir(&root).arg("run").arg(entry))?;
    assert_code(&run, 0);
    let golden = std::fs::read(root.join("corpus/accept/a19-modules.expected"))
        .map_err(|error| format!("read a19 golden: {error}"))?;
    assert_eq!(run.stdout, golden);
    assert!(run.stderr.is_empty());
    Ok(())
}

#[test]
fn a19_emit_is_byte_identical_to_emit_c_directory_mode() -> Result<(), String> {
    let root = workspace_root();
    let directory = TestDir::new()?;
    let cli_output = directory.0.join("cli");
    let emit_c_output = directory.0.join("emit-c");
    let entry = Path::new("corpus/accept/a19-modules/main.ts");

    let emitted = output(
        subscript()
            .current_dir(&root)
            .arg("emit")
            .arg(entry)
            .arg("-o")
            .arg(&cli_output),
    )?;
    assert_code(&emitted, 0);
    assert!(emitted.stdout.is_empty());
    assert!(emitted.stderr.is_empty());

    let module_directory = root.join("corpus/accept/a19-modules");
    let main_source = std::fs::read_to_string(module_directory.join("main.ts"))
        .map_err(|error| format!("read a19 main.ts: {error}"))?;
    let math_source = std::fs::read_to_string(module_directory.join("math.ts"))
        .map_err(|error| format!("read a19 math.ts: {error}"))?;
    let directory_mode_sources = [
        SourceFile::new("main.ts", main_source),
        SourceFile::new("math.ts", math_source),
    ];
    subscript_codegen::emit_c_files(&directory_mode_sources, &emit_c_output, "a19-modules", true)
        .map_err(|error| format!("emit a19 directory-mode reference: {error}"))?;

    for (cli_name, emit_c_name) in [
        ("program.c", "a19-modules.c"),
        ("program.alloc.h", "a19-modules.alloc.h"),
        ("entry.c", "entry.c"),
    ] {
        let cli_bytes = std::fs::read(cli_output.join(cli_name))
            .map_err(|error| format!("read CLI {cli_name}: {error}"))?;
        let emit_c_bytes = std::fs::read(emit_c_output.join(emit_c_name))
            .map_err(|error| format!("read emit-c {emit_c_name}: {error}"))?;
        assert_eq!(cli_bytes, emit_c_bytes, "{cli_name} differs");
    }
    Ok(())
}

#[test]
fn missing_import_is_a_positioned_checker_diagnostic() -> Result<(), String> {
    let directory = TestDir::new()?;
    directory.write(
        "main.ts",
        concat!(
            "import { absent } from \"./absent\";\n",
            "export function main(): void {}\n",
        )
        .as_bytes(),
    )?;

    let checked = output(
        subscript()
            .current_dir(&directory.0)
            .arg("check")
            .arg("main.ts"),
    )?;
    assert_code(&checked, 1);
    assert!(checked.stdout.is_empty());
    assert_eq!(checked.stderr, unresolved_import_output("./absent"));
    Ok(())
}

#[test]
fn parent_import_is_not_loaded_and_renders_the_checker_diagnostic() -> Result<(), String> {
    let directory = TestDir::new()?;
    directory.write("x.ts", b"import {")?;
    directory.write(
        "program/main.ts",
        concat!(
            "import { absent } from \"../x\";\n",
            "export function main(): void {}\n",
        )
        .as_bytes(),
    )?;

    let checked = output(
        subscript()
            .current_dir(&directory.0)
            .arg("check")
            .arg("program/main.ts"),
    )?;
    assert_code(&checked, 1);
    assert!(checked.stdout.is_empty());
    assert_eq!(checked.stderr, unresolved_import_output("../x"));
    Ok(())
}

#[test]
fn nested_import_is_not_loaded_and_renders_the_checker_diagnostic() -> Result<(), String> {
    let directory = TestDir::new()?;
    directory.write("program/sub/x.ts", b"import {")?;
    directory.write(
        "program/main.ts",
        concat!(
            "import { absent } from \"./sub/x\";\n",
            "export function main(): void {}\n",
        )
        .as_bytes(),
    )?;

    let checked = output(
        subscript()
            .current_dir(&directory.0)
            .arg("check")
            .arg("program/main.ts"),
    )?;
    assert_code(&checked, 1);
    assert!(checked.stdout.is_empty());
    assert_eq!(checked.stderr, unresolved_import_output("./sub/x"));
    Ok(())
}

#[test]
fn two_file_cycle_terminates_and_loads_each_file_once() -> Result<(), String> {
    let directory = TestDir::new()?;
    directory.write(
        "main.ts",
        concat!(
            "import { helper } from \"./other\";\n",
            "export function root(): i32 { return 1; }\n",
            "export function main(): void { print(`${helper()}`); }\n",
        )
        .as_bytes(),
    )?;
    directory.write(
        "other.ts",
        concat!(
            "import { root } from \"./main\";\n",
            "export function helper(): i32 { return root(); }\n",
        )
        .as_bytes(),
    )?;

    let checked = output(
        subscript()
            .current_dir(&directory.0)
            .arg("check")
            .arg("main.ts"),
    )?;
    assert_code(&checked, 0);
    assert!(checked.stdout.is_empty());
    assert_eq!(checked.stderr, b"check: main.ts: no errors\n");
    Ok(())
}

#[test]
fn check_and_emit_cover_clean_diagnostic_and_io_paths() -> Result<(), String> {
    let directory = TestDir::new()?;
    let clean = directory.write(
        "clean.ts",
        b"export function main(): void {\n  print(\"clean\");\n}\n",
    )?;
    let rejected = directory.write("rejected.ts", b"const value: number = 1;\n")?;

    let checked = output(
        subscript()
            .current_dir(&directory.0)
            .arg("check")
            .arg("clean.ts"),
    )?;
    assert_code(&checked, 0);
    assert!(checked.stdout.is_empty());
    assert_eq!(checked.stderr, b"check: clean.ts: no errors\n");

    let diagnostic = output(subscript().arg("check").arg(&rejected))?;
    assert_code(&diagnostic, 1);
    assert!(diagnostic.stdout.is_empty());
    assert_eq!(diagnostic.stderr, s007_output(&rejected));

    let emitted_dir = directory.0.join("emitted");
    let emitted = output(
        subscript()
            .arg("emit")
            .arg(&clean)
            .arg("-o")
            .arg(&emitted_dir)
            .arg("--no-entry"),
    )?;
    assert_code(&emitted, 0);
    assert!(emitted.stdout.is_empty());
    assert!(emitted.stderr.is_empty());
    assert!(emitted_dir.join("program.c").is_file());
    assert!(emitted_dir.join("program.alloc.h").is_file());
    assert!(!emitted_dir.join("entry.c").exists());

    let emit_diagnostic = output(
        subscript()
            .arg("emit")
            .arg(&rejected)
            .arg("-o")
            .arg(directory.0.join("rejected-output")),
    )?;
    assert_code(&emit_diagnostic, 1);
    assert!(emit_diagnostic.stdout.is_empty());
    assert_eq!(emit_diagnostic.stderr, diagnostic.stderr);

    let missing = output(subscript().arg("emit").arg(&clean))?;
    assert_code(&missing, 2);
    Ok(())
}

#[test]
fn link_flags_covers_clean_and_unresolved_archive_paths() -> Result<(), String> {
    let directory = TestDir::new()?;
    let archive = directory.write("runtime/libsubscript_runtime.a", b"archive")?;
    let include = directory.directory("runtime/include")?;
    let linked = output(
        subscript()
            .arg("link-flags")
            .arg("--cc")
            .arg("unix")
            .arg("--runtime-lib")
            .arg(&archive)
            .arg("--runtime-include")
            .arg(&include),
    )?;
    assert_code(&linked, 0);
    assert_eq!(
        linked.stdout,
        format!("-I{}\n{}\n", include.display(), archive.display()).as_bytes()
    );
    assert!(linked.stderr.is_empty());

    let missing = output(
        subscript()
            .arg("link-flags")
            .arg("--runtime-lib")
            .arg(directory.0.join("missing.a"))
            .arg("--runtime-include")
            .arg(&include),
    )?;
    assert_code(&missing, 2);
    assert!(String::from_utf8_lossy(&missing.stderr).contains("--runtime-lib"));
    Ok(())
}

#[test]
fn build_and_run_cover_clean_and_environment_error_paths() -> Result<(), String> {
    let directory = TestDir::new()?;
    let source = directory.write(
        "hello.ts",
        b"export function main(): void {\n  print(\"hello from cli\");\n}\n",
    )?;
    let runtime = subscript_codegen::runtime_staticlib_path().map_err(|error| error.to_string())?;
    let include = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("runtime")
        .join("include");
    let build_dir = directory.0.join("build");

    let built = output(
        subscript()
            .arg("build")
            .arg("--source")
            .arg(&source)
            .arg("-o")
            .arg(&build_dir)
            .arg("--runtime-lib")
            .arg(&runtime)
            .arg("--runtime-include")
            .arg(&include)
            .arg("--run"),
    )?;
    assert_code(&built, 0);
    assert_eq!(built.stdout, b"hello from cli\n");
    assert!(build_dir
        .join(format!("hello{}", std::env::consts::EXE_SUFFIX))
        .is_file());

    let run = output(subscript().arg("run").arg(&source))?;
    assert_code(&run, 0);
    assert_eq!(run.stdout, b"hello from cli\n");
    assert!(run.stderr.is_empty());

    let missing_runtime = output(
        subscript()
            .arg("build")
            .arg("--source")
            .arg(&source)
            .arg("--runtime-lib")
            .arg(directory.0.join("missing.a"))
            .arg("--runtime-include")
            .arg(&include),
    )?;
    assert_code(&missing_runtime, 2);

    let rejected = directory.write("bad.ts", b"const value: number = 1;\n")?;
    let run_rejected = output(subscript().arg("run").arg(&rejected))?;
    assert_code(&run_rejected, 1);
    Ok(())
}

#[test]
fn rejection_text_is_byte_identical_for_all_four_program_commands() -> Result<(), String> {
    let directory = TestDir::new()?;
    directory.write("same.ts", b"const value: number = 1;\n")?;
    let runtime = subscript_codegen::runtime_staticlib_path().map_err(|error| error.to_string())?;
    let include = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("runtime")
        .join("include");

    let source = Path::new("same.ts");
    let checked = output(
        subscript()
            .current_dir(&directory.0)
            .arg("check")
            .arg(source),
    )?;
    let emitted = output(
        subscript()
            .current_dir(&directory.0)
            .arg("emit")
            .arg(source)
            .arg("-o")
            .arg("emit-rejected"),
    )?;
    let built = output(
        subscript()
            .current_dir(&directory.0)
            .arg("build")
            .arg("--source")
            .arg(source)
            .arg("-o")
            .arg("build-rejected")
            .arg("--runtime-lib")
            .arg(&runtime)
            .arg("--runtime-include")
            .arg(&include),
    )?;
    let run = output(subscript().current_dir(&directory.0).arg("run").arg(source))?;

    for result in [&checked, &emitted, &built, &run] {
        assert_code(result, 1);
        assert!(result.stdout.is_empty());
    }
    assert_eq!(checked.stderr, s007_output(source));
    assert_eq!(emitted.stderr, checked.stderr);
    assert_eq!(built.stderr, checked.stderr);
    assert_eq!(run.stderr, checked.stderr);
    Ok(())
}

#[test]
fn warning_text_is_exact_and_byte_identical_with_artifacts_for_all_commands() -> Result<(), String>
{
    let directory = TestDir::new()?;
    directory.write("warning.ts", w001_source())?;
    let runtime = subscript_codegen::runtime_staticlib_path().map_err(|error| error.to_string())?;
    let include = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("runtime")
        .join("include");
    let source = Path::new("warning.ts");

    let checked = output(
        subscript()
            .current_dir(&directory.0)
            .arg("check")
            .arg(source),
    )?;
    let emitted = output(
        subscript()
            .current_dir(&directory.0)
            .arg("emit")
            .arg(source)
            .arg("-o")
            .arg("warn-emitted"),
    )?;
    let built = output(
        subscript()
            .current_dir(&directory.0)
            .arg("build")
            .arg("--source")
            .arg(source)
            .arg("-o")
            .arg("warn-built")
            .arg("--runtime-lib")
            .arg(&runtime)
            .arg("--runtime-include")
            .arg(&include),
    )?;
    let run = output(subscript().current_dir(&directory.0).arg("run").arg(source))?;

    for result in [&checked, &emitted, &built, &run] {
        assert_code(result, 0);
    }
    assert!(checked.stdout.is_empty());
    assert!(emitted.stdout.is_empty());
    assert!(built.stdout.is_empty());
    assert_eq!(run.stdout, b"0\n1\n");
    assert_eq!(checked.stderr, w001_output(source));
    assert_eq!(emitted.stderr, checked.stderr);
    assert_eq!(built.stderr, checked.stderr);
    assert_eq!(run.stderr, checked.stderr);
    assert!(directory.0.join("warn-emitted/program.c").is_file());
    assert!(directory
        .0
        .join(format!(
            "warn-built/warning{}",
            std::env::consts::EXE_SUFFIX
        ))
        .is_file());
    Ok(())
}

#[test]
fn deny_warnings_exits_one_and_prevents_emit_and_build_artifacts() -> Result<(), String> {
    let directory = TestDir::new()?;
    directory.write("warning.ts", w001_source())?;
    let source = Path::new("warning.ts");

    let checked = output(
        subscript()
            .current_dir(&directory.0)
            .arg("check")
            .arg(source)
            .arg("--deny-warnings"),
    )?;
    let emitted = output(
        subscript()
            .current_dir(&directory.0)
            .arg("emit")
            .arg(source)
            .arg("-o")
            .arg("denied-emit")
            .arg("--deny-warnings"),
    )?;
    let built = output(
        subscript()
            .current_dir(&directory.0)
            .arg("build")
            .arg("--source")
            .arg(source)
            .arg("-o")
            .arg("denied-build")
            .arg("--deny-warnings"),
    )?;
    let run = output(
        subscript()
            .current_dir(&directory.0)
            .arg("run")
            .arg(source)
            .arg("--deny-warnings"),
    )?;

    for result in [&checked, &emitted, &built, &run] {
        assert_code(result, 1);
        assert!(result.stdout.is_empty());
        assert_eq!(result.stderr, w001_output(source));
    }
    assert!(!directory.0.join("denied-emit").exists());
    assert!(!directory.0.join("denied-build").exists());
    Ok(())
}

#[test]
fn unknown_subcommand_is_a_usage_error() -> Result<(), String> {
    let result = output(subscript().arg("unknown"))?;
    assert_code(&result, 2);
    assert!(result.stdout.is_empty());
    assert!(String::from_utf8_lossy(&result.stderr).contains("unknown subcommand"));
    Ok(())
}
