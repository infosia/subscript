//! `subscript bind --explicit-callback-lifetime <aggregate>`
//! (`specs/blocks/cli.md` §10.1, `specs/blocks/compiler.md` §111 rule 1).
//!
//! The option reaches the binder on both output paths, and a rejected
//! selection exits 1 and writes no mirror.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

const DIRECTIVE: &str = "// @subscript-c-callback-lifetime aggregate=\"SubCallbackInfo\"";
const CALLBACK_RECORD: &str = "// @subscript-c-callback typedef=\"SubLogCallback\"\n";
/// The aggregate the committed mirror already carries. A run that
/// regenerates that mirror repeats this selection, so the test compares
/// against the committed text plus one added directive.
const COMMITTED_AGGREGATE: &str = "SubRequestInfo";

struct TestDir(PathBuf);

impl TestDir {
    fn new() -> Result<Self, String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "subscript-cli-bind-lifetime-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path)
            .map_err(|error| format!("create {}: {error}", path.display()))?;
        Ok(Self(path))
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
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

fn header() -> PathBuf {
    Path::new("corpus/interop/interop.h").to_path_buf()
}

/// Builds the wanted text from the committed mirror plus one more
/// directive line, which follows the record of the callback typedef.
/// `SubCallbackInfo` is declared before `SubRequestInfo`, so its
/// directive precedes the one the committed mirror carries.
fn committed_mirror_with_the_directive() -> Result<Vec<u8>, String> {
    let committed =
        std::fs::read_to_string(workspace_root().join("corpus/interop/interop.generated.d.ts"))
            .map_err(|error| format!("read committed mirror: {error}"))?;
    let at = committed
        .find(CALLBACK_RECORD)
        .ok_or("the committed mirror carries the callback record")?
        + CALLBACK_RECORD.len();
    let mut expected = String::with_capacity(committed.len() + DIRECTIVE.len() + 1);
    expected.push_str(&committed[..at]);
    expected.push_str(DIRECTIVE);
    expected.push('\n');
    expected.push_str(&committed[at..]);
    Ok(expected.into_bytes())
}

#[test]
fn the_option_reaches_stdout_and_the_output_file() -> Result<(), String> {
    let root = workspace_root();
    let expected = committed_mirror_with_the_directive()?;

    // The second selection is reversed against the first, because §111
    // rule 1 makes the mirror a function of the selected set, not of the
    // option order.
    let to_stdout = output(
        subscript()
            .current_dir(&root)
            .arg("bind")
            .arg("--header")
            .arg(header())
            .arg("--explicit-callback-lifetime")
            .arg("SubCallbackInfo")
            .arg("--explicit-callback-lifetime")
            .arg(COMMITTED_AGGREGATE),
    )?;
    assert_code(&to_stdout, 0);
    assert_eq!(to_stdout.stdout, expected);
    assert!(to_stdout.stderr.is_empty());

    let directory = TestDir::new()?;
    let mirror = directory.0.join("interop.d.ts");
    let to_file = output(
        subscript()
            .current_dir(&root)
            .arg("bind")
            .arg(header())
            .arg("--explicit-callback-lifetime")
            .arg(COMMITTED_AGGREGATE)
            .arg("--explicit-callback-lifetime")
            .arg("SubCallbackInfo")
            .arg("-o")
            .arg(&mirror),
    )?;
    assert_code(&to_file, 0);
    assert!(to_file.stdout.is_empty());
    assert!(to_file.stderr.is_empty());
    let written =
        std::fs::read(&mirror).map_err(|error| format!("read {}: {error}", mirror.display()))?;
    assert_eq!(written, expected);
    Ok(())
}

#[test]
fn an_aggregate_the_header_does_not_declare_exits_one_and_writes_no_file() -> Result<(), String> {
    let directory = TestDir::new()?;
    let mirror = directory.0.join("absent.d.ts");
    let result = output(
        subscript()
            .current_dir(workspace_root())
            .arg("bind")
            .arg("--header")
            .arg(header())
            .arg("--explicit-callback-lifetime")
            .arg("SubAbsentAggregate")
            .arg("-o")
            .arg(&mirror),
    )?;
    assert_code(&result, 1);
    assert!(result.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(stderr.contains("SubAbsentAggregate"), "{stderr}");
    assert!(
        !mirror.exists(),
        "a rejected selection left {}",
        mirror.display()
    );
    Ok(())
}

#[test]
fn an_aggregate_without_a_callback_field_exits_one_and_writes_no_file() -> Result<(), String> {
    let directory = TestDir::new()?;
    let mirror = directory.0.join("no-callback.d.ts");
    let result = output(
        subscript()
            .current_dir(workspace_root())
            .arg("bind")
            .arg("--header")
            .arg(header())
            .arg("--explicit-callback-lifetime")
            .arg("SubTransform")
            .arg("-o")
            .arg(&mirror),
    )?;
    assert_code(&result, 1);
    assert!(result.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(stderr.contains("SubTransform"), "{stderr}");
    assert!(stderr.contains("no callback field"), "{stderr}");
    assert!(
        !mirror.exists(),
        "a rejected selection left {}",
        mirror.display()
    );
    Ok(())
}

#[test]
fn the_option_repeats_and_selects_each_aggregate_one_time() -> Result<(), String> {
    let result = output(
        subscript()
            .current_dir(workspace_root())
            .arg("bind")
            .arg("--header")
            .arg(header())
            .arg("--explicit-callback-lifetime")
            .arg("SubCallbackInfo")
            .arg("--explicit-callback-lifetime")
            .arg("SubCallbackInfo"),
    )?;
    assert_code(&result, 1);
    assert!(result.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(stderr.contains("duplicate"), "{stderr}");
    assert!(stderr.contains("SubCallbackInfo"), "{stderr}");
    Ok(())
}

#[test]
fn the_option_without_a_value_is_a_usage_failure() -> Result<(), String> {
    let result = output(
        subscript()
            .current_dir(workspace_root())
            .arg("bind")
            .arg("--header")
            .arg(header())
            .arg("--explicit-callback-lifetime"),
    )?;
    assert_code(&result, 2);
    assert!(result.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        stderr.contains("--explicit-callback-lifetime requires a value"),
        "{stderr}"
    );
    Ok(())
}
