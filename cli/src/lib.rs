#![warn(missing_docs)]
//! Implementation of the `subscript` developer command.

mod runtime_paths;

use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use runtime_paths::{resolve_runtime_paths, RuntimeEnvironment, RuntimeOverrides, RuntimePaths};
use subscript_codegen::{
    add_c11_optimized_flags, add_executable_output, add_object_directory, emit_c_files,
    host_c_compiler, include_directory_arg, run_jit, runtime_system_libraries, CCompilerStyle,
    EmitCFilesError, RunError,
};
use subscript_compiler::{check_program, Diagnostic, SourceFile};

const SUCCESS: u8 = 0;
const PROGRAM_ERROR: u8 = 1;
const USAGE_ERROR: u8 = 2;

#[derive(Debug)]
struct Failure {
    code: u8,
    message: String,
}

impl Failure {
    fn program(message: impl Into<String>) -> Self {
        Self {
            code: PROGRAM_ERROR,
            message: message.into(),
        }
    }

    fn usage(message: impl Into<String>) -> Self {
        Self {
            code: USAGE_ERROR,
            message: message.into(),
        }
    }
}

/// Executes one CLI invocation and returns the process exit code.
///
/// `args` excludes the executable name. Requested answers and program
/// output are written to `stdout`; diagnostics, compiler output, and
/// environment errors are written to `stderr`.
pub fn execute<I, O, E>(args: I, stdout: &mut O, stderr: &mut E) -> u8
where
    I: IntoIterator<Item = OsString>,
    O: Write,
    E: Write,
{
    let args = args.into_iter().collect::<Vec<_>>();
    let result = dispatch(&args, stdout, stderr);
    match result {
        Ok(code) => code,
        Err(failure) => {
            let _ = writeln!(stderr, "subscript: {}", failure.message);
            failure.code
        }
    }
}

fn dispatch<O: Write, E: Write>(
    args: &[OsString],
    stdout: &mut O,
    stderr: &mut E,
) -> Result<u8, Failure> {
    let Some(command) = args.first().and_then(|arg| arg.to_str()) else {
        return Err(Failure::usage(usage()));
    };
    match command {
        "check" => check_command(&args[1..]),
        "emit" => emit_command(&args[1..]),
        "link-flags" => link_flags_command(&args[1..], stdout),
        "build" => build_command(&args[1..], stdout, stderr),
        "run" => run_command(&args[1..], stdout),
        _ => Err(Failure::usage(format!(
            "unknown subcommand `{command}`; {}",
            usage()
        ))),
    }
}

fn usage() -> &'static str {
    "usage: subscript <check|emit|link-flags|build|run> ..."
}

#[derive(Debug, Default)]
struct SourceArguments {
    source: Option<PathBuf>,
    mirrors: Vec<PathBuf>,
}

fn check_command(args: &[OsString]) -> Result<u8, Failure> {
    let parsed = parse_source_arguments(args)?;
    let files = load_program(
        parsed
            .source
            .as_ref()
            .ok_or_else(|| Failure::usage("check requires <file.ts>"))?,
        &parsed.mirrors,
    )?;
    match check_program(&files) {
        Ok(_) => Ok(SUCCESS),
        Err(diagnostics) => Err(rejection(diagnostics)),
    }
}

fn parse_source_arguments(args: &[OsString]) -> Result<SourceArguments, Failure> {
    let mut parsed = SourceArguments::default();
    let mut index = 0;
    while index < args.len() {
        match args[index].to_str() {
            Some("--mirror") => {
                parsed
                    .mirrors
                    .push(path_value(args, &mut index, "--mirror")?);
            }
            Some(flag) if flag.starts_with('-') => {
                return Err(Failure::usage(format!("unknown option `{flag}`")));
            }
            _ if parsed.source.is_none() => {
                parsed.source = Some(PathBuf::from(&args[index]));
            }
            _ => {
                return Err(Failure::usage(format!(
                    "unexpected argument `{}`",
                    args[index].to_string_lossy()
                )));
            }
        }
        index += 1;
    }
    Ok(parsed)
}

fn emit_command(args: &[OsString]) -> Result<u8, Failure> {
    let mut source = None;
    let mut mirrors = Vec::new();
    let mut output = None;
    let mut write_entry = true;
    let mut index = 0;
    while index < args.len() {
        match args[index].to_str() {
            Some("--mirror") => mirrors.push(path_value(args, &mut index, "--mirror")?),
            Some("-o") => set_once(&mut output, path_value(args, &mut index, "-o")?, "-o")?,
            Some("--no-entry") => write_entry = false,
            Some(flag) if flag.starts_with('-') => {
                return Err(Failure::usage(format!("unknown option `{flag}`")));
            }
            _ if source.is_none() => source = Some(PathBuf::from(&args[index])),
            _ => {
                return Err(Failure::usage(format!(
                    "unexpected argument `{}`",
                    args[index].to_string_lossy()
                )));
            }
        }
        index += 1;
    }
    let source = source.ok_or_else(|| Failure::usage("emit requires <file.ts>"))?;
    let output = output.ok_or_else(|| Failure::usage("emit requires -o <dir>"))?;
    let files = load_program(&source, &mirrors)?;
    emit_c_files(&files, &output, "program", write_entry)
        .map(|_| SUCCESS)
        .map_err(map_emit_error)
}

#[derive(Debug)]
struct LinkArguments {
    style: CCompilerStyle,
    runtime: RuntimeOverrides,
}

fn link_flags_command<O: Write>(args: &[OsString], stdout: &mut O) -> Result<u8, Failure> {
    let parsed = parse_link_arguments(args)?;
    let current = std::env::current_dir()
        .map_err(|error| Failure::usage(format!("read current directory: {error}")))?;
    let runtime = resolve_runtime_paths(parsed.runtime, RuntimeEnvironment::current(), &current)
        .map_err(Failure::usage)?;
    writeln!(
        stdout,
        "{}",
        include_directory_arg(parsed.style, &runtime.include).to_string_lossy()
    )
    .and_then(|_| writeln!(stdout, "{}", runtime.library.display()))
    .map_err(|error| Failure::usage(format!("write link flags: {error}")))?;
    for library in runtime_system_libraries(parsed.style) {
        writeln!(stdout, "{}", library.to_string_lossy())
            .map_err(|error| Failure::usage(format!("write link flags: {error}")))?;
    }
    Ok(SUCCESS)
}

fn parse_link_arguments(args: &[OsString]) -> Result<LinkArguments, Failure> {
    let mut style = CCompilerStyle::Unix;
    let mut style_set = false;
    let mut runtime = RuntimeOverrides::default();
    let mut index = 0;
    while index < args.len() {
        match args[index].to_str() {
            Some("--cc") => {
                if style_set {
                    return Err(Failure::usage("--cc may be supplied only once"));
                }
                let value = string_value(args, &mut index, "--cc")?;
                style = parse_style(value)?;
                style_set = true;
            }
            Some("--runtime-lib") => {
                let value = path_value(args, &mut index, "--runtime-lib")?;
                set_once(&mut runtime.library, value, "--runtime-lib")?;
            }
            Some("--runtime-include") => {
                let value = path_value(args, &mut index, "--runtime-include")?;
                set_once(&mut runtime.include, value, "--runtime-include")?;
            }
            Some(flag) => return Err(Failure::usage(format!("unknown option `{flag}`"))),
            None => {
                return Err(Failure::usage(format!(
                    "invalid non-Unicode option `{}`",
                    args[index].to_string_lossy()
                )));
            }
        }
        index += 1;
    }
    Ok(LinkArguments { style, runtime })
}

fn parse_style(value: &str) -> Result<CCompilerStyle, Failure> {
    match value {
        "unix" => Ok(CCompilerStyle::Unix),
        "msvc" => Ok(CCompilerStyle::Msvc),
        _ => Err(Failure::usage(format!(
            "unknown compiler style `{value}`; expected unix or msvc"
        ))),
    }
}

#[derive(Debug, Default)]
struct BuildArguments {
    source: Option<PathBuf>,
    mirrors: Vec<PathBuf>,
    hosts: Vec<PathBuf>,
    output: Option<PathBuf>,
    run: bool,
    runtime: RuntimeOverrides,
}

fn build_command<O: Write, E: Write>(
    args: &[OsString],
    stdout: &mut O,
    stderr: &mut E,
) -> Result<u8, Failure> {
    let parsed = parse_build_arguments(args)?;
    let current = std::env::current_dir()
        .map_err(|error| Failure::usage(format!("read current directory: {error}")))?;
    let source = absolute(
        parsed
            .source
            .as_ref()
            .ok_or_else(|| Failure::usage("build requires --source <file.ts>"))?,
        &current,
    );
    let mirrors = parsed
        .mirrors
        .iter()
        .map(|path| absolute(path, &current))
        .collect::<Vec<_>>();
    let hosts = parsed
        .hosts
        .iter()
        .map(|path| absolute(path, &current))
        .collect::<Vec<_>>();
    let output = parsed.output.map_or_else(
        || source.parent().unwrap_or(&current).join("subscript-build"),
        |path| absolute(&path, &current),
    );
    let runtime = resolve_runtime_paths(parsed.runtime, RuntimeEnvironment::current(), &current)
        .map_err(Failure::usage)?;
    let files = load_program(&source, &mirrors)?;
    let emitted =
        emit_c_files(&files, &output, "program", hosts.is_empty()).map_err(map_emit_error)?;
    let executable = executable_path(&output, &source)?;
    compile_build(
        &emitted.source,
        emitted.entry.as_deref(),
        &mirrors,
        &hosts,
        &runtime,
        &executable,
        stderr,
    )?;
    if parsed.run {
        run_executable(&executable, stdout, stderr)
    } else {
        Ok(SUCCESS)
    }
}

fn parse_build_arguments(args: &[OsString]) -> Result<BuildArguments, Failure> {
    let mut parsed = BuildArguments::default();
    let mut index = 0;
    while index < args.len() {
        match args[index].to_str() {
            Some("--source") => {
                let value = path_value(args, &mut index, "--source")?;
                set_once(&mut parsed.source, value, "--source")?;
            }
            Some("--mirror") => parsed
                .mirrors
                .push(path_value(args, &mut index, "--mirror")?),
            Some("--host") => parsed.hosts.push(path_value(args, &mut index, "--host")?),
            Some("-o") => {
                let value = path_value(args, &mut index, "-o")?;
                set_once(&mut parsed.output, value, "-o")?;
            }
            Some("--run") if !parsed.run => parsed.run = true,
            Some("--run") => return Err(Failure::usage("--run may be supplied only once")),
            Some("--runtime-lib") => {
                let value = path_value(args, &mut index, "--runtime-lib")?;
                set_once(&mut parsed.runtime.library, value, "--runtime-lib")?;
            }
            Some("--runtime-include") => {
                let value = path_value(args, &mut index, "--runtime-include")?;
                set_once(&mut parsed.runtime.include, value, "--runtime-include")?;
            }
            Some(flag) => return Err(Failure::usage(format!("unknown option `{flag}`"))),
            None => {
                return Err(Failure::usage(format!(
                    "invalid non-Unicode option `{}`",
                    args[index].to_string_lossy()
                )));
            }
        }
        index += 1;
    }
    Ok(parsed)
}

fn compile_build<E: Write>(
    program: &Path,
    entry: Option<&Path>,
    mirrors: &[PathBuf],
    hosts: &[PathBuf],
    runtime: &RuntimePaths,
    executable: &Path,
    stderr: &mut E,
) -> Result<(), Failure> {
    let compiler = host_c_compiler().map_err(|error| Failure::usage(error.to_string()))?;
    let style = compiler.style();
    let mut command = compiler.command();
    add_c11_optimized_flags(&mut command, style);
    let output_directory = executable
        .parent()
        .ok_or_else(|| Failure::usage("build output has no parent directory"))?;
    add_object_directory(&mut command, output_directory, style);

    let mut includes = Vec::new();
    for path in mirrors.iter().chain(hosts.iter()) {
        if let Some(directory) = path.parent() {
            push_unique(&mut includes, directory.to_path_buf());
        }
    }
    push_unique(&mut includes, runtime.include.clone());
    for directory in includes {
        command.arg(include_directory_arg(style, &directory));
    }
    command.arg(program);
    if let Some(path) = entry {
        command.arg(path);
    }
    command
        .args(hosts)
        .arg(&runtime.library)
        .args(runtime_system_libraries(style));
    add_executable_output(&mut command, executable, style);

    let output = command.output().map_err(|error| {
        Failure::usage(format!(
            "the platform C compiler `{}` could not be run: {error}",
            compiler.program().to_string_lossy()
        ))
    })?;
    stderr
        .write_all(&output.stdout)
        .and_then(|_| stderr.write_all(&output.stderr))
        .map_err(|error| Failure::usage(format!("write compiler output: {error}")))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(Failure::usage(format!(
            "compiling/linking the emitted C failed with {}",
            output.status
        )))
    }
}

fn run_executable<O: Write, E: Write>(
    executable: &Path,
    stdout: &mut O,
    stderr: &mut E,
) -> Result<u8, Failure> {
    let output = Command::new(executable)
        .output()
        .map_err(|error| Failure::usage(format!("run {}: {error}", executable.display())))?;
    stdout
        .write_all(&output.stdout)
        .map_err(|error| Failure::usage(format!("write program stdout: {error}")))?;
    stderr
        .write_all(&output.stderr)
        .map_err(|error| Failure::usage(format!("write program stderr: {error}")))?;
    Ok(status_code(output.status))
}

fn status_code(status: std::process::ExitStatus) -> u8 {
    status
        .code()
        .and_then(|code| u8::try_from(code).ok())
        .unwrap_or(PROGRAM_ERROR)
}

fn executable_path(output: &Path, source: &Path) -> Result<PathBuf, Failure> {
    let stem = source
        .file_stem()
        .filter(|stem| !stem.is_empty())
        .ok_or_else(|| Failure::usage(format!("source {} has no file stem", source.display())))?;
    let mut name = stem.to_os_string();
    name.push(std::env::consts::EXE_SUFFIX);
    Ok(output.join(name))
}

fn run_command<O: Write>(args: &[OsString], stdout: &mut O) -> Result<u8, Failure> {
    if args.len() != 1 {
        return Err(Failure::usage("run requires exactly one <file.ts>"));
    }
    let source = PathBuf::from(&args[0]);
    let files = load_program(&source, &[])?;
    match run_jit(&files) {
        Ok(output) => {
            stdout
                .write_all(&output)
                .map_err(|error| Failure::usage(format!("write program stdout: {error}")))?;
            Ok(SUCCESS)
        }
        Err(RunError::Rejected(diagnostics)) => Err(rejection(diagnostics)),
        Err(RunError::Trap(report)) => Err(Failure::program(report.to_string())),
        Err(RunError::UnresolvedForeignSymbol(symbol)) => Err(Failure::usage(format!(
            "run supports only programs without host C bindings; unresolved symbol `{symbol}`"
        ))),
        Err(RunError::Internal(message)) => Err(Failure::usage(message)),
        Err(other) => Err(Failure::usage(other.to_string())),
    }
}

fn load_program(source: &Path, mirrors: &[PathBuf]) -> Result<Vec<SourceFile>, Failure> {
    let mut files = Vec::with_capacity(mirrors.len() + 1);
    for path in mirrors {
        let text = read_text(path, "mirror")?;
        files.push(SourceFile::ambient(path.to_string_lossy(), text));
    }
    let text = read_text(source, "source")?;
    files.push(SourceFile::new(source.to_string_lossy(), text));
    Ok(files)
}

fn read_text(path: &Path, kind: &str) -> Result<String, Failure> {
    std::fs::read_to_string(path)
        .map_err(|error| Failure::usage(format!("read {kind} {}: {error}", path.display())))
}

fn rejection(diagnostics: Vec<Diagnostic>) -> Failure {
    let message = diagnostics
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    Failure::program(message)
}

fn map_emit_error(error: EmitCFilesError) -> Failure {
    match error {
        EmitCFilesError::Diagnostics(diagnostics) => rejection(diagnostics),
        EmitCFilesError::Emission(message) => Failure::program(message),
        other => Failure::usage(other.to_string()),
    }
}

fn path_value(args: &[OsString], index: &mut usize, option: &str) -> Result<PathBuf, Failure> {
    *index += 1;
    args.get(*index)
        .map(PathBuf::from)
        .ok_or_else(|| Failure::usage(format!("{option} requires a path")))
}

fn string_value<'a>(
    args: &'a [OsString],
    index: &mut usize,
    option: &str,
) -> Result<&'a str, Failure> {
    *index += 1;
    args.get(*index)
        .ok_or_else(|| Failure::usage(format!("{option} requires a value")))?
        .to_str()
        .ok_or_else(|| Failure::usage(format!("{option} requires a Unicode value")))
}

fn set_once<T>(slot: &mut Option<T>, value: T, option: &str) -> Result<(), Failure> {
    if slot.is_some() {
        Err(Failure::usage(format!(
            "{option} may be supplied only once"
        )))
    } else {
        *slot = Some(value);
        Ok(())
    }
}

fn absolute(path: &Path, current: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        current.join(path)
    }
}

fn push_unique(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths.contains(&path) {
        paths.push(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct TestFile(PathBuf);

    impl TestFile {
        fn program(source: &str) -> Result<Self, String> {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "subscript-cli-execute-{}-{}.ts",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::write(&path, source)
                .map_err(|error| format!("write {}: {error}", path.display()))?;
            Ok(Self(path))
        }
    }

    impl Drop for TestFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    fn os_args(values: impl IntoIterator<Item = impl AsRef<OsStr>>) -> Vec<OsString> {
        values
            .into_iter()
            .map(|value| value.as_ref().to_os_string())
            .collect()
    }

    #[test]
    fn public_execute_runs_clean_check_and_program_error_paths() -> Result<(), String> {
        let clean = TestFile::program("export function main(): void {}\n")?;
        let bad = TestFile::program("const value: number = 1;\n")?;
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        assert_eq!(
            execute(
                os_args([OsStr::new("check"), clean.0.as_os_str()]),
                &mut stdout,
                &mut stderr
            ),
            SUCCESS
        );
        assert!(stdout.is_empty());
        assert!(stderr.is_empty());

        assert_eq!(
            execute(
                os_args([OsStr::new("check"), bad.0.as_os_str()]),
                &mut stdout,
                &mut stderr
            ),
            PROGRAM_ERROR
        );
        assert!(String::from_utf8_lossy(&stderr).contains("S007"));
        Ok(())
    }

    #[test]
    fn public_execute_reports_usage_errors_without_panicking() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        assert_eq!(
            execute(Vec::<OsString>::new(), &mut stdout, &mut stderr),
            USAGE_ERROR
        );
        assert!(stdout.is_empty());
        assert!(String::from_utf8_lossy(&stderr).contains("usage:"));
    }
}
