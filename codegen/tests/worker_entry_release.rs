//! Observable worker-result ownership (compiler.md §339 rule 6).

use subscript_codegen::{run_c_aot, run_jit, RunError};
use subscript_compiler::SourceFile;
use subscript_runtime::TrapKind;

fn source(failed: bool) -> Vec<SourceFile> {
    let completion = if failed {
        "throw new Error(\"worker result released\");"
    } else {
        ""
    };
    vec![SourceFile::entry(
        "worker-release.ts",
        format!(
            r#"
class M {{ x: i32 = 1; }}
async function job(): Promise<void> {{ {completion} }}
let stored: Promise<void>[] = [];
function consume(handle: Promise<void>): void {{ stored.push(handle); }}
function* rows(): Generator<Promise<void>> {{
  const handle = job();
  consume(handle);
  stored.pop();
  yield handle;
}}
function entry(): Generator<Promise<void>> {{
  const result = rows();
  result.next();
  return result;
}}
export function main(): void {{
  const worker = Worker.spawn<M, M>(entry);
  worker.close();
  worker.join();
  print("joined");
}}
"#
        ),
    )]
}

fn check_release(run: fn(&[SourceFile]) -> Result<Vec<u8>, RunError>) {
    assert_eq!(
        run(&source(false)).expect("completed handle control"),
        b"joined\n"
    );
    let result = run(&source(true));
    assert!(
        matches!(&result, Err(RunError::Trap(trap))
            if trap.rule == TrapKind::WorkerTrapped
                && trap.message.contains("worker result released")
                && trap.stdout.is_empty()),
        "the returned generator must release its unobserved failed handle: {result:?}"
    );
}

// compiler.md §339 rule 6: debug process cost: 0.0826 s; 0 C builds.
#[test]
fn dev_worker_result_release_reports_its_unobserved_failure() {
    check_release(run_jit);
}

// compiler.md §339 rule 6: debug process cost: 0.4178 s; 2 C builds.
// The successful handle control distinguishes release failure from an unrelated worker failure.
#[test]
fn ship_worker_result_release_reports_its_unobserved_failure() {
    check_release(run_c_aot);
}
