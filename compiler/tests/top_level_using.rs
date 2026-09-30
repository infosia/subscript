//! Top-level `using` HIR and diagnostics under compiler.md §139.

use subscript_compiler::{check_program, hir::Stmt, RuleCode, SourceFile};

fn rejected(source: &str, message: &str) {
    let errors = check_program(&[SourceFile::entry("main.ts", source)]).expect_err("S100");
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].code, RuleCode::S100);
    assert_eq!(errors[0].message, message);
}

#[test]
fn top_level_block_and_function_block_carry_the_same_using_form() {
    let module = check_program(&[SourceFile::entry(
        "main.ts",
        "class R { [Symbol.dispose](): void {} } { using r: R = new R(); print('body'); } export function main(): void { { using r: R = new R(); print('body'); } }",
    )]).expect("accepted blocks");
    let Stmt::Block(top) = &module.top_level[0] else {
        panic!("top-level block");
    };
    let main = module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("main");
    let Stmt::Block(function) = &main.body[0] else {
        panic!("function block");
    };
    for body in [top, function] {
        let [Stmt::Let { dispose: true, .. }, Stmt::Using { bindings, body, .. }] = body.as_slice()
        else {
            panic!("one using form: {body:?}");
        };
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].name, "r");
        assert!(matches!(body.as_slice(), [Stmt::Expr(_)]));
    }
}

#[test]
fn top_level_dispose_hook_rejects_a_later_global_read() {
    rejected(
        "class Foo { v: i32 = 7; } class R { [Symbol.dispose](): void { print(`${later.v}`); } } { using r: R = new R(); print('body'); } const later: Foo = new Foo(); export function main(): void {}",
        "`later` is accessed before its declaration, through `R.[[Symbol.dispose]]`",
    );
}

#[test]
fn module_level_using_keeps_its_full_s100_message() {
    rejected(
        "class R { [Symbol.dispose](): void {} } using r: R = new R(); export function main(): void {}",
        "module-level `using` is not in the decided surface",
    );
}

#[test]
fn top_level_blocks_without_using_keep_the_initializer_read_scan() {
    rejected(
        "{ while (true) {} print(`${later}`); } const later: i32 = 7; export function main(): void {}",
        "`later` is accessed before its declaration, directly from this top-level statement",
    );
    let module = check_program(&[SourceFile::entry(
        "main.ts",
        "const later: i32 = 7; { while (true) {} print(`${later}`); } export function main(): void {}",
    )]).expect("global precedes the block");
    let Stmt::Block(body) = &module.top_level[0] else {
        panic!("top-level block");
    };
    assert!(matches!(
        body.as_slice(),
        [Stmt::While { .. }, Stmt::Expr(_)]
    ));
}
