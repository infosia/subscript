//! End-to-end clean and contracted-error paths for every CLI subcommand.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

#[path = "../../codegen/tests/corpus/mod.rs"]
#[allow(dead_code)]
mod corpus;

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
    let source = workspace_root().join("corpus/trap/t51-bytes-into-range.ts");
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
    assert!(String::from_utf8_lossy(&stderr).contains("index-out-of-bounds"));
}

fn directory_cases() -> Vec<(String, Vec<std::path::PathBuf>)> {
    let root = workspace_root();
    let accept = root.join("corpus/accept");
    corpus::entry_ids(&accept)
        .into_iter()
        .filter(|id| accept.join(id).is_dir())
        .map(|id| {
            let mirrors = corpus::entry_sources(&accept, &id)
                .into_iter()
                .filter(|source| source.dts)
                .map(|source| root.join("corpus/interop").join(source.name))
                .collect();
            (id, mirrors)
        })
        .collect()
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

/// The committed engine mirror is generated with the explicit callback
/// lifetime selected for `EngineRequestInfo` (`specs/blocks/compiler.md`
/// §111 rule 1), so both bind modes pass that selection here.
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
            .arg(header)
            .arg("--explicit-callback-lifetime")
            .arg("EngineRequestInfo"),
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
            .arg("--explicit-callback-lifetime")
            .arg("EngineRequestInfo")
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
fn directory_programs_check_and_run_with_the_committed_goldens() -> Result<(), String> {
    let root = workspace_root();
    for (id, mirrors) in directory_cases() {
        let entry = Path::new("corpus/accept").join(&id).join("main.ts");
        let mut command = subscript();
        command.current_dir(&root).arg("check").arg(&entry);
        for mirror in &mirrors {
            command.arg("--mirror").arg(mirror);
        }
        let checked = output(&mut command)?;
        assert_code(&checked, 0);
        assert!(checked.stdout.is_empty());
        assert_eq!(
            checked.stderr,
            format!("check: {}: no errors\n", entry.display()).as_bytes()
        );
        let directory = TestDir::new()?;
        let mut command = subscript();
        command.current_dir(&root);
        if mirrors.is_empty() {
            command.arg("run").arg(&entry);
        } else {
            command
                .arg("build")
                .arg("--source")
                .arg(&entry)
                .arg("--run")
                .arg("-o")
                .arg(&directory.0)
                .arg("--runtime-lib")
                .arg(
                    subscript_codegen::runtime_staticlib_path()
                        .map_err(|error| error.to_string())?,
                )
                .arg("--runtime-include")
                .arg(root.join("runtime/include"));
            for mirror in &mirrors {
                command.arg("--mirror").arg(mirror);
            }
        }
        let run = output(&mut command)?;
        assert_code(&run, 0);
        assert_eq!(
            run.stdout,
            corpus::golden_bytes(&root.join("corpus/accept"), &id),
            "{id}"
        );
        assert!(run.stderr.is_empty());
    }
    Ok(())
}

#[test]
fn directory_emit_matches_the_shared_directory_reader() -> Result<(), String> {
    let root = workspace_root();
    for (id, mirrors) in directory_cases() {
        let directory = TestDir::new()?;
        let cli_output = directory.0.join("cli");
        let shared_output = directory.0.join("shared");
        let entry = Path::new("corpus/accept").join(&id).join("main.ts");
        let mut command = subscript();
        command
            .current_dir(&root)
            .arg("emit")
            .arg(&entry)
            .arg("-o")
            .arg(&cli_output);
        for mirror in &mirrors {
            command.arg("--mirror").arg(mirror);
        }
        let emitted = output(&mut command)?;
        assert_code(&emitted, 0);
        assert!(emitted.stdout.is_empty());
        assert!(emitted.stderr.is_empty());
        let sources = corpus::entry_sources(&root.join("corpus/accept"), &id);
        subscript_codegen::emit_c_files(&sources, &shared_output, "program", true)
            .map_err(|error| format!("emit {id}: {error}"))?;
        for name in ["program.c", "program.alloc.h", "entry.c"] {
            let cli = std::fs::read(cli_output.join(name)).map_err(|error| error.to_string())?;
            let shared =
                std::fs::read(shared_output.join(name)).map_err(|error| error.to_string())?;
            assert_eq!(cli, shared, "{id}: {name} differs");
        }
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
            "export function root(): void { print(`1`); }\n",
            "export function main(): void { helper(); }\n",
        )
        .as_bytes(),
    )?;
    directory.write(
        "other.ts",
        concat!(
            "import { root } from \"./main\";\n",
            "export function helper(): void { root(); }\n",
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
    let ran = output(
        subscript()
            .current_dir(&directory.0)
            .args(["run", "main.ts"]),
    )?;
    assert_code(&ran, 0);
    assert_eq!(ran.stdout, b"1\n");

    // The cycle must not hide an invalid entry or duplicate its diagnostic.
    directory.write("main.ts", b"import { helper } from './other';\nexport function root(): i32 { return 1; }\nexport function main(): void { helper(); }\n")?;
    let rejected = output(
        subscript()
            .current_dir(&directory.0)
            .args(["check", "main.ts"]),
    )?;
    assert_code(&rejected, 1);
    let diagnostic = String::from_utf8_lossy(&rejected.stderr);
    assert!(diagnostic.contains("entry export `root`: host entries must return void"));
    assert_eq!(diagnostic.matches("error[S100]").count(), 1);
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

#[test]
fn named_re_export_chains_load_their_sources() -> Result<(), String> {
    // One native run checks loading; the failure control only checks types.
    let start = std::time::Instant::now();
    let directory = TestDir::new()?;
    directory.write("lib.ts", b"export function value(): i32 { return 7; }")?;
    directory.write("bridge.ts", b"export { value as other } from \"./lib\";")?;
    directory.write("surface.ts", b"export { other } from \"./bridge\";")?;
    directory.write(
        "main.ts",
        b"import { other } from \"./surface\"; export function main(): void { print(`${other()}`); }",
    )?;
    let run = output(
        subscript()
            .current_dir(&directory.0)
            .arg("run")
            .arg("main.ts"),
    )?;
    assert_code(&run, 0);
    assert_eq!(run.stdout, b"7\n");
    directory.write("lib.ts", b"export function different(): i32 { return 7; }")?;
    let checked = output(
        subscript()
            .current_dir(&directory.0)
            .arg("check")
            .arg("main.ts"),
    )?;
    assert_code(&checked, 1);
    assert!(String::from_utf8_lossy(&checked.stderr).contains("`value` is not exported by `./lib`"));
    eprintln!("named re-export CLI chain: {:?}", start.elapsed());
    Ok(())
}

#[test]
fn cli_and_corpus_keep_import_initialization_order() -> Result<(), String> {
    let start = std::time::Instant::now();
    let directory = TestDir::new()?;
    directory.directory("program")?;
    directory.write(
        "program/a.ts",
        b"function init(): i32 { print(\"init a\"); return 1; } export const a: i32 = init();",
    )?;
    directory.write(
        "program/b.ts",
        b"function init(): i32 { print(\"init b\"); return 2; } export const b: i32 = init();",
    )?;
    for (imports, names, expected) in [
        (
            "import { b } from './b'; import { a } from './a';",
            ["main.ts", "b.ts", "a.ts"],
            "init b\ninit a\n1 2\n",
        ),
        (
            "import { a } from './a'; import { b } from './b';",
            ["main.ts", "a.ts", "b.ts"],
            "init a\ninit b\n1 2\n",
        ),
    ] {
        directory.write(
            "program/main.ts",
            format!("{imports} export function main(): void {{ print(`${{a}} ${{b}}`); }}")
                .as_bytes(),
        )?;
        let sources = corpus::entry_sources(&directory.0, "program");
        assert_eq!(
            sources.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(),
            names
        );
        let module = subscript_compiler::check_program(&sources).map_err(|e| format!("{e:?}"))?;
        let lir = subscript_codegen::lir::lower_module(&module).map_err(|e| e.to_string())?;
        assert_eq!(
            subscript_codegen::interpreter::interpret(&lir).map_err(|e| e.to_string())?,
            expected.as_bytes()
        );
        let run = output(
            subscript()
                .current_dir(&directory.0)
                .arg("run")
                .arg("program/main.ts"),
        )?;
        assert_code(&run, 0);
        assert_eq!(run.stdout, expected.as_bytes());
        assert!(run.stderr.is_empty());
        let emitted = output(
            subscript()
                .current_dir(&directory.0)
                .arg("emit")
                .arg("program/main.ts")
                .arg("-o")
                .arg("cli"),
        )?;
        assert_code(&emitted, 0);
        subscript_codegen::emit_c_files(&sources, &directory.0.join("shared"), "program", true)
            .map_err(|e| e.to_string())?;
        for name in ["program.c", "program.alloc.h", "entry.c"] {
            assert_eq!(
                std::fs::read(directory.0.join("cli").join(name)).map_err(|e| e.to_string())?,
                std::fs::read(directory.0.join("shared").join(name)).map_err(|e| e.to_string())?,
                "{name}"
            );
        }
    }
    directory.write("program/main.ts", b"import { b } from './b'; import { y } from './a'; export function main(): void { print(`y=${y} b=${b}`); }")?;
    directory.write("program/b.ts", b"export const b: i32 = 5;")?;
    directory.write(
        "program/a.ts",
        b"import { b } from './b'; export const y: i32 = b + 1;",
    )?;
    let run = output(
        subscript()
            .current_dir(&directory.0)
            .arg("run")
            .arg("program/main.ts"),
    )?;
    assert_code(&run, 0);
    assert_eq!(run.stdout, b"y=6 b=5\n");
    eprintln!("CLI initialization order test: {:?}", start.elapsed());
    Ok(())
}

#[test]
fn entryless_alias_api_builds_for_a_host_and_cli_main_has_a_control() -> Result<(), String> {
    let started = std::time::Instant::now();
    let directory = TestDir::new()?;
    directory.write(
        "api.ts",
        b"export { update as first, update as second } from './lib';",
    )?;
    directory.write("lib.ts", b"let count: i32 = 0; export function update(): void { count += 1; print(`${count}`); } export function main(): void { print('library main'); }")?;
    let checked = output(
        subscript()
            .current_dir(&directory.0)
            .args(["check", "api.ts"]),
    )?;
    assert_code(&checked, 0);
    let missing = output(
        subscript()
            .current_dir(&directory.0)
            .args(["run", "api.ts"]),
    )?;
    assert_code(&missing, 1);
    assert!(String::from_utf8_lossy(&missing.stderr)
        .contains("error[S100]: entry module exports no host entry `main`"));
    assert!(String::from_utf8_lossy(&missing.stderr).contains("api.ts:1:1"));
    let emitted = output(subscript().current_dir(&directory.0).args([
        "emit",
        "api.ts",
        "--no-entry",
        "-o",
        "out",
    ]))?;
    assert_code(&emitted, 0);
    let header =
        std::fs::read_to_string(directory.0.join("out/program.h")).map_err(|e| e.to_string())?;
    let host = subscript_codegen::host_entry(
        r#"
#include "out/program.h"
#include <stdio.h>
int main(void) {
    subscript_rt_context* ctx = subscript_rt_ctx_new();
    if (ctx == NULL) return 2;
    subscript_rt_ctx_enter_script(ctx);
    subscript_init(ctx);
    subscript_export_first(ctx);
    subscript_export_second(ctx);
    subscript_rt_ctx_exit_script(ctx);
    uint64_t length = 0;
    const uint8_t* bytes = subscript_rt_ctx_stdout(ctx, &length);
    if (length != 0) fwrite(bytes, 1, (size_t)length, stdout);
    int failed = subscript_rt_ctx_trap_kind(ctx) != 0;
    subscript_rt_ctx_release(ctx);
    return failed;
}
"#,
        &header,
    )?;
    directory.write("host.c", host.as_bytes())?;
    let runtime = subscript_codegen::runtime_staticlib_path().map_err(|error| error.to_string())?;
    let include = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../runtime/include");
    let built = output(
        subscript()
            .env("SUBSCRIPT_RUNTIME_LIB", &runtime)
            .env("SUBSCRIPT_RUNTIME_INCLUDE", &include)
            .current_dir(&directory.0)
            .args([
                "build", "--source", "api.ts", "--host", "host.c", "-o", "out", "--run",
            ]),
    )?;
    assert_code(&built, 0);
    assert_eq!(built.stdout, b"1\n2\n");
    let header =
        std::fs::read_to_string(directory.0.join("out/program.h")).map_err(|e| e.to_string())?;
    assert!(!header.contains("void subscript_export_main("));
    assert!(!header.contains("void subscript_export_update("));
    directory.write("api.ts", b"export { update as main } from './lib';")?;
    let present = output(
        subscript()
            .current_dir(&directory.0)
            .args(["run", "api.ts"]),
    )?;
    assert_code(&present, 0);
    assert_eq!(present.stdout, b"1\n");
    eprintln!("host API CLI and ship control: {:?}", started.elapsed());
    Ok(())
}
