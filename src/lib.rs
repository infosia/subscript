#[test]
fn nested_cargo() {
    let out = std::process::Command::new(env!("CARGO"))
        .args(["build", "--offline", "-p", "subscript-runtime"])
        .current_dir("<redacted>")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .env_remove("RUSTC")
        .env_remove("RUSTC_WORKSPACE_WRAPPER")
        .output()
        .expect("spawn");
    eprintln!("status {:?} stderr {}", out.status, String::from_utf8_lossy(&out.stderr));
    assert!(out.status.success());
}
