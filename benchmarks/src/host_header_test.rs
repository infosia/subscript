//! Compiles the actual benchmark host with compiler-owned entry declarations.
pub(super) fn check(host: &str) {
    use subscript_codegen::{
        add_c11_optimized_flags, emit_c, host_c_compiler, include_directory_arg,
    };
    let module = subscript_compiler::check_program(&[subscript_compiler::SourceFile::entry(
        "main.ts",
        "export function main(): void {}",
    )])
    .unwrap();
    let program = emit_c(&module).unwrap();
    let directory = std::env::temp_dir().join(format!("benchmark-header-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let source = directory.join("entry.c");
    std::fs::write(&source, host).unwrap();
    let compiler = host_c_compiler().unwrap();
    for (header, succeeds) in [
        (program.host_header.clone(), true),
        (
            program
                .host_header
                .lines()
                .filter(|line| !line.starts_with("void subscript_export_main("))
                .collect::<Vec<_>>()
                .join("\n"),
            false,
        ),
    ] {
        std::fs::write(directory.join("program.h"), header).unwrap();
        let mut command = compiler.command();
        add_c11_optimized_flags(&mut command, compiler.style());
        command
            .arg(if compiler.style().is_msvc() {
                "/we4013"
            } else {
                "-Werror=implicit-function-declaration"
            })
            .arg(include_directory_arg(compiler.style(), &directory))
            .arg(if compiler.style().is_msvc() {
                "/Zs"
            } else {
                "-fsyntax-only"
            })
            .arg(&source);
        let result = command.output().unwrap();
        assert_eq!(
            result.status.success(),
            succeeds,
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    std::fs::remove_dir_all(directory).unwrap();
}
