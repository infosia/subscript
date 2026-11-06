// Argument pins for both benchmark runners.

use super::c_toolchain::*;
use std::path::Path;
use std::process::Command;
#[cfg(all(windows, target_env = "msvc"))]
use subscript_codegen::host_c_compiler;
use subscript_codegen::CCompilerStyle;

fn assert_unix_subject_arguments(unix_baseline_pin: &[&str]) {
    // Cost: two command constructions and argument comparisons; no compiler execution.
    for ship in [false, true] {
        let sources: Vec<&Path> = if ship {
            vec![Path::new("program.c"), Path::new("entry.c")]
        } else {
            vec![Path::new("baseline.c")]
        };
        let command = configure(
            Command::new("clang"),
            CCompilerStyle::Unix,
            &sources,
            Path::new("subject"),
            Path::new("objects"),
            ship.then_some(Path::new("runtime")),
        );
        assert_eq!(command.get_program(), "clang");
        let mut expected = Vec::new();
        if ship {
            expected.push("-std=c11");
            if cfg!(target_os = "linux") {
                expected.push("-D_POSIX_C_SOURCE=199309L");
            }
        }
        expected.extend_from_slice(unix_baseline_pin);
        if ship {
            expected.extend(["-fwrapv", "program.c", "entry.c", "runtime"]);
            if cfg!(windows) {
                expected.extend([
                    "-lkernel32",
                    "-lntdll",
                    "-luserenv",
                    "-lws2_32",
                    "-ldbghelp",
                ]);
            } else if cfg!(unix) && !cfg!(target_os = "macos") {
                expected.extend([
                    "-lm",
                    "-ldl",
                    "-lpthread",
                    "-lrt",
                    "-lutil",
                    "-lgcc_s",
                    "-lc",
                ]);
            }
            if cfg!(target_os = "macos") {
                expected.push("-Wl,-dead_strip");
            } else if cfg!(windows) || cfg!(unix) {
                expected.push("-Wl,--gc-sections");
            }
        } else {
            expected.push("baseline.c");
        }
        expected.extend(["-o", "subject"]);
        assert_eq!(command.get_args().collect::<Vec<_>>(), expected);
    }
}

#[test]
#[cfg(all(windows, target_env = "msvc"))]
fn every_host_subject_uses_msvc_and_its_environment() {
    // Cost: one toolchain lookup and two command comparisons; no compiler execution.
    let cc = host_c_compiler().expect("host C compiler");
    assert!(cc.style().is_msvc());
    for ship in [false, true] {
        let sources: Vec<&Path> = if ship {
            vec![Path::new("program.c"), Path::new("entry.c")]
        } else {
            vec![Path::new("baseline.c")]
        };
        let command = subject_command(
            &cc,
            &sources,
            Path::new("subject.exe"),
            Path::new("objects"),
            ship.then_some(Path::new("runtime")),
        );
        assert_eq!(command.get_program(), cc.program());
        assert_eq!(
            command.get_envs().collect::<Vec<_>>(),
            cc.command().get_envs().collect::<Vec<_>>()
        );
        let mut expected = vec![
            "/nologo",
            "/std:c11",
            "/O2",
            "/utf-8",
            "/fp:strict",
            "/we4028",
            "/we4029",
            "/Fo:objects\\",
        ];
        if ship {
            expected.extend([
                "/Iobjects",
                "program.c",
                "entry.c",
                "runtime",
                "kernel32.lib",
                "ntdll.lib",
                "userenv.lib",
                "ws2_32.lib",
                "dbghelp.lib",
            ]);
        } else {
            expected.push("baseline.c");
        }
        expected.extend(["/Fe:subject.exe", "-link"]);
        if ship {
            expected.push("/OPT:REF");
        }
        assert_eq!(command.get_args().collect::<Vec<_>>(), expected);
    }
}
