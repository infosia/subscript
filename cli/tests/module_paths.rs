//! A module is named by its path (compiler.md §354 rule 8, acceptance item 6).
//!
//! Cost: six `run` commands, four `build --run` commands of which three
//! reach a C compile, and seven `check` commands. Each program is small;
//! the C compiles dominate.

use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

struct TestDir(PathBuf);

impl TestDir {
    fn new() -> Result<Self, String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "subscript-cli-module-paths-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path)
            .map_err(|error| format!("create {}: {error}", path.display()))?;
        Ok(Self(path))
    }

    fn write(&self, relative: &str, text: &str) -> Result<(), String> {
        let path = self.0.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("create {}: {error}", parent.display()))?;
        }
        std::fs::write(&path, text).map_err(|error| format!("write {}: {error}", path.display()))
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn subscript(directory: &TestDir, args: &[&str]) -> Result<Output, String> {
    Command::new(env!("CARGO_BIN_EXE_subscript"))
        .current_dir(&directory.0)
        .args(args)
        .output()
        .map_err(|error| format!("run subscript {args:?}: {error}"))
}

fn assert_success(output: &Output, expected: &[u8], label: &str) {
    assert_eq!(
        output.status.code(),
        Some(0),
        "{label}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(expected),
        "{label}"
    );
}

const X: &str = "export function x(): i32 { return 7; }\n";

/// The three probes of compiler.md §354 problem 1, and two modules with one
/// base name. The test proves that an import into a subdirectory, an
/// import from a subdirectory, and an import through `../` each load and
/// run, and that two files with one base name are two modules.
fn write_program(directory: &TestDir) -> Result<(), String> {
    directory.write("sub/x.ts", X)?;
    directory.write(
        "sub/a.ts",
        "import { x } from \"./x\";\nexport function a(): i32 { return x() + 1; }\n",
    )?;
    directory.write(
        "a/util.ts",
        "export function name(): string { return \"a\"; }\n",
    )?;
    directory.write(
        "b/util.ts",
        "export function name(): string { return \"b\"; }\n",
    )?;
    directory.write(
        "main.ts",
        concat!(
            "import { x } from \"./sub/x\";\n",
            "import { name as first } from \"./a/util\";\n",
            "import { name as second } from \"./b/util\";\n",
            "export function main(): void { print(`${x()} ${first()} ${second()}`); }\n",
        ),
    )?;
    directory.write(
        "main2.ts",
        "import { a } from \"./sub/a\";\nexport function main(): void { print(`${a()}`); }\n",
    )?;
    directory.write(
        "other/up.ts",
        "import { x } from \"../sub/x\";\nexport function main(): void { print(`up ${x()}`); }\n",
    )
}

#[test]
fn imports_through_directories_run_on_both_tiers() -> Result<(), String> {
    let directory = TestDir::new()?;
    write_program(&directory)?;
    let runtime_lib = subscript_codegen::runtime_staticlib_path()
        .map_err(|error| error.to_string())?
        .to_string_lossy()
        .into_owned();
    let runtime_include = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../runtime/include")
        .to_string_lossy()
        .into_owned();
    for (entry, output, expected) in [
        ("main.ts", "out-main", &b"7 a b\n"[..]),
        ("main2.ts", "out-main2", b"8\n"),
        ("other/up.ts", "out-up", b"up 7\n"),
    ] {
        let run = subscript(&directory, &["run", entry])?;
        assert_success(&run, expected, &format!("run {entry}"));
        let built = subscript(
            &directory,
            &[
                "build",
                "--source",
                entry,
                "-o",
                output,
                "--run",
                "--runtime-lib",
                &runtime_lib,
                "--runtime-include",
                &runtime_include,
            ],
        )?;
        assert_success(&built, expected, &format!("build --run {entry}"));
    }
    Ok(())
}

fn type_error_output(name: &str) -> String {
    format!(
        concat!(
            "error[S100]: type mismatch: the return value expects `i32`, got `string`\n",
            " --> {0}:1:35\n",
            "  |\n",
            "1 | export function x(): i32 {{ return \"s\"; }}\n",
            "  |                                   ^\n",
            "  = rule: Constructs outside the decided language surface are rejected.\n",
            "error: 1 error(s)\n",
        ),
        name
    )
}

#[test]
fn a_diagnostic_in_a_subdirectory_names_the_file_by_its_path() -> Result<(), String> {
    let directory = TestDir::new()?;
    write_program(&directory)?;
    directory.write("sub/x.ts", "export function x(): i32 { return \"s\"; }\n")?;
    // compiler.md §354 rule 8: the root of the names is the directory of the
    // entry, moved up by the leading `..` segments of the imports. For
    // `other/up.ts` the root is the parent of `other/`, so the name is
    // `sub/x.ts` there too.
    for args in [
        &["check", "main.ts"][..],
        &["run", "main.ts"],
        &["build", "--source", "main.ts", "-o", "out", "--run"],
        &["check", "main2.ts"],
        &["check", "other/up.ts"],
    ] {
        let output = subscript(&directory, args)?;
        assert_eq!(output.status.code(), Some(1), "{args:?}");
        assert!(output.stdout.is_empty(), "{args:?}");
        assert_eq!(
            String::from_utf8_lossy(&output.stderr),
            type_error_output("sub/x.ts"),
            "{args:?}"
        );
    }
    Ok(())
}

#[test]
fn a_missing_module_in_a_subdirectory_is_s100_at_the_specifier() -> Result<(), String> {
    let directory = TestDir::new()?;
    directory.write("sub/x.ts", X)?;
    directory.write(
        "main.ts",
        "import { y } from \"./sub/missing\";\nexport function main(): void {}\n",
    )?;
    let output = subscript(&directory, &["check", "main.ts"])?;
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        concat!(
            "error[S100]: imported module `./sub/missing` is not among the program's files\n",
            " --> main.ts:1:19\n",
            "  |\n",
            "1 | import { y } from \"./sub/missing\";\n",
            "  |                   ^\n",
            "  = rule: Constructs outside the decided language surface are rejected.\n",
            "error: 1 error(s)\n",
        )
    );
    Ok(())
}

// compiler.md §354 rule 8: an import that climbs out of the directory of the
// entry and back moves the root of the names up, so the import resolves.
// Cost: one dev-tier run and one CLI check.
#[test]
fn an_import_that_climbs_and_returns_resolves() -> Result<(), String> {
    let directory = TestDir::new()?;
    directory.write("sub/x.ts", X)?;
    directory.write(
        "sub/m.ts",
        "import { x } from \"../sub/x\";\nexport function main(): void { print(`m ${x()}`); }\n",
    )?;
    let run = subscript(&directory, &["run", "sub/m.ts"])?;
    assert_success(&run, b"m 7\n", "run sub/m.ts");
    // The root is the parent of `sub/`, so the names are `sub/m.ts` and `sub/x.ts`.
    directory.write("sub/x.ts", "export function x(): i32 { return \"s\"; }\n")?;
    let output = subscript(&directory, &["check", "sub/m.ts"])?;
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        type_error_output("sub/x.ts")
    );
    Ok(())
}

// compiler.md §354 rule 8: an entry whose imports do not climb is the top of
// the names. Cost: one CLI check.
#[test]
fn an_entry_with_only_descending_imports_keeps_its_file_name() -> Result<(), String> {
    let directory = TestDir::new()?;
    directory.write("a/x.ts", X)?;
    directory.write(
        "main.ts",
        "import { x } from \"./a/x\";\nexport function main(): void { const s: i32 = \"s\"; }\n",
    )?;
    let output = subscript(&directory, &["check", "main.ts"])?;
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains(" --> main.ts:2:"), "{stderr}");
    Ok(())
}

// compiler.md §354 rule 8: a name keeps the spelling of the specifier. On a
// case-insensitive file system `./Util` reads `util.ts` and the import
// resolves. The test skips where the temporary directory is case-sensitive,
// because no file is read there. Cost: one `run` command.
#[test]
fn a_specifier_with_other_case_resolves_on_a_case_insensitive_file_system() -> Result<(), String> {
    let directory = TestDir::new()?;
    directory.write("util.ts", X)?;
    if !directory.0.join("UTIL.TS").is_file() {
        return Ok(());
    }
    directory.write(
        "main.ts",
        "import { x } from \"./Util\";\nexport function main(): void { print(`${x()}`); }\n",
    )?;
    let run = subscript(&directory, &["run", "main.ts"])?;
    assert_success(&run, b"7\n", "run main.ts");
    Ok(())
}

// compiler.md §354 rules 2 and 8: a specifier that names a directory loads
// no file and is S100 at the specifier. Cost: one `check` command.
#[test]
fn a_specifier_that_names_a_directory_is_s100() -> Result<(), String> {
    let directory = TestDir::new()?;
    directory.write("d.ts/x.ts", X)?;
    directory.write(
        "program/main.ts",
        concat!(
            "import { a } from \"../\";\n",
            "import { b } from \"./..\";\n",
            "import { c } from \"./\";\n",
            "import { d } from \"../d\";\n",
            "export function main(): void {}\n",
        ),
    )?;
    let output = subscript(&directory, &["check", "program/main.ts"])?;
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    for (line, specifier) in [(1, "../"), (2, "./.."), (3, "./"), (4, "../d")] {
        assert!(
            stderr.contains(&format!(
                "imported module `{specifier}` is not among the program's files\n --> main.ts:{line}:19\n"
            )),
            "{stderr}"
        );
    }
    assert!(stderr.ends_with("error: 4 error(s)\n"), "{stderr}");
    Ok(())
}
