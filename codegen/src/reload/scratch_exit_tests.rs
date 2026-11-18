//! §296: second scratch request fails; the same call without a fault is the control.
use super::*;

fn files() -> [SourceFile; 2] {
    [
        SourceFile::ambient(
            "scratch.generated.d.ts",
            r#"
// @subscript-c-header include="scratch.h"
// @subscript-c-member aggregate="Outer" member="child" const=true
declare class Inner { label: string; constructor(label: string); }
declare class Outer { label: string; child: Inner | null; constructor(label: string, child: Inner | null); }
// @subscript-c-parameter function="observe" parameter="a" const=true
// @subscript-c-parameter function="observe" parameter="b" const=true
declare function observe(a: Outer | null, b: Outer | null): void;
"#,
        ),
        SourceFile::new(
            "main.ts",
            r#"
const inner: Inner = new Inner("inner");
const outer: Outer = new Outer("outer", inner);
export function main(): void { observe(outer, outer); }
"#,
        ),
    ]
}

unsafe extern "C" fn observe(_: *const u8, _: *const u8) {}

// §296. Measured cost: two dev sessions, <0.83 s.
#[test]
fn dev_scratch_allocation_exit() {
    let library = unsafe {
        NativeLibrary::new(
            vec![],
            vec![],
            vec![("observe".into(), observe as *const u8)],
        )
    };
    for failure in [0, 2] {
        let mut session =
            ReloadSession::new_with_native_libraries(&files(), std::slice::from_ref(&library))
                .unwrap();
        session.ctx.fail_alloc_after(failure);
        let result = session.call_main();
        if failure != 0 {
            assert!(
                matches!(result, Err(RunError::Trap(ref t)) if t.rule == subscript_runtime::TrapKind::AllocationFailure),
                "{result:?}"
            );
        } else {
            result.unwrap();
        }
        let count = session.live_boundary_scratch_allocations();
        println!("dev failure={failure}: scratch={count}");
        assert_eq!(count, 0);
    }
}

// compiler.md §337 rule 3: debug best of three: 0.78 s; two C builds.
#[cfg(unix)]
#[test]
fn ship_scratch_allocation_exit() {
    let root = std::env::temp_dir().join(format!("subscript-296-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let _guard = ScratchDir(root.clone());
    std::fs::write(root.join("scratch.h"), "#include <stddef.h>\ntypedef struct { const char *data; size_t len; } View;\ntypedef struct Inner { View label; } Inner;\ntypedef struct Outer { View label; const Inner *child; } Outer;\nvoid observe(const Outer *, const Outer *);\n").unwrap();
    let count_file = root.join("count");
    for failure in [0, 2] {
        std::fs::write(root.join("scratch.c"), format!(r#"
#include "scratch.h"
#include <stdint.h>
#include <stdio.h>
extern void subscript_rt_ctx_fail_alloc_after(void *, uint64_t);
extern uint64_t subscript_rt_boundary_scratch_mark(void *);
void arm(void *ctx) {{ subscript_rt_ctx_fail_alloc_after(ctx, {failure}); }}
void count(void *ctx) {{ FILE *f = fopen("{}", "w"); fprintf(f, "%llu", (unsigned long long)subscript_rt_boundary_scratch_mark(ctx)); fclose(f); }}
void observe(const Outer *a, const Outer *b) {{ (void)a; (void)b; }}
"#, count_file.display())).unwrap();
        let library = unsafe {
            NativeLibrary::new(
                vec![root.clone()],
                vec![root.join("scratch.c")],
                vec![("observe".into(), observe as *const u8)],
            )
        };
        let libraries = [library];
        let mut config = crate::RunConfig::default().with_native_libraries(&libraries);
        config.pre_entry_hook = Some("arm");
        config.post_run_hook = Some("count");
        // compiler.md §337 rule 5: a missing hook must leave a failing count.
        std::fs::write(&count_file, "unwritten").unwrap();
        let result = crate::run_c_aot_configured(&files(), config);
        if failure != 0 {
            assert!(
                matches!(result, Err(RunError::Trap(ref t)) if t.rule == subscript_runtime::TrapKind::AllocationFailure),
                "{result:?}"
            );
        } else {
            result.unwrap();
        }
        let count = std::fs::read_to_string(&count_file).unwrap();
        println!("ship failure={failure}: scratch={count}");
        assert_eq!(count, "0");
    }
}

#[cfg(unix)]
struct ScratchDir(std::path::PathBuf);
#[cfg(unix)]
impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

// §302: measured cost: <0.01 s; one directory and one unwind, no build.
#[cfg(unix)]
#[test]
fn scratch_guard_cleans_on_assertion_failure() {
    let root = std::env::temp_dir().join(format!("subscript-302-guard-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let result = std::panic::catch_unwind(|| {
        let _guard = ScratchDir(root.clone());
        assert!(!root.exists(), "injected assertion");
    });
    assert!(result.is_err());
    let exists = root.exists();
    let _ = std::fs::remove_dir_all(&root);
    assert!(!exists);
}
