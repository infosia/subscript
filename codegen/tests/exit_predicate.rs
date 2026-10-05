//! Literal branch and function-exit comparisons for §164.
use subscript_codegen::lir::lower_module;
use subscript_compiler::{check_program, lir::Terminator, SourceFile};

#[test]
fn literal_branches_have_one_edge_and_variable_branches_have_two() {
    for (body, block_name, edges) in [
        ("while (true) { return; }", "while.cond", 1),
        ("while (false) { return; }", "while.cond", 1),
        ("for (;true;) { return; }", "for.cond", 1),
        ("for (;false;) { return; }", "for.cond", 1),
        ("if (true) { return; }", "entry", 1),
        ("if (false) { return; }", "entry", 1),
        ("while (x > 0) { return; }", "while.cond", 2),
        ("for (;x > 0;) { return; }", "for.cond", 2),
        ("if (x > 0) { return; }", "entry", 2),
    ] {
        let module = check_program(&[SourceFile::new(
            "branches.ts",
            format!("function probe(x:i32):void {{ {body} }}"),
        )])
        .unwrap();
        let lir = lower_module(&module).unwrap();
        let function = lir
            .functions
            .iter()
            .find(|f| f.source_name == "probe")
            .unwrap();
        let block = function
            .blocks
            .iter()
            .find(|b| b.source_name.as_deref() == Some(block_name))
            .unwrap();
        assert_eq!(block.terminator.successors().len(), edges, "{body}");
        assert_eq!(
            matches!(block.terminator, Terminator::ConditionalBranch { .. }),
            edges == 2
        );
    }
}

#[test]
fn every_body_kind_supports_the_exit_comparison() {
    let source = "class C { constructor() { if (true) { return; } } f():void { if (true) { return; } } } function* gen():Generator<i32> { while(true) { yield 1; } } async function task():Promise<void> { if(true) { return; } } function probe():void { const h=():void=>{ if(true) { return; } }; h(); try { return; } catch(e) {} }";
    let module = check_program(&[SourceFile::new("kinds.ts", source)]).unwrap();
    lower_module(&module).unwrap();
}

#[test]
fn false_loop_bodies_and_variable_controls_agree_on_all_tiers() {
    // One C compilation covers both loop forms, both result types, and their controls.
    let source = r#"
function wf(x:i32):i32 { while(false) { x += 1; } return x; }
function ff(x:i32):i32 { for(;false;x+=10) { x += 1; } return x; }
function wv(x:i32):void { while(false) { x += 1; } print(`${x}`); }
function fv(x:i32):void { for(;false;x+=10) { x += 1; } print(`${x}`); }
function wc(x:i32):i32 { while(x<3) { x += 1; } return x; }
function fc(x:i32):i32 { for(;x<3;x+=1) { x += 1; } return x; }
function wvc(x:i32):void { while(x<3) { x += 1; } print(`${x}`); }
function fvc(x:i32):void { for(;x<3;x+=1) { x += 1; } print(`${x}`); }
export function main():void { print(`${wf(1)}`); print(`${ff(1)}`); wv(1); fv(1); print(`${wc(1)}`); print(`${fc(1)}`); wvc(1); fvc(1); }
"#;
    let files = [SourceFile::entry("false-loops.ts", source)];
    let expected = b"1\n1\n1\n1\n3\n3\n3\n3\n";
    let module = check_program(&files).unwrap();
    let lir = lower_module(&module).unwrap();
    assert_eq!(
        subscript_codegen::interpreter::interpret(&lir).unwrap(),
        expected
    );
    assert_eq!(subscript_codegen::run_jit(&files).unwrap(), expected);
    assert_eq!(subscript_codegen::run_c_aot(&files).unwrap(), expected);
}
