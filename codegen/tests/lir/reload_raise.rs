//! Compile-mode call edges and precise firing controls (compiler.md §121.2).

use super::*;
use subscript_codegen::lir::lower_module_for_reload;

#[test]
fn reload_call_sites_have_edges_and_ordinary_call_sites_stay_precise() {
    let source = r#"
function quiet(): void {}
function raises(): void { throw new Error("control"); }
class R {
    constructor() { quiet(); }
    method(): void { quiet(); }
}
const saved: () => void = (): void => { try { quiet(); } catch (e) { print("caught"); } };
export function main(): void {
    quiet();
    const r = new R();
    r.method();
    saved();
    try { raises(); } catch (e) { print("control"); }
}
"#;
    let hir = check_program(&[SourceFile::new("edges.ts", source)]).expect("checked source");
    assert!(
        !hir.functions
            .iter()
            .find(|f| f.name == "quiet")
            .unwrap()
            .can_raise
    );
    let ordinary = lower_module(&hir).expect("ordinary LIR");
    let reload = lower_module_for_reload(&hir, true).expect("reload LIR");
    for (module, conservative) in [(&ordinary, false), (&reload, true)] {
        let mut quiet_calls = 0;
        let mut raising_calls = 0;
        let mut handled_quiet_calls = 0;
        for function in &module.functions {
            for instruction in function.blocks.iter().flat_map(|block| &block.instructions) {
                let InstructionKind::Call(target) = &instruction.kind else {
                    continue;
                };
                if conservative
                    && instruction
                        .traps
                        .iter()
                        .any(|trap| trap.kind == TrapKind::Call)
                {
                    assert!(
                        instruction.raise_edge().is_some(),
                        "{}: {target:?}",
                        function.source_name
                    );
                }
                let lir::CallTargetKind::Function(id) = target.kind else {
                    continue;
                };
                let callee = &module.functions[id.0 as usize];
                if callee.source_name == "quiet" {
                    quiet_calls += 1;
                    assert_eq!(instruction.raise_edge().is_some(), conservative);
                    handled_quiet_calls += usize::from(instruction.handler().is_some());
                } else if callee.source_name == "raises" {
                    raising_calls += 1;
                    assert!(
                        instruction.handler().is_some(),
                        "firing control has a handler"
                    );
                }
            }
        }
        assert_eq!(quiet_calls, 4);
        assert_eq!(raising_calls, 1);
        assert_eq!(handled_quiet_calls, usize::from(conservative));
        assert_eq!(interpret(module).expect("control executes"), b"control\n");
    }
}
