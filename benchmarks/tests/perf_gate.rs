fn perf_gate_skip_line(debug: bool) -> Option<&'static str> {
    debug.then_some("gate-skip: perf_gate debug runtime is not the optimized gate subject")
}

#[test]
fn perf_gate_skip_line_is_declared() {
    assert_eq!(
        perf_gate_skip_line(true),
        Some("gate-skip: perf_gate debug runtime is not the optimized gate subject")
    );
    assert_eq!(perf_gate_skip_line(false), None);
}

#[test]
fn perf_gate_meets_every_threshold() {
    if let Some(line) = perf_gate_skip_line(cfg!(debug_assertions)) {
        use std::io::Write;
        // Start a new line after any test harness prefix, outside its capture.
        writeln!(std::io::stdout().lock(), "\n{line}").expect("write the gate skip line");
        return;
    }

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_perf-gate"))
        .arg("--gate")
        .output()
        .expect("run the built perf-gate binary");
    assert!(
        output.status.success(),
        "perf-gate exited with {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
