//! Map iteration capacity after scope exit (compiler.md §271).

use subscript_codegen::run_jit_with_memory_accounting;
use subscript_compiler::SourceFile;

// Cost: one JIT compilation and 2,000 delete/insert pairs.
#[test]
fn map_iteration_exit_keeps_storage_bounded() {
    let files = [SourceFile::new(
        "iteration-owner.ts",
        r#"
export function main(): void {
    const m = new Map<i32, i32>();
    for (let i: i32 = 0; i < 4; i++) { m.set(i, i); }
    for (const v of m.values()) { }
    for (let i: i32 = 0; i < 2000; i++) {
        m.delete(i);
        m.set(i + 4, i);
    }
    print(`${m.size}`);
}
"#,
    )];
    let (output, memory) =
        run_jit_with_memory_accounting(&files, false).expect("growth program runs");
    assert_eq!(output, b"4\n");
    let hir = subscript_compiler::check_program(&files).expect("valid program");
    let lir = subscript_codegen::lir::lower_module(&hir).expect("valid LIR");
    assert_eq!(
        subscript_codegen::interpreter::interpret(&lir).expect("interpreter"),
        output
    );
    assert_eq!(subscript_codegen::run_c_aot(&files).expect("C AOT"), output);
    assert!(
        memory.reserved_bytes < 32768,
        "four live entries require bounded storage: {memory:?}"
    );
}

// Cost: four small programs, each with 2,000 mutation pairs in three tiers.
#[test]
fn map_iteration_scope_exits_keep_storage_bounded() {
    for exit in [
        "for (const v of m.values()) { break; }",
        "leave(m);",
        "try { for (const v of m.values()) { throw new Error(\"exit\"); } } catch (e) {}",
        "dropGenerator(m);",
    ] {
        let source = format!(
            r#"
function leave(m: Map<i32, i32>): void {{
    for (const v of m.values()) {{ return; }}
}}
function* visit(m: Map<i32, i32>): Generator<i32> {{
    for (const v of m.values()) {{ yield v; }}
}}
function dropGenerator(m: Map<i32, i32>): void {{
    const g = visit(m);
    g.next();
}}
export function main(): void {{
    const m = new Map<i32, i32>();
    for (let i: i32 = 0; i < 4; i++) {{ m.set(i, i); }}
    {exit}
    for (let i: i32 = 0; i < 2000; i++) {{ m.delete(i); m.set(i + 4, i); }}
    print(`${{m.size}}`);
}}
"#
        );
        let files = [SourceFile::new("exits.ts", &source)];
        let (output, memory) = run_jit_with_memory_accounting(&files, false).expect(exit);
        assert_eq!(output, b"4\n", "{exit}");
        assert!(memory.reserved_bytes < 32768, "{exit}: {memory:?}");
        let hir = subscript_compiler::check_program(&files).expect("valid program");
        let lir = subscript_codegen::lir::lower_module(&hir).expect("valid LIR");
        assert_eq!(
            subscript_codegen::interpreter::interpret(&lir).expect(exit),
            output
        );
        assert_eq!(subscript_codegen::run_c_aot(&files).expect(exit), output);
    }
}

// Cost: one async program with one worker suspension and 2,000 mutation pairs.
#[test]
fn async_cursor_ends_when_frame_completes() {
    let files = [SourceFile::new(
        "async-owner.ts",
        r#"
async function visit(m: Map<i32, i32>): Promise<void> {
    for (const v of m.values()) {
        await Context.suspend();
        break;
    }
}
export async function main(): Promise<void> {
    const m = new Map<i32, i32>();
    for (let i: i32 = 0; i < 4; i++) { m.set(i, i); }
    await visit(m);
    for (let i: i32 = 0; i < 2000; i++) { m.delete(i); m.set(i + 4, i); }
    print(`${m.size}`);
}
"#,
    )];
    let (output, memory) = run_jit_with_memory_accounting(&files, false).expect("async completion");
    assert_eq!(output, b"4\n");
    assert!(memory.reserved_bytes < 32768, "{memory:?}");
    let hir = subscript_compiler::check_program(&files).expect("valid program");
    let lir = subscript_codegen::lir::lower_module(&hir).expect("valid LIR");
    assert_eq!(
        subscript_codegen::interpreter::interpret(&lir).expect("interpreter"),
        output
    );
    assert_eq!(subscript_codegen::run_c_aot(&files).expect("C AOT"), output);
}

// Cost: one small corpus program in each of the three tiers.
#[test]
fn corpus_a444_matches_three_tiers() {
    let files = [SourceFile::new(
        "a444.ts",
        include_str!("../../corpus/accept/a444-a-map-or-set-iteration-ends-with-its-owner.ts"),
    )];
    let expected = include_bytes!(
        "../../corpus/accept/a444-a-map-or-set-iteration-ends-with-its-owner.expected"
    );
    let hir = subscript_compiler::check_program(&files).expect("valid program");
    let lir = subscript_codegen::lir::lower_module(&hir).expect("valid LIR");
    let interpreted = subscript_codegen::interpreter::interpret(&lir).unwrap();
    let jit = subscript_codegen::run_jit(&files).unwrap();
    let c_aot = subscript_codegen::run_c_aot(&files).unwrap();
    assert_eq!(jit, interpreted);
    assert_eq!(c_aot, interpreted);
    assert_eq!(interpreted, expected);
}
