//! Fact splits and same-shape controls for compiler.md §173.

use super::{rejection::RejectionSite, rejection_total};
use crate::{check_program, Diagnostic, SourceFile};

fn rejected(source: &str, site: &str, variant: Option<&str>) -> Vec<Diagnostic> {
    rejection_total::clear_reached();
    let files = [SourceFile::entry("fact.ts", source)];
    let diagnostics = check_program(&files).expect_err("the source must reject");
    let reached = rejection_total::take_reached();
    let first = &diagnostics[0];
    assert!(
        reached.iter().any(|(actual, message, pos)| {
            format!("{actual:?}") == site && *message == first.message && *pos == first.pos
        }),
        "{site}: {reached:?}"
    );
    assert_eq!(
        first.divergence.map(|v| format!("{v:?}")),
        variant.map(str::to_owned)
    );
    diagnostics
}

#[test]
fn error_members_split_from_missing_members() {
    for constructor in ["Error", "TypeError", "RangeError"] {
        for member in ["stack", "cause"] {
            let source = format!("export function main(): void {{ const e = new {constructor}(\"x\"); print(`${{e.{member}}}`); }}");
            rejected(
                &source,
                "ErrorMemberOutsideSurface",
                Some("ErrorMemberOutsideSurface"),
            );
        }
    }
    rejected(
        "export function main(): void { const e = new Error(\"x\"); print(`${e.missing}`); }",
        "ClassUndeclaredMemberRead",
        None,
    );
    rejected("class Error { x: i32 = 1; } export function main(): void { print(`${new Error().stack}`); }", "ClassUndeclaredMemberRead", None);
}

#[test]
fn error_member_writes_use_builtin_identity() {
    for constructor in ["Error", "TypeError", "RangeError"] {
        for member in ["stack", "cause"] {
            let source = format!("export function main(): void {{ const e = new {constructor}(\"x\"); e.{member} = \"s\"; }}");
            rejected(
                &source,
                "ErrorMemberOutsideSurface",
                Some("ErrorMemberOutsideSurface"),
            );
        }
    }
    rejected(
        "export function main(): void { const e = new Error(\"x\"); e.missing = \"s\"; }",
        "ClassUndeclaredPropertyWrite",
        None,
    );
    rejected("class Error { x: i32 = 1; } export function main(): void { const e = new Error(); e.stack = \"s\"; }", "ClassUndeclaredPropertyWrite", None);
}

#[test]
fn map_pairs_reject_before_array_inference() {
    for args in ["<string, i32>", ""] {
        let source =
            format!("export function main(): void {{ const m = new Map{args}([[\"a\", 1]]); }}");
        let diagnostics = rejected(
            &source,
            "Api(FormNewMapIterable, NoTupleType)",
            Some("NoTupleType"),
        );
        assert_eq!(diagnostics.len(), 1);
    }
    for pairs in [
        "[[\"a\", \"b\"]]",
        "[[1, 2]]",
        "[[\"a\", 1], [\"b\", \"c\"]]",
    ] {
        let source =
            format!("export function main(): void {{ const m = new Map<string, i32>({pairs}); }}");
        let diagnostics = rejected(&source, "AssignmentTypeMismatch", None);
        assert_eq!(diagnostics.len(), 1);
    }
    for pairs in [
        "[[\"a\", null], [\"b\", 1]]",
        "[[\"a\", null], [\"b\", null], [\"c\", 1]]",
        "[[\"a\", 1], [\"b\", null]]",
        "[[\"a\", 1], [\"b\", 1.5]]",
    ] {
        let source = format!("export function main(): void {{ const m = new Map({pairs}); }}");
        rejected(
            &source,
            "Api(FormNewMapIterable, NoTupleType)",
            Some("NoTupleType"),
        );
    }
    rejected(
        "export function main(): void { const pair = [\"a\", 1]; }",
        "AssignmentTypeMismatch",
        None,
    );
    assert!(check_program(&[SourceFile::entry("copy.ts", "export function main(): void { const m = new Map<string, i32>(); const copy = new Map(m); }")]).is_ok());
}

#[test]
fn nested_nominality_splits_at_every_depth() {
    for (source, destination) in [
        ("Q[]", "P[]"),
        ("Q[][]", "P[][]"),
        ("() => Q", "() => P"),
        ("(v: Q) => i32", "(v: P) => i32"),
        ("() => Q[]", "() => P[]"),
        ("((v: Q[]) => i32)[]", "((v: P[]) => i32)[]"),
    ] {
        for (field, site, variant) in [
            ("i32 = 1", "NestedNominalClass", Some("NestedNominalClass")),
            ("string = \"x\"", "AssignmentTypeMismatch", None),
        ] {
            let program = format!("class P {{ x: i32 = 1; }} class Q {{ x: {field}; }} function f(q: {source}): void {{ const p: {destination} = q; }} export function main(): void {{}}");
            rejected(&program, site, variant);
        }
    }
}

#[test]
fn nested_private_identity_keeps_the_type_mismatch() {
    for (p, q) in [
        ("private", "private"),
        ("protected", "protected"),
        ("public", "private"),
        ("private", "public"),
    ] {
        let source = format!("class P {{ {p} x: i32 = 1; }} class Q {{ {q} x: i32 = 1; }} function f(q: Q[]): void {{ const p: P[] = q; }} export function main(): void {{}}");
        rejected(&source, "AssignmentTypeMismatch", None);
        let source = format!("class P {{ {p} f(): i32 {{ return 1; }} }} class Q {{ {q} f(): i32 {{ return 1; }} }} function f(q: Q[]): void {{ const p: P[] = q; }} export function main(): void {{}}");
        rejected(&source, "AssignmentTypeMismatch", None);
    }
}

#[test]
fn constrained_scalar_keeps_its_existing_assignment_site() {
    rejected("class P { x: i32 = 1; } class Q { x: i32 = 1; } function f<T extends Q>(q: T): void { const p: P = q; } export function main(): void {}", "AssignmentTypeMismatch", None);
}

#[test]
fn abstract_property_names_the_member() {
    let diagnostics = rejected(
        "abstract class P { abstract x: i32; get(): i32 { return this.x; } } export function main(): void {}",
        "AbstractMember",
        Some("AbstractMember"),
    );
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(
        diagnostics[0].message,
        "abstract member `x` of `P` is not supported because class inheritance is rejected"
    );
    rejected(
        "class P { x: i32; } export function main(): void {}",
        "FieldAssignmentMissingUnassignedExit",
        None,
    );
}

#[test]
fn static_arrow_this_names_the_class_remedy() {
    let diagnostics = rejected("class N { static f(): void { const g = (): void => { const self = this; }; g(); } } export function main(): void {}", "ThisInStaticMethodArrow", Some("ThisStaticMethodMember"));
    assert_eq!(
        diagnostics[0].message,
        "a static method must name its class instead of `this`; use `ClassName.member`"
    );
    assert!(check_program(&[SourceFile::entry("instance.ts", "class N { f(): void { const g = (): void => { const self = this; }; g(); } } export function main(): void {}")]).is_ok());
    assert_eq!(
        RejectionSite::ThisInStaticMethodArrow
            .class()
            .1
            .divergence(),
        diagnostics[0].divergence
    );
}

#[test]
fn catch_property_splits_from_template_use() {
    for expression in [
        "e.message",
        "(e).message",
        "e[\"message\"]",
        "e.toString()",
        "e?.message",
        "(e)?.message",
        "e?.[\"message\"]",
        "e?.message.length",
    ] {
        let program = format!("export function main(): void {{ try {{ throw new Error(\"x\"); }} catch (e) {{ print({expression}); }} }}");
        rejected(&program, "CatchBindingUnnarrowedProperty", None);
    }
    let diagnostics = rejected("export function main(): void { try { throw new Error(\"x\"); } catch (e) { const read = async (): Promise<void> => { print(e.message); }; } }", "CatchBindingUnnarrowedProperty", None);
    assert_eq!(diagnostics.len(), 1);
    rejected("export function main(): void { try { throw new Error(\"x\"); } catch (e) { print(`${e}`); } }", "CatchBindingUnnarrowedUse", Some("Exceptions"));
    assert!(check_program(&[SourceFile::entry("narrowed.ts", "export function main(): void { try { throw new Error(\"x\"); } catch (e) { if (e instanceof Error) { print(e.message); } } }")]).is_ok());
}

#[test]
fn void_value_splits_on_the_destination_type() {
    rejected(
        "function f(): void {} export function main(): void { const x: i32 = f(); }",
        "VoidExpressionNonVoidDestination",
        None,
    );
    rejected(
        "function f(): void {} export function main(): void { const x = f(); }",
        "VoidExpressionValue",
        Some("VoidValue"),
    );
    assert!(check_program(&[SourceFile::entry(
        "statement.ts",
        "function f(): void {} export function main(): void { f(); }"
    )])
    .is_ok());
}

#[test]
fn pending_local_splits_on_the_lambda_boundary() {
    rejected(
        "export function main(): void { print(`${x}`); const x: i32 = 1; }",
        "ImmediatePendingLocalRead",
        None,
    );
    rejected("export function main(): void { const read = (): i32 => x; const x: i32 = 4; print(`${read()}`); }", "BlockPendingReadWithoutProgramShadow", Some("DeclarationScope"));
}
