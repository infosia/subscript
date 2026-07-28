//! Runs the host MSVC C compiler with its discovered toolchain environment.
//!
//! This keeps shell examples independent of a prior `vcvars` invocation:
//! `examples/host/build.sh` runs under `sh` (Git Bash), which does not carry
//! the MSVC `INCLUDE`/`LIB`/`PATH` a bare `cl` needs. This shim resolves
//! `cl.exe` and its environment through the same registry lookup the ship
//! tier uses (`codegen/src/aot.rs`, compiler.md §11c), forwards its own CLI
//! arguments to `cl`, and exits with `cl`'s exit code.

#[cfg(all(windows, target_env = "msvc"))]
use std::ffi::OsString;
#[cfg(all(windows, target_env = "msvc"))]
use std::process::Command;
use std::process::ExitCode;

#[cfg(all(windows, target_env = "msvc"))]
fn main() -> Result<ExitCode, String> {
    let mut command = if let Some(cc) = std::env::var_os("CC") {
        Command::new(cc)
    } else {
        let target = target_lexicon::HOST.to_string();
        let tool = cc::windows_registry::find_tool(&target, "cl.exe")
            .ok_or_else(|| format!("MSVC cl.exe was not found for target {target}"))?;
        let mut command = Command::new(tool.path());
        command.envs(tool.env().iter().cloned());
        command
    };
    command.args(std::env::args_os().skip(1).collect::<Vec<OsString>>());
    let status = command
        .status()
        .map_err(|error| format!("failed to run the host C compiler: {error}"))?;
    Ok(ExitCode::from(status.code().unwrap_or(1) as u8))
}

#[cfg(not(all(windows, target_env = "msvc")))]
fn main() -> Result<ExitCode, String> {
    Err("msvc-cl is only available on windows-msvc hosts".to_string())
}
