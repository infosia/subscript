//! Optional adoption keeps the release facts of §236 (compiler.md §318).

use super::*;
use subscript_codegen::lir::lower_module_for_reload;
use subscript_compiler::{check_program, lir::InstructionKind, SourceFile};

fn release_facts(hir: &hir::Module, lir: &l::Module, reload: bool) -> Vec<String> {
    let mut facts = Vec::new();
    compare_for_reload(hir, lir, reload, &mut facts);
    facts
}

// Cost: one check, two lowerings, and six fact comparisons; measured below 0.03 s, with no native build.
#[test]
fn optional_adoption_releases_the_handle_once_on_the_exception_path() {
    let source = r#"
class S { async am(): Promise<void> { throw new Error("rejected"); } }
async function g(s: S | null): Promise<void> { return s?.am(); }
export async function main(): Promise<void> {
  try { await g(new S()); } catch (e) { print("caught"); }
  await g(null);
}
"#;
    let hir = check_program(&[SourceFile::new("optional.ts", source)]).expect("checks");
    for reload in [false, true] {
        let lir = lower_module_for_reload(&hir, reload).expect("lowers");
        let facts = release_facts(&hir, &lir, reload);
        assert!(facts.is_empty(), "reload={reload}: {facts:?}");
        let function = lir
            .functions
            .iter()
            .position(|f| f.source_name == "g")
            .expect("g");
        let landing = lir.functions[function]
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .find_map(|i| match (&i.kind, i.raise_edge()) {
                (InstructionKind::AwaitRaise, Some(l::RaiseEdge::Handler(block))) => Some(*block),
                _ => None,
            })
            .expect("adoption await has an exception landing");
        let release = lir.functions[function].blocks[landing.0 as usize]
            .instructions
            .iter()
            .position(|i| matches!(i.kind, InstructionKind::AsyncHandleRelease))
            .expect("landing releases the adopted handle");
        for duplicate in [false, true] {
            let mut invalid = lir.clone();
            let instructions =
                &mut invalid.functions[function].blocks[landing.0 as usize].instructions;
            if duplicate {
                instructions.insert(release, instructions[release].clone());
            } else {
                instructions.remove(release);
            }
            let findings = release_facts(&hir, &invalid, reload);
            assert!(
                findings
                    .iter()
                    .any(|f| f.contains("owned handles; lexical scopes require")),
                "reload={reload}, duplicate={duplicate}: {findings:?}"
            );
        }
    }
}
