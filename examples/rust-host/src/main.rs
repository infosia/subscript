use std::io::{self, Write as _};
use std::process::ExitCode;

fn report_error(message: &str) -> ExitCode {
    let _ = writeln!(io::stderr().lock(), "{message}");
    ExitCode::FAILURE
}

fn write_output(stdout_bytes: &[u8], stderr_lines: &[String]) -> io::Result<()> {
    io::stdout().lock().write_all(stdout_bytes)?;
    let mut stderr = io::stderr().lock();
    for line in stderr_lines {
        writeln!(stderr, "{line}")?;
    }
    Ok(())
}

fn main() -> ExitCode {
    let (stdout_bytes, stderr_lines) = match subscript_example_rust_host::run() {
        Ok(output) => output,
        Err(error) => return report_error(&error),
    };

    match write_output(&stdout_bytes, &stderr_lines) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => report_error(&format!("write host output: {error}")),
    }
}
