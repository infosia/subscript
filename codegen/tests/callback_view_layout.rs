//! §298: callback view layout assertions. Cost: one HIR check, one C emission,
//! two compile-only C invocations; measured test execution: 0.09 s (macOS).
//! The HIR is built directly through the checker: the binder rejects the
//! reversed member order before C compilation, so the firing control bypasses it.
use subscript_codegen::{add_c11_optimized_flags, emit_c, host_c_compiler, HOST_HEADER_C};
use subscript_compiler::{check_program, SourceFile};

#[test]
fn callback_view_assertion_and_firing_control() {
    let module = check_program(&[
        SourceFile::ambient("host.d.ts", r#"// @subscript-c-header include="host.h"
// @subscript-c-callback typedef="Callback" aggregate="HostView" data="data" len="len"
type Callback = (message: string, u1: object | null, u2: object | null) => void;
declare class Sink {
 callback: Callback;
 u1: object | null;
 u2: object | null;
 constructor(callback: Callback, u1: object | null, u2: object | null);
}
declare function consume(sink: Sink): void;
"#),
        SourceFile::new("main.ts", "export function main(): void { const sink: Sink = new Sink((m,u1,u2)=>{}, null, null); consume(sink); consume(sink); }"),
    ]).unwrap();
    let c = emit_c(&module).unwrap();
    assert_eq!(
        c.source.matches("_Static_assert(sizeof(HostView)").count(),
        1
    );
    for fact in [
        "sizeof",
        "_Alignof",
        "offsetof(HostView, data)",
        "offsetof(HostView, len)",
        "sizeof(((HostView*)0)->data) == sizeof(((subscript_callback_string_view*)0)->data)",
        "sizeof(((HostView*)0)->len) == sizeof(((subscript_callback_string_view*)0)->len)",
    ] {
        assert!(c.source.contains(fact), "{fact}");
    }
    assert!(
        c.source.find("_Static_assert(sizeof(HostView)").unwrap()
            < c.source
                .find("(Callback)&subscript_rt_cb_trampoline")
                .unwrap()
    );
    let dir = std::env::temp_dir().join(format!("subscript-s298-{}", std::process::id()));
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(dir.clone());
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("program.c"), &c.source).unwrap();
    std::fs::write(dir.join("program.h"), &c.host_header).unwrap();
    std::fs::write(dir.join("subscript_runtime.h"), HOST_HEADER_C).unwrap();
    for (members, success) in [
        ("const uint8_t *data; size_t len;", true),
        ("size_t len; const uint8_t *data;", false),
    ] {
        std::fs::write(dir.join("host.h"), format!("#include <stdint.h>\n#include <stddef.h>\ntypedef struct HostView {{ {members} }} HostView;\ntypedef void (*Callback)(HostView, void*, void*);\ntypedef struct Sink {{ Callback callback; void *u1; void *u2; }} Sink;\nvoid consume(Sink sink);\n")).unwrap();
        let tool = host_c_compiler().unwrap();
        let mut cmd = tool.command();
        cmd.current_dir(&dir);
        add_c11_optimized_flags(&mut cmd, tool.style());
        if tool.style() == subscript_codegen::CCompilerStyle::Msvc {
            cmd.args(["/c", "program.c", "/Foprogram.obj"]);
        } else {
            cmd.args(["-c", "program.c", "-o", "program.o"]);
        }
        let output = cmd.output().unwrap();
        let diagnostics = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.status.success(), success, "{diagnostics}");
        if !success {
            assert!(
                diagnostics.contains("callback string-view layout: HostView"),
                "{diagnostics}"
            );
        }
    }
}
