//! Optional adoption releases its task frames (compiler.md §318 rule 2).

use subscript_codegen::ReloadSession;
use subscript_compiler::SourceFile;

// Cost: one JIT session and three entry calls; measured 0.10 s, with no external process.
#[test]
fn optional_adoption_returns_to_the_allocation_baseline_on_each_path() {
    let source = r#"
const failure: Error = new Error("rejected");
class S {
  async am(fail: boolean): Promise<void> {
    await Context.suspend();
    if (fail) throw failure;
  }
}
const s = new S();
async function g(s: S | null, fail: boolean): Promise<void> { return s?.am(fail); }
export async function main(): Promise<void> {
  for (let i: i32 = 0; i < 12; i++) {
    await g(s, false);
    try { await g(s, true); } catch (e) {}
    await g(null, false);
    await g(null, true);
  }
}
"#;
    let mut session =
        ReloadSession::new(&[SourceFile::new("optional.ts", source)]).expect("session");
    let baseline = session.live_allocations();
    for _ in 0..3 {
        session.call_main().expect("main");
        while session.async_pending() != 0 {
            session.async_step().expect("step");
        }
        assert_eq!(
            session.live_allocations(),
            baseline,
            "each task frame is released"
        );
    }
}
