#[test]
fn perf_gate_meets_every_threshold() {
    if cfg!(debug_assertions) {
        println!(
            "perf-gate did not run because the debug runtime is not the optimized gate subject"
        );
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
