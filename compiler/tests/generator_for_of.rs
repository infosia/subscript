//! Generator iteration retains its source form for exit lowering (§180).
//! Cost: one checker fixture takes 0.01 seconds in a debug run.

use subscript_compiler::{check_program, hir, SourceFile, Type};

#[test]
fn generator_for_of_keeps_the_subject_binding_and_body() {
    let module = check_program(&[SourceFile::new(
        "generator-loop.ts",
        "function* source(): Generator<i32> { yield 1; }\n\
         export function main(): void { for (let element of source()) { element = 2; break; } }",
    )])
    .expect("generator loop checks");
    let main = module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("main function");
    let hir::Stmt::GeneratorForOf {
        name,
        ty,
        mutable,
        subject,
        body,
        pos,
    } = &main.body[0]
    else {
        panic!("the checker must retain generator iteration");
    };
    assert_eq!(name, "element");
    assert_eq!(*ty, Type::I32);
    assert!(*mutable);
    assert!(matches!(subject.kind, hir::ExprKind::Call { .. }));
    assert_eq!(subject.ty, Type::Generator(Box::new(Type::I32)));
    assert_eq!(body.len(), 2);
    assert!(matches!(body[1], hir::Stmt::Break(_)));
    assert_eq!(pos.line, 2);
    let children = main.body[0].children();
    assert_eq!(children.len(), 3);
    assert!(matches!(children[0], hir::HirChild::Expr(expression) if expression == subject));
    assert!(module.operation_signatures.iter().any(|signature| {
        signature.target
            == hir::OperationSignatureTarget::BuiltinMethod(hir::BuiltinMethod::GeneratorNext)
            && signature.parameter_types == [subject.ty.clone()]
            && signature.return_type == Some(Type::iter_result(Type::I32))
    }));
}
