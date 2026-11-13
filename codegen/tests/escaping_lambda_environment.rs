//! Three-tier and memory evidence for compiler.md §274 acceptance item 2.
use subscript_codegen::{
    interpreter::interpret, lir::lower_module, run_c_aot, run_jit, run_jit_with_memory_accounting,
};
use subscript_compiler::{check_program, SourceFile};
const SOURCE: &str = include_str!(
    "../../corpus/accept/a446-a-capturing-lambda-that-escapes-owns-its-environment.ts"
);
#[test]
fn three_tiers_match_golden() {
    let files = [SourceFile::new("owned.ts", SOURCE)];
    let expected = include_bytes!(
        "../../corpus/accept/a446-a-capturing-lambda-that-escapes-owns-its-environment.expected"
    );
    let module = check_program(&files).unwrap();
    assert_eq!(
        interpret(&lower_module(&module).unwrap()).unwrap(),
        expected
    );
    assert_eq!(run_jit(&files).unwrap(), expected);
    assert_eq!(run_c_aot(&files).unwrap(), expected);
}
#[test]
fn collection_releases_owned_environments() {
    let source = SOURCE.replace("export function main()", "function exercise()");
    let mut live = Vec::new();
    for count in [1, 20] {
        let source=format!("{source}\nexport function main():void {{ for(let i:i32=0;i<{count};i++) {{ exercise(); Context.collect(); }} }}");
        let (_, memory) =
            run_jit_with_memory_accounting(&[SourceFile::new("memory.ts", source)], true).unwrap();
        live.push(memory.live_bytes);
    }
    let control = r#"class Value { n:i32=8; } export function main():void {
        print(`${2} ${3}`);
        for(let i:i32=0;i<10;i++) { print(`${3}`); }
        print(`${4} ${5}`); print(`${6}`);
        const s:string="value"; const a:i32[]=[7]; const v=new Value();
        print(`${s} ${a[0]} ${v.n}`); Context.collect();
    }"#;
    let (_, baseline) =
        run_jit_with_memory_accounting(&[SourceFile::new("control.ts", control)], true).unwrap();
    assert_eq!(live[0], baseline.live_bytes);
    assert_eq!(live[0], live[1]);
    eprintln!("owned environment live bytes: {live:?}");
}

#[test]
fn retired_shapes_match_three_tiers() {
    let source = include_str!("../../corpus/accept/a447-retired-capture-escape-shapes.ts");
    let expected =
        include_bytes!("../../corpus/accept/a447-retired-capture-escape-shapes.expected");
    let files = [SourceFile::new("retired.ts", source)];
    let module = check_program(&files).unwrap();
    assert_eq!(
        interpret(&lower_module(&module).unwrap()).unwrap(),
        expected
    );
    assert_eq!(run_jit(&files).unwrap(), expected);
    assert_eq!(run_c_aot(&files).unwrap(), expected);
}

#[test]
fn callback_identity_changes_reload_declarations() {
    let module = |name: &str| {
        check_program(&[
            SourceFile::ambient("callback.generated.d.ts", "// @subscript-c-header include=\"callback.h\"\n// @subscript-c-callback typedef=\"First\"\n// @subscript-c-callback typedef=\"Second\"\ntype First=(n:i32)=>void; type Second=(n:i32)=>void;"),
            SourceFile::new("reload.ts", format!("function take(cb:{name}):void {{}} export function main():void {{}}")),
        ]).unwrap()
    };
    assert_ne!(
        subscript_codegen::declaration_hash(&module("First")).value(),
        subscript_codegen::declaration_hash(&module("Second")).value(),
    );
    assert_ne!(
        subscript_codegen::declaration_hash(&module("First")).value(),
        subscript_codegen::declaration_hash(&module("(n:i32)=>void")).value(),
    );
}
