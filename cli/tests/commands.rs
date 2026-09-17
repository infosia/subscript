//! End-to-end clean and contracted-error paths for every CLI subcommand.

use std::ffi::OsString;
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
            "  = TypeScript accepts:\n",
            "  |   const count: number = 3;\n",
            "  = subscript:\n",
            "  |   const count: i32 = 3;\n",
            "  = why: `number` is a 64-bit float with no C width, so every declaration names ",
            "one of the sized types. (collisions.md C3)\n",
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

#[test]
fn run_preserves_stdout_before_a_trap() {
    let source = workspace_root().join("corpus/trap/t03-loop-stops-at-fault.ts");
    let expected = std::fs::read(source.with_extension("expected")).unwrap();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = subscript_cli::execute(
        [std::ffi::OsString::from("run"), source.into_os_string()],
        &mut stdout,
        &mut stderr,
    );
    assert_eq!(stdout, expected);
    assert_eq!(code, 1);
    assert!(String::from_utf8_lossy(&stderr).contains("trap"));
}

#[test]
fn run_trap_without_output_keeps_stdout_empty() {
    let source = workspace_root().join("corpus/trap/t01-json-result-value.ts");
    let expected = std::fs::read(source.with_extension("expected")).unwrap();
    assert!(expected.is_empty());
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = subscript_cli::execute(
        [std::ffi::OsString::from("run"), source.into_os_string()],
        &mut stdout,
        &mut stderr,
    );
    assert_eq!(stdout, expected);
    assert_eq!(code, 1);
    assert!(String::from_utf8_lossy(&stderr).contains("json-result-value"));
}

fn directory_sources(directory: &Path) -> Result<Vec<SourceFile>, String> {
    let entries = std::fs::read_dir(directory)
        .map_err(|error| format!("read {}: {error}", directory.display()))?;
    let mut names = Vec::new();
    for entry in entries {
        let entry =
            entry.map_err(|error| format!("read {} entry: {error}", directory.display()))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with(".ts") {
            names.push(name);
        }
    }
    names.sort();
    names.sort_by_key(|name| !name.contains("main"));

    let mut sources = Vec::with_capacity(names.len());
    for name in names {
        let text = std::fs::read_to_string(directory.join(&name))
            .map_err(|error| format!("read {name}: {error}"))?;
        sources.push(SourceFile::new(name, text));
    }
    Ok(sources)
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
fn bind_stdout_and_output_file_match_the_committed_mirror() -> Result<(), String> {
    let root = workspace_root();
    let header = Path::new("examples/engine/engine.h");
    let committed = std::fs::read(root.join("examples/engine/engine.generated.d.ts"))
        .map_err(|error| format!("read committed engine mirror: {error}"))?;

    let cli_stdout = output(
        subscript()
            .current_dir(&root)
            .arg("bind")
            .arg("--header")
            .arg(header),
    )?;
    assert_code(&cli_stdout, 0);
    assert_eq!(cli_stdout.stdout, committed);
    assert!(cli_stdout.stderr.is_empty());

    let directory = TestDir::new()?;
    let cli_path = directory.0.join("cli.d.ts");
    let cli_file = output(
        subscript()
            .current_dir(&root)
            .arg("bind")
            .arg(header)
            .arg("-o")
            .arg(&cli_path),
    )?;
    assert_code(&cli_file, 0);
    assert!(cli_file.stdout.is_empty());
    assert!(cli_file.stderr.is_empty());

    let cli_bytes = std::fs::read(&cli_path)
        .map_err(|error| format!("read {}: {error}", cli_path.display()))?;
    assert_eq!(cli_bytes, committed);
    Ok(())
}

#[test]
fn bind_unmappable_construct_is_a_program_error_without_an_output_file() -> Result<(), String> {
    let directory = TestDir::new()?;
    let header = directory.write("bad.h", b"typedef struct S { long n; } S;\nvoid f(S s);\n")?;
    let mirror = directory.0.join("bad.d.ts");

    let result = output(
        subscript()
            .arg("bind")
            .arg("--header")
            .arg(&header)
            .arg("-o")
            .arg(&mirror),
    )?;
    assert_code(&result, 1);
    assert!(result.stdout.is_empty());
    assert_eq!(
        String::from_utf8_lossy(&result.stderr),
        concat!(
            "subscript: bindgen: unmapped C type `long` at a boundary use site: ",
            "it is neither a mapped scalar/builtin nor a named type declared by ",
            "this header. If another ambient mirror declares it, add ",
            "`/* @subscript-external long */` to this header; refusing to emit ",
            "an unresolved name otherwise.\n",
        )
    );
    assert!(
        !mirror.exists(),
        "a rejected header left {}",
        mirror.display()
    );
    Ok(())
}

#[test]
fn bind_usage_and_io_failures_exit_two() -> Result<(), String> {
    let directory = TestDir::new()?;
    let missing_argument = output(subscript().arg("bind"))?;
    assert_code(&missing_argument, 2);
    assert!(missing_argument.stdout.is_empty());
    assert!(String::from_utf8_lossy(&missing_argument.stderr)
        .contains("bind requires --header <file.h> or <file.h>"));

    let missing_header = directory.0.join("missing.h");
    let mirror = directory.0.join("missing.d.ts");
    let io_failure = output(
        subscript()
            .arg("bind")
            .arg("--header")
            .arg(&missing_header)
            .arg("-o")
            .arg(&mirror),
    )?;
    assert_code(&io_failure, 2);
    assert!(io_failure.stdout.is_empty());
    assert!(String::from_utf8_lossy(&io_failure.stderr).contains("read header"));
    assert!(!mirror.exists());
    Ok(())
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
fn a19_emit_is_byte_identical_to_the_shared_directory_entry() -> Result<(), String> {
    let root = workspace_root();
    let directory = TestDir::new()?;
    let cli_output = directory.0.join("cli");
    let shared_output = directory.0.join("shared");
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
    let directory_mode_sources = directory_sources(&module_directory)?;
    subscript_codegen::emit_c_files(
        &directory_mode_sources,
        &shared_output,
        "a19-modules",
        true,
        subscript_compiler::Profile::Default,
    )
    .map_err(|error| format!("emit a19 directory-mode reference: {error}"))?;

    for (cli_name, shared_name) in [
        ("program.c", "a19-modules.c"),
        ("program.alloc.h", "a19-modules.alloc.h"),
        ("entry.c", "entry.c"),
    ] {
        let cli_bytes = std::fs::read(cli_output.join(cli_name))
            .map_err(|error| format!("read CLI {cli_name}: {error}"))?;
        let shared_bytes = std::fs::read(shared_output.join(shared_name))
            .map_err(|error| format!("read shared {shared_name}: {error}"))?;
        assert_eq!(cli_bytes, shared_bytes, "{cli_name} differs");
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
    let mut expected_stdout =
        format!("-I{}\n{}\n", include.display(), archive.display()).into_bytes();
    for library in
        subscript_codegen::runtime_system_libraries(subscript_codegen::CCompilerStyle::Unix)
    {
        expected_stdout.extend_from_slice(format!("{}\n", library.to_string_lossy()).as_bytes());
    }
    assert_eq!(linked.stdout, expected_stdout);
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

#[test]
fn run_trap_keeps_exit_one_when_stdout_fails() {
    struct FailingOutput(bool);
    impl std::io::Write for FailingOutput {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if self.0 {
                Err(std::io::ErrorKind::BrokenPipe.into())
            } else {
                Ok(bytes.len())
            }
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Err(std::io::ErrorKind::BrokenPipe.into())
        }
    }
    for fail_write in [true, false] {
        let source = workspace_root().join("corpus/trap/t03-loop-stops-at-fault.ts");
        let mut stderr = Vec::new();
        let code = subscript_cli::execute(
            [std::ffi::OsString::from("run"), source.into_os_string()],
            &mut FailingOutput(fail_write),
            &mut stderr,
        );
        assert_eq!(code, 1, "fail_write={fail_write}");
        assert!(String::from_utf8_lossy(&stderr).contains("index-out-of-bounds"));
    }
}

/// §109.1 rule 1: `check`, `build`, and `run` accept `--profile sandbox`.
/// The default profile has no name, so every other value is a usage error.
#[test]
fn profile_selects_the_sandbox_rules_and_rejects_every_other_name() -> Result<(), String> {
    let dir = TestDir::new()?;
    let source = dir.write(
        "free.ts",
        concat!(
            "class Counter {\n",
            "  value: i32 = 0;\n",
            "}\n",
            "export function main(): void {\n",
            "  const counter: Counter = new Counter();\n",
            "  Context.free(counter);\n",
            "}\n",
        )
        .as_bytes(),
    )?;

    // The firing control: the default profile accepts the same source.
    let clean = output(subscript().arg("check").arg(&source))?;
    assert_code(&clean, 0);

    let sandbox = output(
        subscript()
            .arg("check")
            .arg("--profile")
            .arg("sandbox")
            .arg(&source),
    )?;
    assert_code(&sandbox, 1);
    let rendered = String::from_utf8_lossy(&sandbox.stderr).into_owned();
    assert!(
        rendered.contains("error[S023]") && rendered.contains("sandbox profile"),
        "{rendered}"
    );

    // `run` reads the same flag and stops at the same rejection.
    let run = output(
        subscript()
            .arg("run")
            .arg("--profile")
            .arg("sandbox")
            .arg(&source),
    )?;
    assert_code(&run, 1);
    assert!(
        String::from_utf8_lossy(&run.stderr).contains("error[S023]"),
        "stderr:\n{}",
        String::from_utf8_lossy(&run.stderr)
    );

    // `build` reads it before it emits.
    let build_output = dir.directory("build")?;
    let build = output(
        subscript()
            .arg("build")
            .arg("--source")
            .arg(&source)
            .arg("-o")
            .arg(&build_output)
            .arg("--profile")
            .arg("sandbox"),
    )?;
    assert_code(&build, 1);

    for command in ["check", "run"] {
        let unknown = output(
            subscript()
                .arg(command)
                .arg("--profile")
                .arg("strict")
                .arg(&source),
        )?;
        assert_code(&unknown, 2);
        assert!(
            String::from_utf8_lossy(&unknown.stderr).contains("unknown profile `strict`"),
            "{command}: stderr:\n{}",
            String::from_utf8_lossy(&unknown.stderr)
        );
        let missing = output(subscript().arg(command).arg("--profile"))?;
        assert_code(&missing, 2);
    }

    let twice = output(
        subscript()
            .arg("check")
            .arg("--profile")
            .arg("sandbox")
            .arg("--profile")
            .arg("sandbox")
            .arg(&source),
    )?;
    assert_code(&twice, 2);
    Ok(())
}

/// §109.2 rule 5: the parser's entry owns the S026 scan, so no command
/// parses a source the scan rejects.
///
/// `deep.ts` nests 300,000 type arguments in 900,012 bytes, over the
/// per-file byte limit of 131,072. Every command reports S026 and no
/// command parses it: the parser recurses once for each level, and the
/// compile thread's stack does not hold 300,000 of them.
///
/// `octal.ts` puts a legacy octal literal, which the lexer refuses, in a
/// file padded past the byte limit. Under the profile S026 reports
/// alone, so the byte check ran before the lexer.
#[test]
fn s026_rejects_a_source_before_any_command_parses_it() -> Result<(), String> {
    let dir = TestDir::new()?;
    let levels = 300_000;
    let deep = dir.write(
        "deep.ts",
        format!("let x: {}i32{};\n", "A<".repeat(levels), ">".repeat(levels)).as_bytes(),
    )?;
    let octal = dir.write(
        "octal.ts",
        format!("let o: i32 = 010;\n// {}\n", "x".repeat(131_072)).as_bytes(),
    )?;

    for source in [&deep, &octal] {
        for command in ["check", "run"] {
            let rejected = output(
                subscript()
                    .arg(command)
                    .arg("--profile")
                    .arg("sandbox")
                    .arg(source),
            )?;
            assert_code(&rejected, 1);
            let rendered = String::from_utf8_lossy(&rejected.stderr).into_owned();
            assert!(
                rendered.contains("error[S026]") && rendered.contains("sandbox profile"),
                "{command} {}: {rendered}",
                source.display()
            );
        }
        let built = dir.directory("build")?;
        let rejected = output(
            subscript()
                .arg("build")
                .arg("--source")
                .arg(source)
                .arg("-o")
                .arg(&built)
                .arg("--profile")
                .arg("sandbox"),
        )?;
        assert_code(&rejected, 1);
        assert!(
            String::from_utf8_lossy(&rejected.stderr).contains("error[S026]"),
            "build {}: {}",
            source.display(),
            String::from_utf8_lossy(&rejected.stderr)
        );
    }

    // The firing control: the default profile runs no §109.2 rule, so
    // the octal source keeps the parse error it always had. The deep
    // source has no control here, because the default profile keeps no
    // limit and the parser recurses once for each of its 300,000 levels.
    let default = output(subscript().arg("check").arg(&octal))?;
    assert_code(&default, 1);
    let rendered = String::from_utf8_lossy(&default.stderr).into_owned();
    assert!(
        rendered.contains("error[S100]") && rendered.contains("Legacy octal"),
        "{rendered}"
    );
    assert!(!rendered.contains("S026"), "{rendered}");
    // The byte check runs before the lexer, so the profile reports S026
    // alone and the octal error of the same file does not join it.
    let profiled = output(
        subscript()
            .arg("check")
            .arg("--profile")
            .arg("sandbox")
            .arg(&octal),
    )?;
    let rendered = String::from_utf8_lossy(&profiled.stderr).into_owned();
    assert!(
        rendered.contains("error[S026]") && !rendered.contains("S100"),
        "{rendered}"
    );
    Ok(())
}

/// §109.5: `build --profile sandbox` writes the run-time defaults into
/// the generated entry, and `run --profile sandbox` applies them.
///
/// The firing control is the same source without the flag: the entry
/// carries neither call, and the run completes.
#[test]
fn the_sandbox_profile_carries_its_run_time_defaults_through_build_and_run() -> Result<(), String> {
    let dir = TestDir::new()?;
    // The program allocates past the 64 MiB quota, so the profile stops
    // it and the default profile runs it to the end.
    let source = dir.write(
        "quota.ts",
        concat!(
            "export function main(): void {\n",
            "  print(\"start\");\n",
            "  let seed: u8[] = [1];\n",
            "  for (let step: i32 = 0; step < 18; step = step + 1) {\n",
            "    seed = seed.concat(seed);\n",
            "  }\n",
            "  const blocks: u8[][] = [];\n",
            "  for (let block: i32 = 0; block < 300; block = block + 1) {\n",
            "    blocks.push(seed.slice(0, seed.length));\n",
            "  }\n",
            "  print(`${blocks.length}`);\n",
            "}\n",
        )
        .as_bytes(),
    )?;

    for (profile, present) in [(None, false), (Some("sandbox"), true)] {
        let out = dir.directory(if present { "sandbox" } else { "default" })?;
        let mut command = subscript();
        command
            .arg("build")
            .arg("--source")
            .arg(&source)
            .arg("-o")
            .arg(&out);
        if let Some(name) = profile {
            command.arg("--profile").arg(name);
        }
        let built = output(&mut command)?;
        assert_code(&built, 0);
        let entry = std::fs::read_to_string(out.join("entry.c"))
            .map_err(|error| format!("read entry.c: {error}"))?;
        for call in [
            "subscript_rt_ctx_set_alloc_quota(ctx, UINT64_C(67108864));",
            "subscript_rt_ctx_set_stack_budget(ctx, UINT64_C(524288));",
        ] {
            assert_eq!(
                entry.contains(call),
                present,
                "profile {profile:?}: `{call}` presence in the generated entry"
            );
        }
    }

    let completed = output(subscript().arg("run").arg(&source))?;
    assert_code(&completed, 0);
    assert_eq!(String::from_utf8_lossy(&completed.stdout), "start\n300\n");

    let stopped = output(
        subscript()
            .arg("run")
            .arg("--profile")
            .arg("sandbox")
            .arg(&source),
    )?;
    assert_code(&stopped, 1);
    assert_eq!(String::from_utf8_lossy(&stopped.stdout), "start\n");
    assert!(
        String::from_utf8_lossy(&stopped.stderr).contains("allocation-quota"),
        "stderr:\n{}",
        String::from_utf8_lossy(&stopped.stderr)
    );
    Ok(())
}

/// A 65,536-byte line printed 4,096 times.
///
/// The program prints 256 MiB, which is four times the profile's default
/// quota of 64 MiB.
fn sink_source() -> &'static str {
    concat!(
        "export function main(): void {\n",
        "  const line: string = \"x\".repeat(65536);\n",
        "  for (let step: i32 = 0; step < 4096; step = step + 1) {\n",
        "    print(line);\n",
        "  }\n",
        "}\n",
    )
}

/// §109.7a: `run --profile sandbox` installs no print observer, so the
/// quota charges every printed line and a program that prints past the
/// quota traps with the bytes before the trap intact.
///
/// The control is the same program under the default profile, which has
/// no quota and prints all 4,096 lines.
#[test]
fn the_print_sink_charges_the_quota_under_the_profile() -> Result<(), String> {
    /// The bytes of one line, without its newline.
    const LINE: usize = 65_536;
    /// The lines the program prints.
    const LINES: usize = 4_096;
    /// §109.5: the profile's default allocation quota.
    const QUOTA: usize = 67_108_864;

    let dir = TestDir::new()?;
    let source = dir.write("sink.ts", sink_source().as_bytes())?;
    // The output is measured, not compared against a golden, so it goes
    // to a file: 256 MiB in a pipe buffer is the harness's memory, not
    // the program's.
    let captured = dir.0.join("sink.out");

    let stopped = output(
        subscript()
            .arg("run")
            .arg("--profile")
            .arg("sandbox")
            .arg(&source)
            .stdout(std::process::Stdio::from(
                std::fs::File::create(&captured)
                    .map_err(|error| format!("create {}: {error}", captured.display()))?,
            )),
    )?;
    assert_code(&stopped, 1);
    assert!(
        String::from_utf8_lossy(&stopped.stderr).contains("allocation-quota"),
        "stderr:\n{}",
        String::from_utf8_lossy(&stopped.stderr)
    );
    let bytes = std::fs::read(&captured).map_err(|error| format!("read the output: {error}"))?;
    // The bytes before the trap are whole lines, and the quota stopped
    // the program before the last one.
    assert_eq!(bytes.len() % (LINE + 1), 0, "{} bytes", bytes.len());
    let lines = bytes.len() / (LINE + 1);
    assert!(lines > 0 && lines < LINES, "{lines} lines before the trap");
    assert!(
        bytes.len() < QUOTA,
        "{} bytes against the quota",
        bytes.len()
    );
    let one = [b"x".repeat(LINE), b"\n".to_vec()].concat();
    assert!(
        bytes.chunks_exact(LINE + 1).all(|chunk| chunk == one),
        "a line before the trap is not intact"
    );
    println!(
        "sink.ts under the profile: {lines} lines, {} bytes",
        bytes.len()
    );

    // The control: no quota, so every line reaches the caller.
    let completed = output(
        subscript()
            .arg("run")
            .arg(&source)
            .stdout(std::process::Stdio::from(
                std::fs::File::create(&captured)
                    .map_err(|error| format!("create {}: {error}", captured.display()))?,
            )),
    )?;
    assert_code(&completed, 0);
    let all = std::fs::metadata(&captured)
        .map_err(|error| format!("read the output: {error}"))?
        .len();
    assert_eq!(all, (LINES * (LINE + 1)) as u64);
    Ok(())
}

/// A chain of `labels` same labels, inside S026's byte limit.
///
/// The SWC parser's duplicate-label path is superlinear in memory on
/// this shape. The parser is external, so §109.2 rule 6 bounds it with
/// a process.
fn same_label_chain(labels: usize) -> String {
    format!(
        "export function main(): void {{}}\n{};\n//a\n",
        "a:".repeat(labels)
    )
}

/// A generic call whose type argument nests `levels` deep over a leaf
/// that is an expression and not a type.
///
/// The parser reads the whole nest before the leaf refuses it, so its
/// work is superlinear in the level count.
fn nested_generic_call(levels: usize) -> String {
    format!(
        "export function main(): void {{\n  const x: i32 = f<{}1+1{}>(1);\n  print(`${{x}}`);\n}}\n",
        "A<".repeat(levels),
        ">".repeat(levels)
    )
}

/// The variable that selects the heavy CLI tests (§109.6a).
const HEAVY_TESTS_VARIABLE: &str = "SUBSCRIPT_HEAVY_TESTS";

/// One poll's growth past the memory budget (§109.2 rule 6).
///
/// The parent reads the child's resident bytes every 10 ms, so the peak
/// the kernel measures is the budget plus what the child took in one
/// interval.
#[cfg(target_os = "macos")]
const POLL_GROWTH_BYTES: u64 = 268_435_456;

/// The memory budget of this build (§109.2 rule 6).
///
/// The test binary and the CLI binary carry the same build, so the
/// number here is the number the parent holds the child to.
#[cfg(target_os = "macos")]
const MEMORY_BUDGET_BYTES: u64 = if cfg!(debug_assertions) {
    12_884_901_888
} else {
    4_294_967_296
};

/// The `gate-skip:` line a heavy part prints when `selected` is false
/// (§109.6a). `part` names the test and what it omits.
fn heavy_skip_line(part: &str, selected: bool) -> Option<String> {
    (!selected).then(|| format!("gate-skip: {part}; set {HEAVY_TESTS_VARIABLE}=1 to run it"))
}

/// True when the caller must omit its heavy part, and prints the skip
/// line first.
fn skipped_as_heavy(part: &str) -> bool {
    let Some(line) = heavy_skip_line(part, std::env::var_os(HEAVY_TESTS_VARIABLE).is_some()) else {
        return false;
    };
    use std::io::Write;
    // Start a new line after any test harness prefix, outside its capture.
    writeln!(std::io::stdout().lock(), "\n{line}").expect("write the gate skip line");
    true
}

/// §109.6a: the skip line a heavy part prints, and the variable that
/// removes it.
#[test]
fn the_heavy_skip_line_is_declared() {
    assert_eq!(
        heavy_skip_line("a_heavy_test reaches a budget", false).as_deref(),
        Some("gate-skip: a_heavy_test reaches a budget; set SUBSCRIPT_HEAVY_TESTS=1 to run it")
    );
    assert_eq!(heavy_skip_line("a_heavy_test reaches a budget", true), None);
}

/// The largest resident set size of any child this process waited for.
///
/// macOS reports `ru_maxrss` in bytes. The kernel measures this figure,
/// and the parent's poll reads `proc_pid_rusage`, so the two facts are
/// derived apart (§109.2 rule 6).
#[cfg(target_os = "macos")]
fn largest_child_peak_bytes() -> u64 {
    // SAFETY: `rusage` is plain data, and all zeros is one value of it.
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    // SAFETY: `getrusage` writes one `rusage` through the pointer.
    let read = unsafe { libc::getrusage(libc::RUSAGE_CHILDREN, &mut usage) };
    assert_eq!(read, 0, "read the resource use of the waited children");
    u64::try_from(usage.ru_maxrss).expect("a resident set size is not negative")
}

/// §109.2 rule 6: a child that passes its memory budget is one S026 at
/// the entry file, and the same source under the default profile keeps
/// the parser's own outcome with no child.
///
/// 65,476 labels is 130,990 bytes, inside S026's per-file limit of
/// 131,072, so the compile runs. The control is what the process
/// boundary closes: with no child the parser's outcome reaches the
/// caller, and on a host that kills the process that outcome is no
/// diagnostic at all.
#[test]
fn a_compile_over_the_memory_budget_reports_one_s026() -> Result<(), String> {
    /// The labels the chain spells.
    const LABELS: usize = 65_476;

    if skipped_as_heavy(concat!(
        "a_compile_over_the_memory_budget_reports_one_s026 ",
        "drives the compile child to the memory budget"
    )) {
        return Ok(());
    }
    let dir = TestDir::new()?;
    let source = dir.write("profile_label.ts", same_label_chain(LABELS).as_bytes())?;
    assert_eq!(
        std::fs::metadata(&source)
            .map_err(|error| format!("read profile_label.ts: {error}"))?
            .len(),
        130_990
    );

    let started = std::time::Instant::now();
    let stopped = output(
        subscript()
            .arg("check")
            .arg("--profile")
            .arg("sandbox")
            .arg(&source),
    )?;
    let wall = started.elapsed();
    assert_code(&stopped, 1);
    let rendered = String::from_utf8_lossy(&stopped.stderr).into_owned();
    assert!(
        rendered.starts_with("error[S026]: the compiler passed its memory budget\n"),
        "{rendered}"
    );
    assert!(
        rendered.contains(&source.display().to_string()),
        "the stop names the entry file: {rendered}"
    );

    // macOS refuses `RLIMIT_AS`, so the parent holds the budget by its
    // poll. The kernel's own figure for the child that just ended must
    // then stay under the budget plus one poll's growth. The reading
    // comes before the control below, which runs with no budget at all.
    #[cfg(target_os = "macos")]
    {
        let peak = largest_child_peak_bytes();
        println!("the budgeted child: {wall:?}, peak {peak} resident bytes");
        assert!(
            peak < MEMORY_BUDGET_BYTES + POLL_GROWTH_BYTES,
            "the peak is {peak} resident bytes"
        );
    }
    #[cfg(not(target_os = "macos"))]
    println!("the budgeted child: {wall:?}");

    // The control: the default profile spawns nothing, so the parser's
    // own outcome reaches the caller and no budget stop is reported.
    let unbudgeted = output(subscript().arg("check").arg(&source))?;
    assert_ne!(unbudgeted.status.code(), Some(0));
    let rendered = String::from_utf8_lossy(&unbudgeted.stderr).into_owned();
    assert!(!rendered.contains("S026"), "{rendered}");
    println!(
        "profile_label.ts under the default profile: status {}, {} bytes of stderr",
        unbudgeted.status,
        unbudgeted.stderr.len()
    );
    Ok(())
}

/// §109.2 rule 6: the parent kills a child that passes its time budget
/// and reports one S026 at the entry file.
///
/// The test-only variable shortens the budget to 2 s. The firing
/// control is the same source under the contract's budget: the parser
/// finishes and its own diagnostic reaches the caller.
#[test]
fn a_compile_over_the_time_budget_reports_one_s026() -> Result<(), String> {
    /// The nesting levels the source spells.
    const LEVELS: usize = 4_000;
    /// The seconds the test gives the child.
    const BUDGET: &str = "2";

    let dir = TestDir::new()?;
    let source = dir.write("nested_generic.ts", nested_generic_call(LEVELS).as_bytes())?;

    let started = std::time::Instant::now();
    let stopped = output(
        subscript()
            .env(subscript_cli::COMPILE_TIME_BUDGET_VARIABLE, BUDGET)
            .arg("check")
            .arg("--profile")
            .arg("sandbox")
            .arg(&source),
    )?;
    let wall = started.elapsed();
    assert_code(&stopped, 1);
    let rendered = String::from_utf8_lossy(&stopped.stderr).into_owned();
    assert!(
        rendered.starts_with("error[S026]: the compiler passed its time budget\n"),
        "{rendered}"
    );
    assert!(
        rendered.contains(&source.display().to_string()),
        "the stop names the entry file: {rendered}"
    );
    assert!(
        wall < std::time::Duration::from_secs(60),
        "the parent waited {wall:?} on a 2-second budget"
    );

    println!("nested_generic.ts at {LEVELS} levels: the 2-second budget stops it in {wall:?}");

    // The firing control: with the contract's budget the same source
    // reaches the parser's own diagnostic, so the budget is what
    // stopped the run above. The control runs under the 300 s budget,
    // so it is the heavy part of this test (§109.6a); the stop above
    // keeps its 2 s budget and stays in the quick gate.
    if skipped_as_heavy(concat!(
        "a_compile_over_the_time_budget_reports_one_s026 ",
        "omits its firing control under the 300 s budget"
    )) {
        return Ok(());
    }
    let started = std::time::Instant::now();
    let parsed = output(
        subscript()
            .arg("check")
            .arg("--profile")
            .arg("sandbox")
            .arg(&source),
    )?;
    let control_wall = started.elapsed();
    assert_code(&parsed, 1);
    let rendered = String::from_utf8_lossy(&parsed.stderr).into_owned();
    assert!(
        rendered.contains("error[S100]") && !rendered.contains("S026"),
        "{rendered}"
    );
    println!("the control reaches the parser's own diagnostic in {control_wall:?}");
    Ok(())
}

/// §109.2 rule 6: an accepted profile program compiles and runs through
/// the budgeted child, with its committed golden output.
#[test]
fn the_budgeted_child_runs_an_accepted_profile_program() -> Result<(), String> {
    let entry = workspace_root().join("corpus/accept/a238-sandbox-clean.ts");
    let golden = std::fs::read(entry.with_extension("expected"))
        .map_err(|error| format!("read the golden: {error}"))?;

    let checked = output(
        subscript()
            .arg("check")
            .arg("--profile")
            .arg("sandbox")
            .arg(&entry),
    )?;
    assert_code(&checked, 0);

    let ran = output(
        subscript()
            .arg("run")
            .arg("--profile")
            .arg("sandbox")
            .arg(&entry),
    )?;
    assert_code(&ran, 0);
    assert_eq!(ran.stdout, golden);
    Ok(())
}

/// A source that checks clean and prints one line.
fn clean_source() -> &'static [u8] {
    b"export function main(): void {\n  print(`ok`);\n}\n"
}

/// §109.2 rule 6: a child that stopped abnormally is one S026 that
/// names the signal or the exit code, and never a budget.
///
/// The two facts are derived apart: the message is the parent's, and
/// the exit code and the signal number are the host's own reading of a
/// child the test-only variable ended.
#[test]
fn an_abnormal_end_of_the_child_names_the_signal_or_the_exit_code() -> Result<(), String> {
    let dir = TestDir::new()?;
    let source = dir.write("clean.ts", clean_source())?;

    // The panic runtime picks the exit code; the parent reads it back.
    let panicked = output(
        subscript()
            .env(subscript_cli::COMPILE_CHILD_STOP_VARIABLE, "panic")
            .arg("check")
            .arg("--profile")
            .arg("sandbox")
            .arg(&source),
    )?;
    assert_code(&panicked, 1);
    let rendered = String::from_utf8_lossy(&panicked.stderr).into_owned();
    assert!(
        rendered.contains("error[S026]: the compiler stopped abnormally (exit code 101)"),
        "{rendered}"
    );
    assert!(
        rendered.contains(&source.display().to_string()),
        "the stop names the entry file: {rendered}"
    );
    // The child's own stderr reaches the caller before the stop.
    assert!(rendered.contains("panicked"), "{rendered}");
    // An abnormal stop is not a budget.
    assert!(!rendered.contains("passed its memory budget"), "{rendered}");

    #[cfg(unix)]
    {
        let terminated = output(
            subscript()
                .env(subscript_cli::COMPILE_CHILD_STOP_VARIABLE, "sigterm")
                .arg("check")
                .arg("--profile")
                .arg("sandbox")
                .arg(&source),
        )?;
        assert_code(&terminated, 1);
        let rendered = String::from_utf8_lossy(&terminated.stderr).into_owned();
        assert!(
            rendered.contains("error[S026]: the compiler stopped abnormally (signal 15)"),
            "{rendered}"
        );
        assert!(!rendered.contains("passed its memory budget"), "{rendered}");
    }

    // The control: with no test-only stop the same source checks clean
    // through the same child.
    let clean = output(
        subscript()
            .arg("check")
            .arg("--profile")
            .arg("sandbox")
            .arg(&source),
    )?;
    assert_code(&clean, 0);
    Ok(())
}

/// §109.2 rule 6: a time budget over the ceiling, and one that is not a
/// count of seconds, leave the contract's 300 s budget.
///
/// `2` is the accepted value in the list, and it runs the same path.
/// That the parent reads an accepted value at all is
/// `a_compile_over_the_time_budget_reports_one_s026`, which stops a
/// child with a 2 s value.
#[test]
fn a_time_budget_the_environment_cannot_ask_for_keeps_the_contract_budget() -> Result<(), String> {
    let dir = TestDir::new()?;
    let source = dir.write("clean.ts", clean_source())?;

    for value in ["18446744073709551615", "86401", "soon", "2"] {
        let checked = output(
            subscript()
                .env(subscript_cli::COMPILE_TIME_BUDGET_VARIABLE, value)
                .arg("check")
                .arg("--profile")
                .arg("sandbox")
                .arg(&source),
        )?;
        assert_code(&checked, 0);
        let rendered = String::from_utf8_lossy(&checked.stderr).into_owned();
        assert!(!rendered.contains("S026"), "{value}: {rendered}");
        assert!(rendered.contains("no errors"), "{value}: {rendered}");
    }
    Ok(())
}

/// §109.2 rule 6: a failed write of the parent's own stdout reports the
/// write error and keeps the child's outcome.
///
/// The reader closes the pipe before the parent writes, so the write
/// fails. The firing control is the same run with an open stdout: it
/// carries the golden bytes and reports no write error.
#[cfg(unix)]
#[test]
fn a_closed_stdout_keeps_the_childs_outcome_and_reports_the_write_error() -> Result<(), String> {
    use std::process::Stdio;

    let entry = workspace_root().join("corpus/accept/a238-sandbox-clean.ts");
    let golden = std::fs::read(entry.with_extension("expected"))
        .map_err(|error| format!("read the golden: {error}"))?;
    assert!(!golden.is_empty(), "the control needs bytes to write");

    let mut child = subscript()
        .arg("run")
        .arg("--profile")
        .arg("sandbox")
        .arg(&entry)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("run subscript: {error}"))?;
    drop(child.stdout.take());
    let closed = child
        .wait_with_output()
        .map_err(|error| format!("wait for subscript: {error}"))?;
    let rendered = String::from_utf8_lossy(&closed.stderr).into_owned();
    assert!(rendered.contains("write program stdout"), "{rendered}");
    assert_code(&closed, 0);

    // The firing control: the same run writes the golden and reports
    // no write error.
    let open = output(
        subscript()
            .arg("run")
            .arg("--profile")
            .arg("sandbox")
            .arg(&entry),
    )?;
    assert_code(&open, 0);
    assert_eq!(open.stdout, golden);
    let rendered = String::from_utf8_lossy(&open.stderr).into_owned();
    assert!(!rendered.contains("write program stdout"), "{rendered}");
    Ok(())
}

/// A C host that takes seconds to compile at `-O2`.
///
/// The preprocessor expands the chain, so the source is short and the
/// translation unit the optimizer reads is not. The function takes its
/// input from the caller and has external linkage, so the optimizer
/// folds nothing and drops nothing.
fn slow_host_c() -> Result<String, String> {
    subscript_codegen::host_entry(
        r#"
#define SUBSCRIPT_STEP_1 t = t * 3 + 1; t = t ^ (t >> 3);
#define SUBSCRIPT_STEP_2 SUBSCRIPT_STEP_1 SUBSCRIPT_STEP_1
#define SUBSCRIPT_STEP_4 SUBSCRIPT_STEP_2 SUBSCRIPT_STEP_2
#define SUBSCRIPT_STEP_8 SUBSCRIPT_STEP_4 SUBSCRIPT_STEP_4
#define SUBSCRIPT_STEP_16 SUBSCRIPT_STEP_8 SUBSCRIPT_STEP_8
#define SUBSCRIPT_STEP_32 SUBSCRIPT_STEP_16 SUBSCRIPT_STEP_16
#define SUBSCRIPT_STEP_64 SUBSCRIPT_STEP_32 SUBSCRIPT_STEP_32
#define SUBSCRIPT_STEP_128 SUBSCRIPT_STEP_64 SUBSCRIPT_STEP_64
#define SUBSCRIPT_STEP_256 SUBSCRIPT_STEP_128 SUBSCRIPT_STEP_128
#define SUBSCRIPT_STEP_512 SUBSCRIPT_STEP_256 SUBSCRIPT_STEP_256
#define SUBSCRIPT_STEP_1024 SUBSCRIPT_STEP_512 SUBSCRIPT_STEP_512
#define SUBSCRIPT_STEP_2048 SUBSCRIPT_STEP_1024 SUBSCRIPT_STEP_1024
#define SUBSCRIPT_STEP_4096 SUBSCRIPT_STEP_2048 SUBSCRIPT_STEP_2048
#define SUBSCRIPT_STEP_8192 SUBSCRIPT_STEP_4096 SUBSCRIPT_STEP_4096
#define SUBSCRIPT_STEP_16384 SUBSCRIPT_STEP_8192 SUBSCRIPT_STEP_8192
#define SUBSCRIPT_STEP_32768 SUBSCRIPT_STEP_16384 SUBSCRIPT_STEP_16384
int subscript_slow_host(int a);
int subscript_slow_host(int a) {
    int t = a;
    SUBSCRIPT_STEP_32768
    return t;
}
int main(void) { return 0; }
"#,
    )
}

/// §109.2 rule 6: a budget kill reaches the child's whole process
/// group, so the C compiler the child started dies with it.
///
/// The two facts are derived apart: the parent reports one S026, and
/// the executable is the C compiler's own record of reaching its end.
/// The firing control is the same build under the contract's budget: it
/// writes the executable. The wait after the killed build is the
/// control's own wall time, so a slower host waits longer.
#[test]
fn a_budget_kill_reaches_the_c_compiler_the_child_started() -> Result<(), String> {
    /// The seconds the killed build gets. The C compile is what runs
    /// when it elapses: the emit before it takes about one second of
    /// the whole build.
    const BUDGET: &str = "2";

    let dir = TestDir::new()?;
    let source = dir.write("tiny.ts", clean_source())?;
    let host = dir.write("host.c", slow_host_c()?.as_bytes())?;
    let runtime = subscript_codegen::runtime_staticlib_path().map_err(|error| error.to_string())?;
    let include = workspace_root().join("runtime").join("include");
    let build = |out: &Path| {
        let mut command = subscript();
        command
            .arg("build")
            .arg("--profile")
            .arg("sandbox")
            .arg("--source")
            .arg(&source)
            .arg("--host")
            .arg(&host)
            .arg("-o")
            .arg(out)
            .arg("--runtime-lib")
            .arg(&runtime)
            .arg("--runtime-include")
            .arg(&include);
        command
    };

    // The firing control: the C compiler reaches its end and writes.
    let control_out = dir.directory("control")?;
    let executable = control_out.join(format!("tiny{}", std::env::consts::EXE_SUFFIX));
    let started = std::time::Instant::now();
    let built = output(&mut build(&control_out))?;
    let control_wall = started.elapsed();
    assert_code(&built, 0);
    assert!(executable.is_file(), "the control writes the executable");
    println!("the whole build: {control_wall:?}");

    let killed_out = dir.directory("killed")?;
    let executable = killed_out.join(format!("tiny{}", std::env::consts::EXE_SUFFIX));
    let started = std::time::Instant::now();
    let stopped =
        output(build(&killed_out).env(subscript_cli::COMPILE_TIME_BUDGET_VARIABLE, BUDGET))?;
    let killed_wall = started.elapsed();
    assert_code(&stopped, 1);
    let rendered = String::from_utf8_lossy(&stopped.stderr).into_owned();
    assert!(
        rendered.starts_with("error[S026]: the compiler passed its time budget\n"),
        "{rendered}"
    );
    assert!(
        killed_wall < control_wall,
        "the budget stopped the build before its end: {killed_wall:?} against {control_wall:?}"
    );
    // The emitted C is what the C compiler reads, so the kill reached a
    // build that had started its C compile.
    assert!(
        killed_out.join("program.c").is_file(),
        "the child emitted the C before the budget elapsed"
    );

    // A C compiler that outlived the kill has less work left than the
    // whole build took, so this wait covers it.
    std::thread::sleep(control_wall);
    assert!(
        !executable.is_file(),
        "the killed C compiler wrote {}",
        executable.display()
    );
    println!("the killed build: {killed_wall:?}, no executable after {control_wall:?} more");
    Ok(())
}

/// §109.2 rule 6: the private flag is not a user option, on every
/// subcommand.
///
/// The firing control is each command with no flag: it runs.
#[test]
fn the_compile_child_flag_is_not_a_user_option() -> Result<(), String> {
    let dir = TestDir::new()?;
    let source = dir.write("clean.ts", clean_source())?;
    let header = dir.write("host.h", b"void host_tick(void);\n")?;
    let emitted = dir.directory("emitted")?;
    let bound = dir.0.join("host.ts");

    let commands: [(&str, Vec<OsString>); 4] = [
        ("check", vec![source.clone().into()]),
        ("run", vec![source.clone().into()]),
        (
            "emit",
            vec![source.clone().into(), "-o".into(), emitted.clone().into()],
        ),
        (
            "bind",
            vec![
                "--header".into(),
                header.clone().into(),
                "-o".into(),
                bound.clone().into(),
            ],
        ),
    ];
    for (command, args) in commands {
        let refused = output(subscript().arg(command).args(&args).arg("--compile-child"))?;
        assert_code(&refused, 2);
        let rendered = String::from_utf8_lossy(&refused.stderr).into_owned();
        assert_eq!(
            rendered.trim_end(),
            "subscript: the compile child flag is not a user option",
            "{command}"
        );

        // The firing control: the same command with no flag runs.
        let ran = output(subscript().arg(command).args(&args))?;
        assert_code(&ran, 0);
    }
    Ok(())
}
