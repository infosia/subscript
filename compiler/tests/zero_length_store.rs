//! A zero length store uses one clear operation and one rejection site (§174).

use subscript_compiler::{check_program, divergence::Divergence, hir, SourceFile};

#[test]
fn only_a_literal_zero_statement_clears_a_dynamic_array() {
    for form in ["xs.length = 0;", "(xs.length = 0);", "xs.pop();"] {
        let source = format!("export function main(): void {{ const xs: i32[] = [1]; {form} }}");
        assert!(
            check_program(&[SourceFile::new("zero.ts", source)]).is_ok(),
            "{form}"
        );
    }
    for form in [
        "xs.length = 2;",
        "xs.length -= 1;",
        "const n = (xs.length = 0);",
        "const zero: i32 = 0; xs.length = zero;",
        "xs.length++;",
        "xs.length = -0;",
        "xs.length = 0.0;",
    ] {
        let source = format!("export function main(): void {{ const xs: i32[] = [1]; {form} }}");
        let errors = check_program(&[SourceFile::new("length.ts", source)]).expect_err(form);
        assert_eq!(errors.len(), 1, "{form}: {errors:?}");
        assert_eq!(
            errors[0].divergence,
            Some(Divergence::ArrayLengthStore),
            "{form}"
        );
        assert!(errors[0]
            .message
            .starts_with("only `xs.length = 0` as a statement is accepted"));
    }
}

#[test]
fn a_class_length_field_and_fixed_array_keep_their_own_rules() {
    let class = "class C { length: i32 = 1; } export function main(): void { const c = new C(); c.length = 0; const n = (c.length = 2); }";
    assert!(check_program(&[SourceFile::new("class.ts", class)]).is_ok());
    let fixed =
        "export function main(): void { const xs: FixedArray<i32, 1> = [1]; xs.length = 0; }";
    let errors = check_program(&[SourceFile::new("fixed.ts", fixed)]).expect_err("fixed store");
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].divergence, None);
    assert!(errors[0].message.contains("FixedArray surface"));
}

#[test]
fn clear_has_a_void_signature_and_a_single_receiver() {
    let source = "export function main(): void { const xs: i32[] = [1]; xs.length = 0; }";
    let module = check_program(&[SourceFile::new("clear.ts", source)]).expect("zero store");
    let signature = module
        .operation_signatures
        .iter()
        .find(|signature| {
            signature.target
                == hir::OperationSignatureTarget::BuiltinMethod(hir::BuiltinMethod::ArrayClear)
        })
        .expect("clear signature");
    assert_eq!(
        signature.parameter_types,
        vec![subscript_compiler::Type::Array(Box::new(
            subscript_compiler::Type::I32
        ))]
    );
    assert_eq!(signature.return_type, None);
    let fragment = Divergence::ArrayLengthStore.entry();
    assert_eq!(fragment.collision, "stdlib.md §9.12");
    assert!(fragment.why.contains("hole"));
}
