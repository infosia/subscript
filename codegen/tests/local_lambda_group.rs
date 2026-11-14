//! Environment evidence for compiler.md §293 acceptance item 4.
use subscript_codegen::{lir::lower_module, run_jit_with_memory_accounting};
use subscript_compiler::{check_program, lir, SourceFile};

/// Cost: two checker/lowering calls, no native run; measured under 0.01 s (debug).
#[test]
fn a_stack_group_has_no_allocation_and_an_escaping_control_allocates_once() {
    for (result, end, allocations) in [
        ("void", "first(3);", 0),
        ("(n: i32) => i32", "return second;", 1),
    ] {
        let source = format!(
            r#"
function make(): {result} {{
    const captured: i32 = 7;
    const first = (n: i32): i32 => {{ if (n > 0) return second(n - 1); return captured; }};
    const second = (n: i32): i32 => {{ const value = first; return value(n); }};
    {end}
}}
export function main(): void {{ make(); }}
"#
        );
        let hir = check_program(&[SourceFile::new("allocation.ts", source)]).unwrap();
        let module = lower_module(&hir).unwrap();
        let actual = module
            .functions
            .iter()
            .flat_map(|f| &f.blocks)
            .flat_map(|b| &b.instructions)
            .filter(|i| matches!(i.kind, lir::InstructionKind::AllocateClass(_)))
            .count();
        assert_eq!(actual, allocations);
        let members: Vec<_> = module
            .functions
            .iter()
            .filter(|f| f.kind == lir::FunctionKind::Lambda)
            .collect();
        assert_eq!(members.len(), 2);
        assert!(members[0].lambda_group.is_some());
        assert_eq!(members[0].lambda_group, members[1].lambda_group);
        for member in members {
            assert_eq!(
                member.parameters[0].kind,
                if allocations == 0 {
                    lir::ParameterKind::Capture
                } else {
                    lir::ParameterKind::OwnedEnvironment
                }
            );
        }
    }
}

/// Cost: four JIT runs (1/50 iterations, discarded/retained controls), 102 collections.
/// Measured: 0.143 s (debug); discarded 74/74 bytes, retained 174/2938 bytes.
#[test]
fn collection_releases_counted_group_captures_over_fifty_iterations() {
    let started = std::time::Instant::now();
    let mut discarded = Vec::new();
    let mut retained = Vec::new();
    for (keep, counts) in [(false, &mut discarded), (true, &mut retained)] {
        for iterations in [1, 50] {
            let source = format!(
                r#"
class Box {{ value: i32; constructor(value: i32) {{ this.value = value; }} }}
const kept: ((n: i32) => string)[] = [];
function make(i: i32): (n: i32) => string {{
    const text: string = `capture${{i}}`;
    const box = new Box(i);
    const first = (n: i32): string => {{ if (n > 0) return second(n - 1); return `${{text}}:${{box.value}}`; }};
    const second = (n: i32): string => {{ const value = first; return value(n); }};
    return second;
}}
function exercise(i: i32): void {{
    const value = make(i);
    if (value(2) != `capture${{i}}:${{i}}`) throw new Error("capture changed");
    if ({keep}) kept.push(value);
}}
export function main(): void {{
    for (let i: i32 = 0; i < {iterations}; i++) {{ exercise(i); Context.collect(); }}
    print("ok");
}}
"#
            );
            let (output, memory) =
                run_jit_with_memory_accounting(&[SourceFile::new("collection.ts", source)], true)
                    .unwrap();
            assert_eq!(output, b"ok\n");
            counts.push(memory.live_bytes);
        }
    }
    assert_eq!(discarded[0], discarded[1], "discarded groups grow");
    assert!(retained[1] > retained[0], "retained control must grow");
    eprintln!(
        "group captures: discarded={discarded:?} retained={retained:?}; elapsed={:?}",
        started.elapsed()
    );
}

/// Cost: one generated program on each tier; measured 0.43 s (debug).
#[test]
fn a_forward_member_default_reads_the_shared_counted_captures() {
    let source = r#"
class Box { value: i32 = 9; }
export function main(): void {
    const text: string = "default";
    const box = new Box();
    const first = (): string => second();
    const second = (n: i32 = box.value, s: string = text): string => `${s}:${n}`;
    print(first());
}
"#;
    let files = [SourceFile::new("group-default.ts", source)];
    let hir = check_program(&files).unwrap();
    let lir = lower_module(&hir).unwrap();
    let expected = b"default:9\n";
    assert_eq!(
        subscript_codegen::interpreter::interpret(&lir).unwrap(),
        expected
    );
    assert_eq!(subscript_codegen::run_jit(&files).unwrap(), expected);
    assert_eq!(subscript_codegen::run_c_aot(&files).unwrap(), expected);
}
