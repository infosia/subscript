use std::process::ExitCode;

fn main() -> ExitCode {
    let stdout = std::io::stdout();
    let stderr = std::io::stderr();
    let mut stdout = stdout.lock();
    let mut stderr = stderr.lock();
    ExitCode::from(subscript_cli::execute(
        std::env::args_os().skip(1),
        &mut stdout,
        &mut stderr,
    ))
}
