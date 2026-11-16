//! Compiler contract §320 rules 2, 4a, and 5.
//! Cost: each test compiles one program in each tier; measured time printed
//! by the test, excluding the build.
use std::{process::Command, time::Instant};
use subscript_codegen::{interpreter::interpret, lir::lower_module, run_c_aot, run_jit};
use subscript_compiler::{check_program, SourceFile};

fn compare(source: &str, expected: &[u8]) {
    let start = Instant::now();
    let files = [SourceFile::new("defaults.ts", source)];
    let module = check_program(&files).expect("checks");
    assert_eq!(
        interpret(&lower_module(&module).expect("lowers")).expect("interpreter"),
        expected
    );
    assert_eq!(run_jit(&files).expect("development tier"), expected);
    assert_eq!(run_c_aot(&files).expect("ship tier"), expected);
    eprintln!("three tiers: {:?}", start.elapsed());
}

/// Measured cost: 0.325s to run the corpus in all three tiers, excluding the build.
#[test]
fn corpus_defaults_run_once_per_call_in_each_tier() {
    compare(
        include_str!("../../corpus/accept/a470-a-function-with-defaults-fits-a-place-with-fewer-parameters.ts"),
        include_bytes!("../../corpus/accept/a470-a-function-with-defaults-fits-a-place-with-fewer-parameters.expected"),
    );
}

/// Measured cost: 0.494s to run one program in all three tiers, excluding the build.
#[test]
fn callback_indices_and_captured_defaults_keep_their_call_order() {
    compare(r#"
        function tick(a: i32): i32 { print(`default ${a}`); return a + 10; }
        function indexed(a: i32, i: i32 = 99): i32 { return a + i; }
        function sorted(a: i32, b: i32, k: i32 = tick(0)): i32 { return a - b; }
        function f(a: i32, b: i32 = tick(a), c: i32 = tick(b)): i32 { return a + b + c; }
        function app(k: (a: i32) => i32): i32 { return k(1); }
        const alias = f;
        export function main(): void {
            const xs = [1, 2].map(indexed);
            print(`${xs[0]},${xs[1]}`);
            const ys = [1, 2].sort(sorted);
            print(`${ys[0]},${ys[1]}`);
            print(`${app(alias)}`);
            const localAlias = f;
            print(`${app(localAlias)}`);
            const offset: i32 = 5;
            const local = (a: i32, b: i32 = tick(offset)): i32 => a + b;
            const other = local;
            print(`${app(other)}`);
            print(`${app(other)}`);
        }
    "#, b"1,3\ndefault 0\n1,2\ndefault 1\ndefault 11\n33\ndefault 1\ndefault 11\n33\ndefault 5\n16\ndefault 5\n16\n");
}

/// Rule 4a: groupBy passes the index before defaults after the library arguments.
/// Measured cost: 1.204s for one Node run and three tier runs, excluding the build.
#[test]
fn group_by_passes_the_index_in_each_tier_and_matches_node() {
    let start = Instant::now();
    let source = r#"
        function grp(a: i32, i: i32 = 99): string { return `${a}:${i}`; }
        function extra(a: i32, i: i32 = 99, k: i32 = 10): string { return `${a}:${i + k}`; }
        export function main(): void {
            const groups = Map.groupBy([5, 6], grp);
            for (const key of groups.keys()) { print(key); }
            const alias = extra;
            const more = Map.groupBy([5, 6], alias);
            for (const key of more.keys()) { print(key); }
            const literal = Map.groupBy([5, 6], (a: i32, i: i32 = 99): string => `${a}:${i}`);
            for (const key of literal.keys()) { print(key); }
        }
    "#;
    let node = Command::new("node")
        .arg("-e")
        .arg("const ts = require('typescript'); globalThis.print = console.log; eval(ts.transpileModule(process.argv[1], { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText); exports.main();")
        .arg(source)
        .output()
        .expect("run node");
    assert!(
        node.status.success(),
        "{}",
        String::from_utf8_lossy(&node.stderr)
    );
    assert_eq!(node.stdout, b"5:0\n6:1\n5:10\n6:11\n5:0\n6:1\n");
    compare(source, &node.stdout);
    eprintln!("groupBy with node: {:?}", start.elapsed());
}
