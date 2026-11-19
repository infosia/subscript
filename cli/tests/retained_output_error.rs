//! Retained-output errors cross the child outcome protocol (compiler.md §338 rule 8).

// compiler.md §338 rule 8: debug best of three: 0.04 s; 0 C builds.
#[cfg(unix)]
#[test]
fn retained_write_error_reaches_the_parent() {
    use std::process::Command;
    struct Scratch(std::path::PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let scratch =
        Scratch(std::env::temp_dir().join(format!("subscript-write-error-{}", std::process::id())));
    std::fs::create_dir_all(&scratch.0).unwrap();
    let source = scratch.0.join("print.ts");
    std::fs::write(&source, "export function main(): void { for (let i: i32 = 0; i < 2000; i += 1) { print(\"retained line\"); } }").unwrap();
    for fail in [false, true] {
        let retained = scratch
            .0
            .join(if fail { "failed.txt" } else { "control.txt" });
        std::fs::write(&retained, "").unwrap();
        let mut command = Command::new("sh");
        command
            .args([
                "-c",
                if fail {
                    "trap '' XFSZ; ulimit -f 8; exec \"$@\""
                } else {
                    "exec \"$@\""
                },
                "write-error",
            ])
            .arg(env!("CARGO_BIN_EXE_subscript"))
            .arg("run")
            .arg(&source)
            .env("SUBSCRIPT_CODEGEN_JIT_OUTPUT_FILE", &retained);
        let output = command.output().unwrap();
        let error = String::from_utf8_lossy(&output.stderr);
        if fail {
            assert!(!output.status.success(), "{output:?}");
            assert!(
                error.contains("retained") && error.contains("File too large"),
                "{error}"
            );
            assert!(error.contains(retained.to_str().unwrap()), "{error}");
            assert!(std::fs::metadata(retained).unwrap().len() < 28000);
        } else {
            assert!(output.status.success(), "{error}");
            assert_eq!(output.stdout, b"retained line\n".repeat(2000));
        }
    }
}
