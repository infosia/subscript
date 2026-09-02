use subscript_compiler::{
    check_program, divergence::Divergence, hir, Diagnostic, RuleCode, SourceFile,
};

const SUPPORT: &str = "class Box {\n\
                      \x20 v: i32;\n\
                      \x20 constructor(v: i32) { this.v = v; }\n\
                      }\n\
                      function maybe(keep: boolean): Box | null {\n\
                      \x20 return keep ? new Box(1) : null;\n\
                      }\n";

fn diagnostics(source: &str) -> Vec<Diagnostic> {
    check_program(&[SourceFile::new("test.ts", source)]).expect_err("the program must fail")
}

#[test]
fn a_non_place_switch_case_test_matches_the_initializer_rejection() {
    let initializer = diagnostics(&format!(
        "{SUPPORT}class Holder {{\n\
         \x20 value: i32 = (maybe(false) ?? new Box(1)).v;\n\
         }}\n"
    ));
    let switch_case = diagnostics(&format!(
        "{SUPPORT}export function main(): void {{\n\
         \x20 const fb: Box = new Box(1);\n\
         \x20 switch (0) {{\n\
         \x20   case (maybe(false) ?? fb).v: break;\n\
         \x20   default: break;\n\
         \x20 }}\n\
         }}\n"
    ));

    assert_eq!(initializer.len(), 1, "diagnostics: {initializer:?}");
    assert_eq!(switch_case.len(), 1, "diagnostics: {switch_case:?}");
    assert_eq!(switch_case[0].code, RuleCode::S100);
    assert_eq!(switch_case[0].code, initializer[0].code);
    assert_eq!(switch_case[0].message, initializer[0].message);
    assert_eq!(
        switch_case[0].divergence,
        Some(Divergence::NonPlaceNullishInitializer)
    );
    assert_eq!(switch_case[0].divergence, initializer[0].divergence);
}

#[test]
fn switch_case_tests_without_a_prefix_check_clean() {
    let source = format!(
        "{SUPPORT}function five(): i32 {{ return 5; }}\n\
         export function main(): void {{\n\
         \x20 const fb: Box = new Box(1);\n\
         \x20 switch (0) {{\n\
         \x20   case fb.v: break;\n\
         \x20   case five(): break;\n\
         \x20   default: break;\n\
         \x20 }}\n\
         }}\n"
    );
    check_program(&[SourceFile::new("test.ts", source)])
        .expect("case tests without a synthetic prefix must check");
}

#[test]
fn a_for_condition_prefix_stays_at_the_head_of_the_while_body() {
    let source = format!(
        "{SUPPORT}export function main(): void {{\n\
         \x20 const fb: Box = new Box(1);\n\
         \x20 let count: i32 = 0;\n\
         \x20 for (let i: i32 = 0; i < 3 && (maybe(true) ?? fb).v > 0; i++) {{ count++; }}\n\
         }}\n"
    );
    let module =
        check_program(&[SourceFile::new("test.ts", source)]).expect("the for condition must check");
    let main = module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("main must exist");
    let hir::Stmt::Block(loop_block) = &main.body[2] else {
        panic!("the rewritten for must be a block: {:?}", main.body[2]);
    };
    let [hir::Stmt::Let { name: init, .. }, hir::Stmt::While { body, .. }] = loop_block.as_slice()
    else {
        panic!("the block must own the initializer and while: {loop_block:?}");
    };
    assert_eq!(init, "i");
    let [hir::Stmt::Let { name: prefix, .. }, hir::Stmt::If { .. }] = body.as_slice() else {
        panic!("the while body must start with the condition prefix: {body:?}");
    };
    assert!(prefix.starts_with("[["), "synthetic local: {prefix}");
}

#[test]
fn a176_and_a177_shapes_have_no_diagnostics() {
    for (name, source) in [
        (
            "a176-compound-through-accessor.ts",
            include_str!("../../corpus/accept/a176-compound-through-accessor.ts"),
        ),
        (
            "a177-nullish.ts",
            include_str!("../../corpus/accept/a177-nullish.ts"),
        ),
    ] {
        check_program(&[SourceFile::new(name, source)])
            .unwrap_or_else(|diagnostics| panic!("{name} must check: {diagnostics:?}"));
    }
}
