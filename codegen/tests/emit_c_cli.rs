//! The host-facing explicit-path form of the `emit-c` command.

use std::path::PathBuf;
use std::process::Command;

#[test]
fn explicit_sources_and_mirrors_can_suppress_the_generated_entry() {
    struct Cleanup(PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    let root = std::env::temp_dir().join(format!(
        "subscript-emit-c-explicit-{}",
        std::process::id()
    ));
    let output = root.join("out");
    std::fs::create_dir_all(&root).expect("create temp directory");
    let _cleanup = Cleanup(root.clone());
    let source = root.join("main.ts");
    let mirror = root.join("host.generated.d.ts");
    std::fs::write(
        &source,
        "export function main(): void {\n  print(`explicit`);\n}\n",
    )
    .expect("write source");
    std::fs::write(
        &mirror,
        "// @subscript-c-header include=\"host.h\"\n\
         declare function hostTick(): void;\n",
    )
    .expect("write mirror");

    let result = Command::new(env!("CARGO_BIN_EXE_emit-c"))
        .arg(&output)
        .arg("--source")
        .arg(&source)
        .arg("--mirror")
        .arg(&mirror)
        .arg("--no-entry")
        .output()
        .expect("run emit-c");
    assert!(
        result.status.success(),
        "emit-c failed:\n{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(output.join("program.c").is_file());
    assert!(output.join("program.alloc.h").is_file());
    assert!(!output.join("entry.c").exists());
    let emitted = std::fs::read_to_string(output.join("program.c")).expect("read emitted C");
    assert!(emitted.contains("#include \"host.h\""), "{emitted}");
}
