#![warn(missing_docs)]
//! Implementation of the `subscript` developer command.

mod program_loader;
mod runtime_paths;
/// Testable state transitions for `run --watch`.
pub mod watch;

use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

use program_loader::load_program;
use runtime_paths::{resolve_runtime_paths, RuntimeEnvironment, RuntimeOverrides, RuntimePaths};
use subscript_codegen::{
    add_c11_optimized_flags, add_executable_output, add_object_directory,
    emit_c_files_with_options, host_c_compiler, include_directory_arg, run_jit_configured,
    runtime_system_libraries, CCompilerStyle, EmitCFilesError, RunError,
};
use subscript_compiler::{
    check_warnings, render_diagnostics, render_warnings, Diagnostic, SourceFile, Warning,
};
use watch::{WatchCall, WatchOutcome, WatchSession, WatchStep};

const SUCCESS: u8 = 0;
const PROGRAM_ERROR: u8 = 1;
const USAGE_ERROR: u8 = 2;

#[derive(Debug)]
struct Failure {
    code: u8,
    message: String,
    verbatim: bool,
}

impl Failure {
    fn program(message: impl Into<String>) -> Self {
        Self {
            code: PROGRAM_ERROR,
            message: message.into(),
            verbatim: false,
        }
    }

    fn usage(message: impl Into<String>) -> Self {
        Self {
            code: USAGE_ERROR,
            message: message.into(),
            verbatim: false,
        }
    }

    fn rejection(message: impl Into<String>) -> Self {
        Self {
            code: PROGRAM_ERROR,
            message: message.into(),
            verbatim: true,
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
            if failure.verbatim {
                let _ = writeln!(stderr, "{}", failure.message);
            } else {
                let _ = writeln!(stderr, "subscript: {}", failure.message);
            }
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
        "check" => check_command(&args[1..], stderr),
        "boundary" => boundary_command(&args[1..], stdout, stderr),
        "emit" => emit_command(&args[1..], stderr),
        "bind" => bind_command(&args[1..], stdout),
        "link-flags" => link_flags_command(&args[1..], stdout),
        "build" => build_command(&args[1..], stdout, stderr),
        "run" => run_command(&args[1..], stdout, stderr),
        _ => Err(Failure::usage(format!(
            "unknown subcommand `{command}`; {}",
            usage()
        ))),
    }
}

fn usage() -> &'static str {
    "usage: subscript <check|boundary|emit|bind|link-flags|build|run> ..."
}

#[derive(Debug, Default)]
struct SourceArguments {
    enabled_modules: Vec<String>,
    source: Option<PathBuf>,
    mirrors: Vec<PathBuf>,
    deny_warnings: bool,
}

fn check_command<E: Write>(args: &[OsString], stderr: &mut E) -> Result<u8, Failure> {
    let parsed = parse_source_arguments(args)?;
    let source = parsed
        .source
        .as_ref()
        .ok_or_else(|| Failure::usage("check requires <file.ts>"))?;
    let (files, warnings) = load_and_check(source, &parsed.mirrors, &parsed.enabled_modules)?;
    if warnings.is_empty() {
        writeln!(stderr, "check: {}: no errors", source.to_string_lossy())
            .map_err(|error| Failure::usage(format!("write check result: {error}")))?;
        return Ok(SUCCESS);
    }
    write_warnings(&files, &warnings, stderr)?;
    Ok(if parsed.deny_warnings {
        PROGRAM_ERROR
    } else {
        SUCCESS
    })
}

/// `subscript boundary` (`specs/blocks/compiler.md` §189): checks the
/// program as `check` does, lowers it, and prints the crossing plan of each
/// foreign call site.
fn boundary_command<O: Write, E: Write>(
    args: &[OsString],
    stdout: &mut O,
    stderr: &mut E,
) -> Result<u8, Failure> {
    let parsed = parse_source_arguments(args)?;
    let source = parsed
        .source
        .as_ref()
        .ok_or_else(|| Failure::usage("boundary requires <file.ts>"))?;
    let files = load_program(source, &parsed.mirrors)?;
    let module =
        subscript_compiler::check_program_with(&files, &module_options(&parsed.enabled_modules))
            .map_err(|diagnostics| rejection(&files, diagnostics))?;
    let warnings = check_warnings(&module);
    if !warnings.is_empty() {
        write_warnings(&files, &warnings, stderr)?;
        if parsed.deny_warnings {
            return Ok(PROGRAM_ERROR);
        }
    }
    let lowered = subscript_codegen::lir::lower_module(&module).map_err(|error| {
        Failure::program(format!("internal error: LIR construction failed: {error}"))
    })?;
    let lines = subscript_compiler::crossing::report(&lowered).map_err(Failure::program)?;
    for line in lines {
        writeln!(stdout, "{line}")
            .map_err(|error| Failure::usage(format!("write boundary report: {error}")))?;
    }
    Ok(SUCCESS)
}

fn parse_source_arguments(args: &[OsString]) -> Result<SourceArguments, Failure> {
    let mut parsed = SourceArguments::default();
    let mut index = 0;
    while index < args.len() {
        match args[index].to_str() {
            Some("--deny-warnings") if !parsed.deny_warnings => {
                parsed.deny_warnings = true;
            }
            Some("--deny-warnings") => {
                return Err(Failure::usage("--deny-warnings may be supplied only once"));
            }
            Some("--enable-module") => parsed.enabled_modules.push(module_value(args, &mut index)?),
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

fn emit_command<E: Write>(args: &[OsString], stderr: &mut E) -> Result<u8, Failure> {
    let mut source = None;
    let mut mirrors = Vec::new();
    let mut enabled_modules = Vec::new();
    let mut output = None;
    let mut write_entry = true;
    let mut deny_warnings = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].to_str() {
            Some("--deny-warnings") if !deny_warnings => deny_warnings = true,
            Some("--deny-warnings") => {
                return Err(Failure::usage("--deny-warnings may be supplied only once"));
            }
            Some("--enable-module") => enabled_modules.push(module_value(args, &mut index)?),
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
    let (files, warnings) = load_and_check(&source, &mirrors, &enabled_modules)?;
    if !warnings.is_empty() {
        write_warnings(&files, &warnings, stderr)?;
        if deny_warnings {
            return Ok(PROGRAM_ERROR);
        }
    }
    emit_c_files_with_options(
        &files,
        &output,
        "program",
        write_entry,
        &module_options(&enabled_modules),
    )
    .map(|_| SUCCESS)
    .map_err(|error| map_emit_error(error, &files))
}

#[derive(Debug, Default)]
struct BindArguments {
    header: Option<PathBuf>,
    output: Option<PathBuf>,
    /// Aggregates named by `--explicit-callback-lifetime`, in the order
    /// the command line gives them (cli.md §10.1, compiler.md §111 rule 1).
    explicit_callback_lifetimes: Vec<String>,
    completions: Vec<(String, String)>,
}

fn bind_command<O: Write>(args: &[OsString], stdout: &mut O) -> Result<u8, Failure> {
    let parsed = parse_bind_arguments(args)?;
    let header = parsed
        .header
        .ok_or_else(|| Failure::usage("bind requires --header <file.h> or <file.h>"))?;
    let source = read_text(&header, "header")?;
    let include_spelling = header
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            Failure::usage(format!(
                "header path `{}` has no UTF-8 basename",
                header.display()
            ))
        })?;
    // cli.md §10.1: a rejected selection is a program-input failure, and
    // the generator runs before any output path is opened, so a failed run
    // leaves no mirror behind.
    let mut options = subscript_bindgen::BindOptions::new();
    for aggregate in parsed.explicit_callback_lifetimes {
        options = options.with_explicit_callback_lifetime(aggregate);
    }
    for (function, result) in parsed.completions {
        options = options.with_completion(function, result);
    }
    let mirror = subscript_bindgen::generate_with_options(&source, include_spelling, &options)
        .map_err(|error| Failure::program(error.to_string()))?;

    if let Some(output) = parsed.output {
        std::fs::write(&output, mirror)
            .map_err(|error| Failure::usage(format!("write {}: {error}", output.display())))?;
    } else {
        stdout
            .write_all(mirror.as_bytes())
            .map_err(|error| Failure::usage(format!("write bind output: {error}")))?;
    }
    Ok(SUCCESS)
}

fn parse_bind_arguments(args: &[OsString]) -> Result<BindArguments, Failure> {
    let mut parsed = BindArguments::default();
    let mut index = 0;
    while index < args.len() {
        match args[index].to_str() {
            Some("--header") => {
                let value = path_value(args, &mut index, "--header")?;
                set_once(&mut parsed.header, value, "--header")?;
            }
            Some("-o") => {
                let value = path_value(args, &mut index, "-o")?;
                set_once(&mut parsed.output, value, "-o")?;
            }
            Some("--completion") => {
                let value = string_value(args, &mut index, "--completion")?;
                let (function, result) = value
                    .split_once('=')
                    .filter(|(function, result)| !function.is_empty() && !result.is_empty())
                    .ok_or_else(|| Failure::usage("--completion requires <function>=<result>"))?;
                parsed
                    .completions
                    .push((function.to_string(), result.to_string()));
            }
            Some("--explicit-callback-lifetime") => {
                let value = string_value(args, &mut index, "--explicit-callback-lifetime")?;
                parsed.explicit_callback_lifetimes.push(value.to_string());
            }
            Some(flag) if flag.starts_with('-') => {
                return Err(Failure::usage(format!("unknown option `{flag}`")));
            }
            _ if parsed.header.is_none() => parsed.header = Some(PathBuf::from(&args[index])),
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
    enabled_modules: Vec<String>,
    source: Option<PathBuf>,
    mirrors: Vec<PathBuf>,
    hosts: Vec<PathBuf>,
    output: Option<PathBuf>,
    run: bool,
    deny_warnings: bool,
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
    let source_given = parsed
        .source
        .clone()
        .ok_or_else(|| Failure::usage("build requires --source <file.ts>"))?;
    let source = absolute(&source_given, &current);
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
    let (files, warnings) =
        load_and_check(&source_given, &parsed.mirrors, &parsed.enabled_modules)?;
    if !warnings.is_empty() {
        write_warnings(&files, &warnings, stderr)?;
        if parsed.deny_warnings {
            return Ok(PROGRAM_ERROR);
        }
    }
    if hosts.is_empty() {
        let module = subscript_compiler::check_program_with(
            &files,
            &module_options(&parsed.enabled_modules),
        )
        .map_err(|diagnostics| rejection(&files, diagnostics))?;
        module
            .runner_main()
            .map_err(|diagnostic| rejection(&files, vec![diagnostic]))?;
    }
    let runtime = resolve_runtime_paths(parsed.runtime, RuntimeEnvironment::current(), &current)
        .map_err(Failure::usage)?;
    let emitted = emit_c_files_with_options(
        &files,
        &output,
        "program",
        hosts.is_empty(),
        &module_options(&parsed.enabled_modules),
    )
    .map_err(|error| map_emit_error(error, &files))?;
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
            Some("--enable-module") => parsed.enabled_modules.push(module_value(args, &mut index)?),
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
            Some("--deny-warnings") if !parsed.deny_warnings => {
                parsed.deny_warnings = true;
            }
            Some("--deny-warnings") => {
                return Err(Failure::usage("--deny-warnings may be supplied only once"));
            }
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
    push_unique(&mut includes, output_directory.to_path_buf());
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
    if output.status.success() {
        Ok(())
    } else {
        stderr
            .write_all(&output.stdout)
            .and_then(|_| stderr.write_all(&output.stderr))
            .map_err(|error| Failure::usage(format!("write compiler output: {error}")))?;
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

fn run_command<O: Write, E: Write>(
    args: &[OsString],
    stdout: &mut O,
    stderr: &mut E,
) -> Result<u8, Failure> {
    let mut source = None;
    let mut deny_warnings = false;
    let mut watch = false;
    let mut enabled_modules = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        match arg.to_str() {
            Some("--deny-warnings") if !deny_warnings => deny_warnings = true,
            Some("--deny-warnings") => {
                return Err(Failure::usage("--deny-warnings may be supplied only once"));
            }
            Some("--enable-module") => enabled_modules.push(module_value(args, &mut index)?),
            Some("--watch") if !watch => watch = true,
            Some("--watch") => {
                return Err(Failure::usage("--watch may be supplied only once"));
            }
            Some(flag) if flag.starts_with('-') => {
                return Err(Failure::usage(format!("unknown option `{flag}`")));
            }
            _ if source.is_none() => source = Some(PathBuf::from(arg)),
            _ => return Err(Failure::usage("run requires exactly one <file.ts>")),
        }
        index += 1;
    }
    let source = source.ok_or_else(|| Failure::usage("run requires exactly one <file.ts>"))?;
    if watch {
        return run_watch(&source, deny_warnings, enabled_modules, stdout, stderr);
    }
    let (files, warnings) = load_and_check(&source, &[], &enabled_modules)?;
    if !warnings.is_empty() {
        write_warnings(&files, &warnings, stderr)?;
        if deny_warnings {
            return Ok(PROGRAM_ERROR);
        }
    }
    let modules: Vec<_> = enabled_modules.iter().map(String::as_str).collect();
    match run_jit_configured(
        &files,
        subscript_codegen::RunConfig::default().with_enabled_modules(&modules),
    ) {
        Ok(output) => {
            stdout
                .write_all(&output.stdout)
                .map_err(|error| Failure::usage(format!("write program stdout: {error}")))?;
            Ok(SUCCESS)
        }
        Err(RunError::Rejected(diagnostics)) => Err(rejection(&files, diagnostics)),
        Err(RunError::Trap(report)) => {
            let _ = stdout
                .write_all(&report.stdout)
                .and_then(|_| stdout.flush());
            Err(Failure::program(report.to_string()))
        }
        Err(RunError::UnresolvedForeignSymbol(symbol)) => Err(Failure::usage(format!(
            "run supports only programs without host C bindings; unresolved symbol `{symbol}`"
        ))),
        Err(RunError::Internal(message)) => Err(Failure::usage(message)),
        Err(other) => Err(Failure::usage(other.to_string())),
    }
}

const WATCH_POLL_INTERVAL: Duration = Duration::from_millis(150);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileStamp {
    Present {
        modified: Option<SystemTime>,
        len: u64,
    },
    Missing,
    /// No stamp precedes the load that read this file, so the next poll
    /// reloads it once.
    Unseen,
}

fn file_stamp(path: &Path) -> FileStamp {
    match std::fs::metadata(path) {
        Ok(metadata) => FileStamp::Present {
            modified: metadata.modified().ok(),
            len: metadata.len(),
        },
        Err(_) => FileStamp::Missing,
    }
}

#[derive(Debug)]
struct WatchedFiles {
    paths: Vec<PathBuf>,
    stamps: Vec<FileStamp>,
}

impl WatchedFiles {
    fn new(paths: Vec<PathBuf>) -> Self {
        let stamps = paths.iter().map(|path| file_stamp(path)).collect();
        Self { paths, stamps }
    }

    fn changed(&self) -> bool {
        self.paths
            .iter()
            .zip(&self.stamps)
            .any(|(path, stamp)| file_stamp(path) != *stamp)
    }

    fn refresh(&mut self) {
        self.stamps = self.paths.iter().map(|path| file_stamp(path)).collect();
    }

    /// Watches `paths` with the stamps taken before the load that read
    /// them. A write after that load then differs from its stamp.
    fn after_load(&self, paths: Vec<PathBuf>) -> Self {
        let stamps = paths
            .iter()
            .map(|path| {
                self.paths
                    .iter()
                    .zip(&self.stamps)
                    .find(|(known, _)| *known == path)
                    .map_or(FileStamp::Unseen, |(_, stamp)| *stamp)
            })
            .collect();
        Self { paths, stamps }
    }
}

fn loaded_file_paths(entry: &Path, files: &[SourceFile]) -> Result<Vec<PathBuf>, Failure> {
    let entry = std::fs::canonicalize(entry)
        .map_err(|error| Failure::usage(format!("resolve source {}: {error}", entry.display())))?;
    let directory = entry.parent().map(Path::to_path_buf).ok_or_else(|| {
        Failure::usage(format!(
            "source {} has no parent directory",
            entry.display()
        ))
    })?;
    let mut paths = vec![entry];
    for file in files.iter().skip(1) {
        let candidate = directory.join(&file.name);
        let path = std::fs::canonicalize(&candidate).map_err(|error| {
            Failure::usage(format!("resolve source {}: {error}", candidate.display()))
        })?;
        push_unique(&mut paths, path);
    }
    Ok(paths)
}

/// The files that the first load read, with the stamps taken before
/// that load, and the load result. A program error keeps the entry
/// watched; any other load failure ends the watch.
type InitialWatch = (WatchedFiles, Result<Vec<SourceFile>, Failure>);

fn initial_watch_load(
    source: &Path,
    load: impl FnOnce(&Path) -> Result<Vec<SourceFile>, Failure>,
) -> Result<InitialWatch, Failure> {
    let before_load = WatchedFiles::new(std::fs::canonicalize(source).into_iter().collect());
    match load(source) {
        Ok(files) => {
            let paths = loaded_file_paths(source, &files)?;
            Ok((before_load.after_load(paths), Ok(files)))
        }
        Err(failure) if failure.code == PROGRAM_ERROR => {
            // A parser diagnostic can be raised while the loader is
            // discovering imports, before it can return a SourceFile set.
            // Keep watching the entry so the first parseable edit can
            // re-derive the complete loaded file set.
            let entry = std::fs::canonicalize(source).map_err(|error| {
                Failure::usage(format!("resolve source {}: {error}", source.display()))
            })?;
            Ok((before_load.after_load(vec![entry]), Err(failure)))
        }
        Err(failure) => Err(failure),
    }
}

fn run_watch<O: Write, E: Write>(
    source: &Path,
    deny_warnings: bool,
    enabled_modules: Vec<String>,
    stdout: &mut O,
    stderr: &mut E,
) -> Result<u8, Failure> {
    let mut session = WatchSession::new_with_modules(deny_warnings, enabled_modules);
    let (mut watched, initial) = initial_watch_load(source, |path| load_program(path, &[]))?;
    match initial {
        Ok(initial_files) => {
            let initial = session.step(&initial_files);
            if let WatchOutcome::Failed { message } = &initial.outcome {
                if !initial.warnings.is_empty() {
                    write_warnings(&initial_files, &initial.warnings, stderr)?;
                }
                return Err(Failure::usage(message.clone()));
            }
            write_watch_step(&initial_files, initial, stdout, stderr)?;
        }
        Err(failure) => {
            session.invalidate_loaded_sources();
            write_watch_failure(&failure, stderr)?;
        }
    }

    loop {
        std::thread::sleep(WATCH_POLL_INTERVAL);
        if !watched.changed() {
            continue;
        }
        watched.refresh();

        let files = match load_program(source, &[]) {
            Ok(files) => files,
            Err(failure) => {
                session.invalidate_loaded_sources();
                write_watch_failure(&failure, stderr)?;
                continue;
            }
        };
        let paths = match loaded_file_paths(source, &files) {
            Ok(paths) => paths,
            Err(failure) => {
                session.invalidate_loaded_sources();
                write_watch_failure(&failure, stderr)?;
                continue;
            }
        };
        watched = watched.after_load(paths);
        let step = session.step(&files);
        write_watch_step(&files, step, stdout, stderr)?;
    }
}

fn write_watch_failure<E: Write>(failure: &Failure, stderr: &mut E) -> Result<(), Failure> {
    if failure.verbatim {
        writeln!(stderr, "{}", failure.message)
    } else {
        writeln!(stderr, "subscript: {}", failure.message)
    }
    .and_then(|_| writeln!(stderr, "watch: waiting for a fix"))
    .and_then(|_| stderr.flush())
    .map_err(|error| Failure::usage(format!("write watch status: {error}")))
}

fn write_watch_step<O: Write, E: Write>(
    files: &[SourceFile],
    step: WatchStep,
    stdout: &mut O,
    stderr: &mut E,
) -> Result<(), Failure> {
    if !step.warnings.is_empty() {
        write_warnings(files, &step.warnings, stderr)?;
    }
    if !step.diagnostics.is_empty() {
        writeln!(stderr, "{}", render_diagnostics(files, &step.diagnostics))
            .map_err(|error| Failure::usage(format!("write diagnostics: {error}")))?;
    }

    match step.outcome {
        WatchOutcome::Unchanged => {}
        WatchOutcome::Started(call) => {
            writeln!(stderr, "watch: started")
                .map_err(|error| Failure::usage(format!("write watch status: {error}")))?;
            write_watch_call(call, stdout, stderr)?;
        }
        WatchOutcome::Swapped(call) => {
            writeln!(stderr, "watch: swapped")
                .map_err(|error| Failure::usage(format!("write watch status: {error}")))?;
            write_watch_call(call, stdout, stderr)?;
        }
        WatchOutcome::WaitingForFix => {
            writeln!(stderr, "watch: waiting for a fix")
                .map_err(|error| Failure::usage(format!("write watch status: {error}")))?;
        }
        WatchOutcome::Refused { declaration } => {
            writeln!(stderr, "watch: refused: {declaration}")
                .map_err(|error| Failure::usage(format!("write watch status: {error}")))?;
        }
        WatchOutcome::Failed { message } => {
            writeln!(stderr, "watch: error: {message}")
                .map_err(|error| Failure::usage(format!("write watch status: {error}")))?;
        }
    }
    stderr
        .flush()
        .map_err(|error| Failure::usage(format!("flush watch status: {error}")))
}

fn write_watch_call<O: Write, E: Write>(
    call: WatchCall,
    stdout: &mut O,
    stderr: &mut E,
) -> Result<(), Failure> {
    stdout
        .write_all(call.trap.as_ref().map_or(&call.output, |trap| &trap.stdout))
        .and_then(|_| stdout.flush())
        .map_err(|error| Failure::usage(format!("write program stdout: {error}")))?;
    if let Some(trap) = call.trap {
        writeln!(stderr, "subscript: {trap}")
            .map_err(|error| Failure::usage(format!("write trap: {error}")))?;
    }
    Ok(())
}

fn read_text(path: &Path, kind: &str) -> Result<String, Failure> {
    std::fs::read_to_string(path)
        .map_err(|error| Failure::usage(format!("read {kind} {}: {error}", path.display())))
}

fn rejection(files: &[SourceFile], diagnostics: Vec<Diagnostic>) -> Failure {
    Failure::rejection(render_diagnostics(files, &diagnostics))
}

fn accepted_warnings(
    files: &[SourceFile],
    enabled_modules: &[String],
) -> Result<Vec<Warning>, Failure> {
    match subscript_compiler::check_program_with(files, &module_options(enabled_modules)) {
        Ok(module) => Ok(check_warnings(&module)),
        Err(diagnostics) => Err(rejection(files, diagnostics)),
    }
}

/// Loads one program from disk and checks it.
fn load_and_check(
    source: &Path,
    mirrors: &[PathBuf],
    enabled_modules: &[String],
) -> Result<(Vec<SourceFile>, Vec<Warning>), Failure> {
    let files = load_program(source, mirrors)?;
    let warnings = accepted_warnings(&files, enabled_modules)?;
    Ok((files, warnings))
}

fn write_warnings<E: Write>(
    files: &[SourceFile],
    warnings: &[Warning],
    stderr: &mut E,
) -> Result<(), Failure> {
    writeln!(stderr, "{}", render_warnings(files, warnings))
        .map_err(|error| Failure::usage(format!("write warnings: {error}")))
}

fn map_emit_error(error: EmitCFilesError, files: &[SourceFile]) -> Failure {
    match error {
        EmitCFilesError::Diagnostics(diagnostics) => rejection(files, diagnostics),
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

fn module_options(modules: &[String]) -> subscript_compiler::CheckOptions {
    let mut options = subscript_compiler::CheckOptions::default();
    options.enabled_modules = modules.to_vec();
    options
}
fn module_value(args: &[OsString], index: &mut usize) -> Result<String, Failure> {
    let value = path_value(args, index, "--enable-module")?;
    if value != Path::new("node:fs/promises") {
        return Err(Failure::usage(
            "the enabled standard module must be node:fs/promises",
        ));
    }
    Ok("node:fs/promises".into())
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
        assert_eq!(
            stderr,
            format!("check: {}: no errors\n", clean.0.to_string_lossy()).as_bytes()
        );
        stderr.clear();

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

    /// compiler.md §190.2 acceptance 3: an in-process `run` on a libtest
    /// thread starts this test binary again as the dev-tier runner.
    #[test]
    fn an_in_process_run_on_a_test_thread_reaches_the_runner() -> Result<(), String> {
        let program = TestFile::program("export function main(): void {\n  print(`runner`);\n}\n")?;
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = execute(
            os_args([OsStr::new("run"), program.0.as_os_str()]),
            &mut stdout,
            &mut stderr,
        );
        assert_eq!(code, SUCCESS, "{}", String::from_utf8_lossy(&stderr));
        assert_eq!(stdout, b"runner\n");
        Ok(())
    }

    #[test]
    fn watched_files_keep_the_stamp_taken_before_the_load() -> Result<(), String> {
        let known = TestFile::program("export function main(): void {}\n")?;
        let before_load = WatchedFiles::new(vec![known.0.clone()]);
        let unchanged = before_load.after_load(vec![known.0.clone()]);
        assert!(!unchanged.changed());

        // Only known paths: a write after the stamp and before the
        // watcher records the file set is a change.
        std::fs::write(&known.0, "export function main(): void { print(\"x\"); }\n")
            .map_err(|error| error.to_string())?;
        let mut watched = before_load.after_load(vec![known.0.clone()]);
        assert_eq!(watched.stamps, before_load.stamps);
        assert!(watched.changed());
        watched.refresh();
        assert!(!watched.changed());
        Ok(())
    }

    #[test]
    fn watched_files_mark_a_path_without_a_stamp_unseen() -> Result<(), String> {
        let known = TestFile::program("export function main(): void {}\n")?;
        let imported = TestFile::program("export function helper(): void {}\n")?;
        let before_load = WatchedFiles::new(vec![known.0.clone()]);
        let mut watched = before_load.after_load(vec![known.0.clone(), imported.0.clone()]);
        assert_eq!(watched.stamps[0], before_load.stamps[0]);
        assert_eq!(watched.stamps[1], FileStamp::Unseen);
        assert!(watched.changed());
        watched.refresh();
        assert!(!watched.changed());
        Ok(())
    }

    #[test]
    fn initial_watch_load_stamps_before_the_load() -> Result<(), String> {
        let entry = TestFile::program("export function main(): void {}\n")?;
        let (watched, initial) = initial_watch_load(&entry.0, |path| {
            std::fs::write(path, "export function main(): void { print(\"x\"); }\n")
                .map_err(|error| Failure::usage(error.to_string()))?;
            load_program(path, &[])
        })
        .map_err(|failure| failure.message)?;
        assert!(initial.is_ok());
        assert!(watched.changed());

        // A program error keeps the entry watched with the same stamp.
        let broken = TestFile::program("export function main(): void {}\n")?;
        let (watched, initial) = initial_watch_load(&broken.0, |path| {
            std::fs::write(path, "export function main(: void {\n")
                .map_err(|error| Failure::usage(error.to_string()))?;
            load_program(path, &[])
        })
        .map_err(|failure| failure.message)?;
        assert!(initial.is_err_and(|failure| failure.code == PROGRAM_ERROR));
        assert!(watched.changed());
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
