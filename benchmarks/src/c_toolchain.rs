//! C toolchain commands for the benchmark runners.

use std::path::Path;
use std::process::Command;
use subscript_codegen::{
    add_c11_optimized_flags, add_executable_output, add_object_directory, include_directory_arg,
    posix_feature_arguments, runtime_system_libraries, unreferenced_code_removal_arguments,
    HostCCompiler,
};

/// Builds a subject command with the resolved toolchain environment.
pub(super) fn subject_command(
    cc: &HostCCompiler,
    sources: &[&Path],
    executable: &Path,
    directory: &Path,
    staticlib: Option<&Path>,
) -> Command {
    configure(
        cc.command(),
        cc.style(),
        sources,
        executable,
        directory,
        staticlib,
    )
}

/// Configures a subject command with the selected driver style.
pub(super) fn configure(
    mut command: Command,
    style: subscript_codegen::CCompilerStyle,
    sources: &[&Path],
    executable: &Path,
    directory: &Path,
    staticlib: Option<&Path>,
) -> Command {
    if style.is_msvc() {
        add_c11_optimized_flags(&mut command, style);
        add_object_directory(&mut command, directory, style);
    } else {
        // Section 225 preserves the recorded Unix arguments and their order.
        if staticlib.is_some() {
            command.arg("-std=c11").args(posix_feature_arguments());
        }
        command.args(super::BASELINE_CFLAGS);
        if staticlib.is_some() {
            command.arg("-fwrapv");
        }
    }
    if style.is_msvc() && staticlib.is_some() {
        command.arg(include_directory_arg(style, directory));
    }
    command.args(sources);
    if let Some(library) = staticlib {
        command.arg(library).args(runtime_system_libraries(style));
        if !style.is_msvc() {
            command.args(unreferenced_code_removal_arguments(style));
        }
    }
    add_executable_output(&mut command, executable, style);
    if staticlib.is_some() && style.is_msvc() {
        command.args(unreferenced_code_removal_arguments(style));
    }
    command
}

/// Reports the compiler version and both subjects' arguments.
pub(super) fn description(cc: &HostCCompiler) -> String {
    let mut probe = cc.command();
    if !cc.style().is_msvc() {
        probe.arg("--version");
    }
    let version = probe
        .output()
        .ok()
        .and_then(|output| {
            let text = if (cc.style().is_msvc() && !output.stderr.is_empty())
                || output.stdout.is_empty()
            {
                output.stderr
            } else {
                output.stdout
            };
            String::from_utf8_lossy(&text)
                .lines()
                .next()
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "version unavailable".to_string());
    let arguments = |ship: bool| {
        let sources: Vec<&Path> = if ship {
            vec![Path::new("program.c"), Path::new("entry.c")]
        } else {
            vec![Path::new("baseline.c")]
        };
        let command = subject_command(
            cc,
            &sources,
            Path::new("subject.exe"),
            Path::new("objects"),
            ship.then_some(Path::new("runtime-staticlib")),
        );
        command
            .get_args()
            .map(|arg| arg.to_string_lossy())
            .collect::<Vec<_>>()
            .join(" ")
    };
    format!(
        "{}: {version}; C [{}]; ship [{}]",
        Path::new(cc.program())
            .file_name()
            .unwrap_or(cc.program())
            .to_string_lossy(),
        arguments(false),
        arguments(true)
    )
}
