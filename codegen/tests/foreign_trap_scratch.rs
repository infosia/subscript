//! Foreign-call scratch release (compiler.md section 264).
//! Cost: two ship program builds on Unix.
#![deny(dead_code)]
#[path = "support/main_thread.rs"]
mod main_thread;

mod cases {
    use std::ffi::{c_char, c_void};
    use subscript_codegen::{NativeLibrary, ReloadSession, RunError};
    use subscript_compiler::SourceFile;
    use subscript_runtime::TrapKind;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct View {
        data: *const c_char,
        len: usize,
    }

    #[repr(C)]
    struct State {
        label: View,
        value: u32,
        callback: unsafe extern "C" fn(View, *mut c_void, *mut c_void),
        userdata: *mut c_void,
        userparam: *mut c_void,
    }

    #[repr(C)]
    struct Outer {
        label: View,
        child: *const c_void,
        value: u32,
    }

    unsafe extern "C" fn scratch_call(state: *mut State, target: *mut Outer) {
        // SAFETY: the mirror supplies live pointers with these C layouts.
        unsafe {
            (*target).value = 42;
            ((*state).callback)((*state).label, (*state).userdata, (*state).userparam);
        }
    }

    #[cfg(unix)]
    struct ShipFixture(std::path::PathBuf);

    #[cfg(unix)]
    impl Drop for ShipFixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[cfg(unix)]
    fn ship_fixture() -> ShipFixture {
        let root =
            std::env::temp_dir().join(format!("subscript-trap-scratch-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let fixture = ShipFixture(root.clone());
        let header = r#"#include <stdint.h>
#include <stddef.h>
typedef struct View { const char *data; size_t len; } View;
typedef void (*Callback)(View, void *, void *);
typedef struct State { View label; uint32_t value; Callback callback; void *userdata; void *userparam; } State;
typedef struct Target { View label; uint32_t value; size_t valueCount; const uint32_t *values; } Target;
typedef struct Outer { View label; const Target *child; uint32_t value; } Outer;
void scratchCall(State *state, Outer *target);
"#;
        std::fs::write(root.join("scratch.h"), header).unwrap();
        std::fs::write(root.join("scratch.c"), "#include \"scratch.h\"\nvoid scratchCall(State *s, Outer *target) { target->value = 42; s->callback(s->label, s->userdata, s->userparam); }\n").unwrap();
        fixture
    }

    pub fn scratch_is_released_after_callback_traps() {
        // SAFETY: the Rust C ABI function matches the mirror and lives for the process.
        let library = unsafe {
            NativeLibrary::new(
                Vec::new(),
                Vec::new(),
                vec![("scratchCall".into(), scratch_call as *const u8)],
            )
        };
        #[cfg(unix)]
        let fixture = ship_fixture();
        let mirror = r#"// @subscript-c-header include="scratch.h"
// @subscript-c-callback typedef="Callback"
// @subscript-c-parameter function="scratchCall" parameter="state" const=false
// @subscript-c-parameter function="scratchCall" parameter="target" const=false
type Callback = (message: string, userdata: object | null, userparam: object | null) => void;
declare class State {
 label: string;
 value: u32;
 callback: Callback;
 userdata: object | null;
 userparam: object | null;
 constructor(label: string, value: u32, callback: Callback, userdata: object | null, userparam: object | null);
}
declare class Target {
 label: string;
 value: u32;
 values: u32[];
 constructor(label: string, value: u32, values: u32[]);
}
// @subscript-c-member aggregate="Outer" member="child" const=true
declare class Outer {
 label: string;
 child: Target | null;
 value: u32;
 constructor(label: string, child: Target | null, value: u32);
}
declare function scratchCall(state: State | null, target: Outer | null): void;
"#;
        for throws in [true, false] {
            let body = if throws {
                "throw new Error(\"callback\");"
            } else {
                ""
            };
            let files = [
                SourceFile::ambient("scratch.generated.d.ts", mirror),
                SourceFile::new(
                    "main.ts",
                    format!(
                        r#"
export function main(): void {{
 const state: State = new State("scratch", 1, (message, userdata, userparam) => {{ {body} }}, null, null);
 const target: Target = new Target("target", 1, [7, 8]);
 const outer: Outer = new Outer("outer", target, 1);
 scratchCall(state, outer);
 print(`${{outer.value}}`);
}}
"#
                    ),
                ),
            ];
            let mut session =
                ReloadSession::new_with_native_libraries(&files, std::slice::from_ref(&library))
                    .expect("session");
            let mut counts = Vec::new();
            for _ in 0..3 {
                let result = session.call_main();
                if throws {
                    assert!(
                        matches!(result, Err(RunError::Trap(ref t)) if t.rule == TrapKind::UncaughtException),
                        "{result:?}"
                    );
                } else {
                    result.expect("normal call");
                    assert_eq!(session.take_output(), b"42\n");
                }
                counts.push(session.live_boundary_scratch_allocations());
            }
            println!("throws={throws}: scratch counts {counts:?}");
            assert_eq!(counts, [0, 0, 0]);
            #[cfg(unix)]
            {
                // SAFETY: ship compiles the C source with the same mirror ABI.
                let ship_library = unsafe {
                    NativeLibrary::new(
                        vec![fixture.0.clone()],
                        vec![fixture.0.join("scratch.c")],
                        vec![("scratchCall".into(), scratch_call as *const u8)],
                    )
                };
                let ship = subscript_codegen::run_c_aot_with_native_libraries(
                    &files,
                    std::slice::from_ref(&ship_library),
                );
                if throws {
                    assert!(
                        matches!(ship, Err(RunError::Trap(ref t)) if t.rule == TrapKind::UncaughtException),
                        "{ship:?}"
                    );
                } else {
                    assert_eq!(ship.expect("ship normal call"), b"42\n");
                }
            }
        }
    }
}

fn main() -> std::process::ExitCode {
    main_thread::run(&main_thread_tests! {
        parallel: [],
        main_thread: [cases::scratch_is_released_after_callback_traps],
    })
}
