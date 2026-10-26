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

#[test]
fn s180_finally_keeps_enum_switch_return_coverage_site() {
    let switch = "switch (e) { case E.A: return 1; case E.B: return 2; }";
    let probe = |body: &str| {
        let source = format!("enum E {{ A, B }} function f(e: E): i32 {{ {body} }} export function main(): void {{}}");
        rejection_total::clear_reached();
        let diagnostics = check_program(&[SourceFile::entry("coverage.ts", source)])
            .expect_err("the subset requires an explicit default return");
        let reached = rejection_total::take_reached();
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        reached
            .into_iter()
            .find(|(_, message, pos)| {
                *message == diagnostics[0].message && *pos == diagnostics[0].pos
            })
            .expect("the diagnostic carries its rejection site")
            .0
    };
    let plain = probe(switch);
    assert_eq!(plain, RejectionSite::FunctionReturnCoverage);
    let wrapped = probe(&format!(
        "try {{ {switch} }} finally {{ print(\"done\"); }}"
    ));
    assert_eq!(wrapped, plain);
    let finalizer = probe(&format!(
        "try {{ print(\"body\"); }} finally {{ {switch} }}"
    ));
    assert_eq!(finalizer, plain);
}

type CheckerObserver = Box<dyn Fn(&super::Checker<'_>)>;

thread_local! {
    static OBSERVER: std::cell::RefCell<Option<CheckerObserver>> =
        const { std::cell::RefCell::new(None) };
}

/// Calls the observer that the current test installed, after the bodies
/// of the program are checked.
pub(super) fn observe_checker(checker: &super::Checker<'_>) {
    OBSERVER.with_borrow(|observer| {
        if let Some(observer) = observer {
            observer(checker);
        }
    });
}

/// Checks `source` and gives what `observe` reads from each checker run.
fn observed<T: 'static>(
    source: &str,
    observe: impl Fn(&super::Checker<'_>) -> T + 'static,
) -> Vec<T> {
    let results = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let sink = std::rc::Rc::clone(&results);
    OBSERVER.with_borrow_mut(|observer| {
        *observer = Some(Box::new(move |checker| {
            sink.borrow_mut().push(observe(checker));
        }));
    });
    let checked = check_program(&[SourceFile::entry("fact.ts", source)]);
    OBSERVER.with_borrow_mut(|observer| *observer = None);
    checked.expect("the observed source must check");
    std::rc::Rc::try_unwrap(results)
        .ok()
        .expect("the observer is dropped")
        .into_inner()
}

/// The pair `Box | undefined`, `Box | null` of the checked program.
fn spelled_boxes(checker: &super::Checker<'_>) -> (crate::types::Type, crate::types::Type) {
    use crate::types::{Absence, ClassId, Type};
    let index = checker
        .classes
        .iter()
        .position(|class| class.name == "Box")
        .expect("the program declares `Box`");
    let class = Type::Class(ClassId(index));
    (
        Type::nullable_spelled(class.clone(), Absence::Undefined),
        Type::nullable_spelled(class, Absence::Null),
    )
}

const BOX_PROGRAM: &str = "class Box { v: i32 = 1; }\nexport function main(): void {}\n";

#[test]
fn the_tsc_model_converts_within_a_spelling_and_compares_across_spellings() {
    // compiler.md §195.11 rule 3: `tsc` rejects the conversion (TS2322) in
    // each direction, and accepts `u === n`.
    let runs = observed(BOX_PROGRAM, |checker| {
        let (undefined, null) = spelled_boxes(checker);
        [
            checker.ts_erased_assignable(&undefined, &null),
            checker.ts_erased_assignable(&null, &undefined),
            checker.ts_nominal_assignable(&undefined, &null),
            checker.ts_nominal_assignable(&null, &undefined),
            checker.ts_erased_comparable(&undefined, &null),
            checker.ts_erased_comparable(&null, &undefined),
            // Control: the same spelling converts.
            checker.ts_erased_assignable(&undefined, &undefined),
            checker.ts_erased_assignable(&null, &null),
        ]
    });
    assert!(!runs.is_empty());
    for run in runs {
        assert_eq!(
            run,
            [false, false, false, false, true, true, true, true],
            "conversion x4, comparison x2, same-spelling controls x2"
        );
    }
}

#[test]
fn a_conversion_across_spellings_is_a_spelling_mismatch_only() {
    use crate::types::Type;
    // compiler.md §195.1 rule 2: the pair differs only in the spelling.
    let runs = observed(BOX_PROGRAM, |checker| {
        let (undefined, null) = spelled_boxes(checker);
        let class = checker.non_null_type(&null);
        [
            checker.converts_across_spellings(&undefined, &null),
            checker.converts_across_spellings(&null, &undefined),
            // Controls: one spelling, a non-nullable side, and another inner type.
            checker.converts_across_spellings(&null, &null),
            checker.converts_across_spellings(&undefined, &class),
            checker.converts_across_spellings(&class, &null),
            checker.converts_across_spellings(&undefined, &Type::nullable(Type::Str)),
        ]
    });
    assert!(!runs.is_empty());
    for run in runs {
        assert_eq!(run, [true, true, false, false, false, false]);
    }
}

#[test]
fn a_conversion_across_spellings_has_its_own_site_without_a_block() {
    rejected(
        "class Box { v: i32 = 1; }\nfunction pick(): Box | undefined { return undefined; }\nexport function main(): void { const b: Box | null = pick(); print(`${b === null}`); }",
        "ConversionAcrossSpellings",
        None,
    );
}

#[test]
fn a_generic_join_keeps_one_absence_spelling() {
    use crate::types::Type;
    // compiler.md §195.11 rule 5: `T` and `T | s` join to `T | s`, and the
    // `null` literal and `T | null` join to `T | null`.
    let runs = observed(BOX_PROGRAM, |checker| {
        let (undefined, null) = spelled_boxes(checker);
        let class = checker.non_null_type(&null);
        let joined = [
            checker.generic_union(&class, &undefined),
            checker.generic_union(&undefined, &class),
            checker.generic_union(&null, &class),
            checker.generic_union(&Type::Null, &null),
            checker.generic_union(&null, &Type::Null),
            checker.generic_union(
                &Type::GenericUnion(vec![class.clone(), Type::Null].into_boxed_slice()),
                &null,
            ),
        ];
        // Controls: two spellings, two types, and no `T | s` member stay a union.
        let union = |members: Vec<Type>| Type::GenericUnion(members.into_boxed_slice());
        let kept = [
            (
                checker.generic_union(&undefined, &null),
                union(vec![undefined.clone(), null.clone()]),
            ),
            (
                checker.generic_union(&Type::Null, &undefined),
                union(vec![Type::Null, undefined.clone()]),
            ),
            (
                checker.generic_union(&Type::nullable(Type::Str), &class),
                union(vec![Type::nullable(Type::Str), class.clone()]),
            ),
            (
                checker.generic_union(&class, &Type::Null),
                union(vec![class.clone(), Type::Null]),
            ),
        ];
        (undefined, null, joined, kept)
    });
    assert!(!runs.is_empty());
    for (undefined, null, joined, kept) in runs {
        assert_eq!(
            joined,
            [
                undefined.clone(),
                undefined,
                null.clone(),
                null.clone(),
                null.clone(),
                null
            ]
        );
        for (actual, expected) in kept {
            assert_eq!(actual, expected);
        }
    }
}
