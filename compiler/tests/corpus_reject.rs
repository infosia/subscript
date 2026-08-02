//! Gate test (compiler.md §6): every reject-corpus entry is rejected
//! with its contracted rule code, and the diagnostic points into the
//! entry's file at the line of the offending construct.

use std::fs;
use std::path::PathBuf;

use subscript_compiler::{check_program, RuleCode, SourceFile};

fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../corpus")
}

/// Expected (entry, rule code, 1-based line of the offending construct).
/// Lines are derived from reading the corpus files; r02 and r05 both
/// map to S002 (no dynamic code evaluation).
const EXPECTED: &[(&str, RuleCode, u32)] = &[
    ("r01-any.ts", RuleCode::S001, 7),
    ("r02-eval.ts", RuleCode::S002, 8),
    ("r03-prototype-mutation.ts", RuleCode::S003, 11),
    ("r04-undeclared-property.ts", RuleCode::S004, 13),
    ("r05-new-function.ts", RuleCode::S002, 8),
    ("r06-structural-substitution.ts", RuleCode::S005, 21),
    ("r07-value-class-extends.ts", RuleCode::S006, 13),
    ("r08-bare-number.ts", RuleCode::S007, 7),
    ("r09-int-literal-overflow.ts", RuleCode::S008, 7),
    ("r10-escaping-capture.ts", RuleCode::S009, 9),
    ("r11-throw.ts", RuleCode::S010, 8),
    ("r12-general-union.ts", RuleCode::S011, 8),
    ("r13-undefined.ts", RuleCode::S012, 7),
    ("r16-math-variadic-max.ts", RuleCode::S014, 8),
    ("r18-math-value.ts", RuleCode::S014, 9),
    ("r19-date-local-accessor.ts", RuleCode::S014, 10),
    ("r20-date-setter.ts", RuleCode::S014, 9),
    ("r21-date-multiarg-ctor.ts", RuleCode::S014, 10),
    ("r22-date-template.ts", RuleCode::S014, 11),
    ("r23-date-zero-arg-ctor.ts", RuleCode::S014, 9),
    // tsc accepts `Date === Date`, so r24 lives only in the reject
    // corpus (excluded from tsconfig like every r-entry).
    ("r24-date-compare.ts", RuleCode::S014, 11),
    ("r26-string-localecompare.ts", RuleCode::S014, 10),
    ("r27-string-match.ts", RuleCode::S014, 9),
    ("r28-string-tolocaleupper.ts", RuleCode::S014, 11),
    ("r29-array-sort-noarg.ts", RuleCode::S014, 10),
    ("r30-array-find.ts", RuleCode::S014, 11),
    ("r31-array-reduce-noinit.ts", RuleCode::S014, 11),
    ("r32-array-splice.ts", RuleCode::S014, 10),
    ("r33-narrow-literal-overflow.ts", RuleCode::S008, 7),
    ("r34-narrow-mixed-arithmetic.ts", RuleCode::S007, 9),
    ("r35-narrow-mixed-bitwise.ts", RuleCode::S007, 9),
    ("r36-f16-arithmetic.ts", RuleCode::S014, 9),
    ("r37-literal-overshift.ts", RuleCode::S008, 8),
    ("r38-map-f16-key.ts", RuleCode::S014, 8),
    ("r39-map-array-key.ts", RuleCode::S014, 8),
    ("r40-map-cstruct-key.ts", RuleCode::S014, 16),
    ("r41-map-scalar-get.ts", RuleCode::S014, 9),
    ("r42-map-iterator-member.ts", RuleCode::S014, 9),
    ("r43-map-iterable-constructor.ts", RuleCode::S014, 8),
    ("r44-map-container-key.ts", RuleCode::S014, 8),
    ("r45-map-nested-object-value.ts", RuleCode::S011, 8),
    ("r46-number-global-isnan.ts", RuleCode::S014, 8),
    ("r47-number-coercion.ts", RuleCode::S014, 8),
    ("r48-number-to-precision.ts", RuleCode::S014, 9),
    ("r49-number-to-string-radix.ts", RuleCode::S014, 9),
    ("r50-parse-int-no-radix.ts", RuleCode::S014, 8),
    ("r51-array-unshift-variadic.ts", RuleCode::S014, 10),
    ("r52-object-groupby.ts", RuleCode::S014, 9),
    ("r53-set-algebra-nonset.ts", RuleCode::S014, 10),
    ("r54-map-groupby-key.ts", RuleCode::S014, 9),
    ("r55-array-callback-container.ts", RuleCode::S014, 11),
    ("r56-json-stringify-map.ts", RuleCode::S014, 9),
    ("r57-json-stringify-set.ts", RuleCode::S014, 9),
    ("r58-json-stringify-object.ts", RuleCode::S014, 12),
    ("r59-json-stringify-function.ts", RuleCode::S014, 13),
    ("r60-json-parse-no-context.ts", RuleCode::S014, 8),
    ("r61-json-parse-date.ts", RuleCode::S014, 8),
    (
        "r62-cstruct-fixed-array-layout-too-large.ts",
        RuleCode::S100,
        9,
    ),
    (
        "r63-local-fixed-array-layout-too-large.ts",
        RuleCode::S100,
        8,
    ),
    (
        "r64-nested-fixed-array-layout-too-large.ts",
        RuleCode::S100,
        8,
    ),
    ("r87-literal-union-nonmember.ts", RuleCode::S100, 9),
    ("r88-literal-union-inline.ts", RuleCode::S011, 7),
    ("r89-literal-union-cross-alias.ts", RuleCode::S100, 13),
    ("r90-descriptor-missing-required.ts", RuleCode::S100, 13),
    ("r91-descriptor-excess-member.ts", RuleCode::S004, 13),
    ("r92-literal-for-unmarked-class.ts", RuleCode::S005, 13),
    (
        "r93-descriptor-optional-without-default.ts",
        RuleCode::S012,
        9,
    ),
    ("r94-descriptor-method.ts", RuleCode::S100, 10),
    ("r95-descriptor-new.ts", RuleCode::S100, 14),
    ("r96-new-promise.ts", RuleCode::S013, 8),
    ("r97-promise-combinator.ts", RuleCode::S013, 12),
    ("r98-promise-static.ts", RuleCode::S013, 8),
    ("r99-await-outside-async.ts", RuleCode::S013, 7),
    ("r100-floating-async-call.ts", RuleCode::S013, 13),
    ("r101-async-static-method.ts", RuleCode::S100, 8),
    ("r102-async-generator-method.ts", RuleCode::S100, 8),
    ("r103-async-cstruct-method.ts", RuleCode::S100, 9),
    ("r104-async-generic-class-method.ts", RuleCode::S100, 8),
    ("r105-floating-async-method-call.ts", RuleCode::S013, 16),
    (
        "r106-capturing-lambda-worker-entry.ts",
        RuleCode::S100,
        15,
    ),
    ("r107-async-worker-entry.ts", RuleCode::S100, 20),
    (
        "r108-string-field-worker-message.ts",
        RuleCode::S100,
        8,
    ),
    ("r109-worker-module-global.ts", RuleCode::S100, 18),
    ("r110-new-worker.ts", RuleCode::S100, 13),
    ("r111-worker-in-map-value.ts", RuleCode::S100, 19),
    (
        "r112-switch-alias-missing-member.ts",
        RuleCode::S100,
        12,
    ),
    ("r113-switch-alias-non-member.ts", RuleCode::S100, 16),
    (
        "r114-switch-alias-duplicate-member.ts",
        RuleCode::S100,
        19,
    ),
    ("r115-unreachable-as-value.ts", RuleCode::S100, 9),
    (
        "r116-object-literal-nullable-class.ts",
        RuleCode::S005,
        11,
    ),
    (
        "r65-cstruct-field-offset-layout-too-large.ts",
        RuleCode::S100,
        10,
    ),
    (
        "r66-coroutine-step-layout-too-large.ts",
        RuleCode::S100,
        12,
    ),
    (
        "r67-frame-local-boundary-too-large.ts",
        RuleCode::S100,
        8,
    ),
    (
        "r68-cstruct-stack-frame-too-large.ts",
        RuleCode::S100,
        13,
    ),
    (
        "r69-closure-environment-layout-too-large.ts",
        RuleCode::S100,
        13,
    ),
    (
        "r70-generator-frame-layout-too-large.ts",
        RuleCode::S100,
        9,
    ),
    (
        "r71-accumulated-frame-locals-too-large.ts",
        RuleCode::S100,
        12,
    ),
    ("r72-for-of-user-class.ts", RuleCode::S014, 13),
    ("r73-for-of-object.ts", RuleCode::S014, 12),
    ("r74-for-of-number.ts", RuleCode::S014, 9),
    ("r75-for-of-entries.ts", RuleCode::S014, 9),
    ("r76-return-keys-view.ts", RuleCode::S014, 8),
    ("r77-pass-keys-view.ts", RuleCode::S014, 13),
    ("r78-call-spread-variadic.ts", RuleCode::S014, 13),
    ("r79-assign-entries.ts", RuleCode::S014, 9),
];

const REGEX_EXPECTED: &[(&str, RuleCode, u32)] = &[
    ("r80-regex-exec.ts", RuleCode::S014, 9),
    ("r81-regex-match-all.ts", RuleCode::S014, 9),
    ("r82-regex-last-index.ts", RuleCode::S014, 9),
    ("r83-regex-groups.ts", RuleCode::S014, 8),
    ("r84-regex-sticky-last-index.ts", RuleCode::S014, 8),
    ("r85-invalid-regex-literal.ts", RuleCode::S100, 8),
    (
        "r86-regex-literal-replace-all-without-global.ts",
        RuleCode::S100,
        8,
    ),
];

fn expected_entries() -> Vec<(&'static str, RuleCode, u32)> {
    EXPECTED.iter().chain(REGEX_EXPECTED).copied().collect()
}

#[test]
fn every_reject_entry_fails_with_its_rule_code_at_the_offending_line() {
    let dir = corpus_dir().join("reject");
    for (file, code, line) in expected_entries() {
        let path = dir.join(file);
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {}", path.display(), e));
        let result = check_program(&[SourceFile::new(file, source)]);
        let diags = match result {
            Err(diags) => diags,
            Ok(_) => panic!("{} was accepted; expected {}", file, code),
        };
        assert!(!diags.is_empty(), "{}: empty diagnostic list", file);
        let first = &diags[0];
        assert_eq!(
            first.code, code,
            "{}: expected first diagnostic {}, got {} ({})",
            file, code, first.code, first.message
        );
        assert_eq!(
            first.pos.file, file,
            "{}: diagnostic points at wrong file {}",
            file, first.pos.file
        );
        assert_eq!(
            first.pos.line, line,
            "{}: expected line {}, got {}:{} ({})",
            file, line, first.pos.line, first.pos.col, first.message
        );
    }
}

#[test]
fn json_parse_without_context_is_pinned_to_the_parse_member() {
    let file = "r60-json-parse-no-context.ts";
    let source = fs::read_to_string(corpus_dir().join("reject").join(file))
        .expect("read JSON.parse reject entry");
    let diagnostics = check_program(&[SourceFile::new(file, source)])
        .expect_err("context-free JSON.parse must be rejected");
    assert_eq!(diagnostics[0].code, RuleCode::S014);
    assert_eq!(
        (diagnostics[0].pos.line, diagnostics[0].pos.col),
        (8, 8),
        "S014 must point at the `parse` member"
    );
}

#[test]
fn json_parse_date_rejection_explains_why_the_target_is_unreachable() {
    let file = "r61-json-parse-date.ts";
    let source = fs::read_to_string(corpus_dir().join("reject").join(file))
        .expect("read JSON.parse<Date> reject entry");
    let diagnostics = check_program(&[SourceFile::new(file, source)])
        .expect_err("JSON.parse<Date> must be rejected");
    assert_eq!(diagnostics[0].code, RuleCode::S014);
    assert_eq!(
        (diagnostics[0].pos.line, diagnostics[0].pos.col),
        (8, 8),
        "S014 must point at the `parse` member"
    );
    assert!(
        diagnostics[0]
            .message
            .contains("untagged ISO string cannot identify a Date")
            && diagnostics[0].message.contains("target could never match"),
        "diagnostic must explain the unreachable target: {}",
        diagnostics[0].message
    );
}

#[test]
fn reject_table_covers_every_corpus_entry() {
    assert_eq!(
        expected_entries().len(),
        112,
        "expected 89 standing reject entries, the seven-entry P23 battery, five R13 entries, six Q35 entries, three R14 entries, one R15 entry, and one R17 entry"
    );
    let dir = corpus_dir().join("reject");
    let mut entries: Vec<String> = fs::read_dir(&dir)
        .expect("read corpus/reject")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".ts"))
        .collect();
    let active = expected_entries();
    entries.sort();
    let mut expected: Vec<String> = active.iter().map(|(f, _, _)| f.to_string()).collect();
    expected.sort();
    assert_eq!(entries, expected, "reject corpus and test table disagree");
}

fn first_diagnostic(file: &str) -> subscript_compiler::Diagnostic {
    let source = fs::read_to_string(corpus_dir().join("reject").join(file))
        .unwrap_or_else(|error| panic!("read {file}: {error}"));
    check_program(&[SourceFile::new(file, source)])
        .expect_err("reject entry must fail")
        .into_iter()
        .next()
        .expect("reject entry must produce a diagnostic")
}

#[test]
fn omitted_regex_surface_names_each_language_gap() {
    for (file, needles) in [
        ("r80-regex-exec.ts", &["array", "tuple"][..]),
        ("r27-string-match.ts", &["optional", "i32"][..]),
        ("r81-regex-match-all.ts", &["Q30", "object"][..]),
        ("r82-regex-last-index.ts", &["mutable", "exec"][..]),
        ("r83-regex-groups.ts", &["dynamic keys"][..]),
        (
            "r84-regex-sticky-last-index.ts",
            &["lastIndex", "mutable"][..],
        ),
    ] {
        let diagnostic = first_diagnostic(file);
        assert!(
            needles
                .iter()
                .all(|needle| diagnostic.message.contains(needle)),
            "{file}: diagnostic does not name its language gap: {}",
            diagnostic.message
        );
    }
}

#[test]
fn aggregate_layout_rejections_pin_the_exact_construct_and_limit() {
    let dir = corpus_dir().join("reject");
    for (file, line, col) in [
        ("r62-cstruct-fixed-array-layout-too-large.ts", 9, 9),
        ("r63-local-fixed-array-layout-too-large.ts", 8, 15),
        ("r64-nested-fixed-array-layout-too-large.ts", 8, 17),
        ("r65-cstruct-field-offset-layout-too-large.ts", 10, 3),
        ("r66-coroutine-step-layout-too-large.ts", 12, 23),
    ] {
        let source = fs::read_to_string(dir.join(file))
            .unwrap_or_else(|e| panic!("read {file}: {e}"));
        let diagnostics = check_program(&[SourceFile::new(file, source)])
            .expect_err("oversized aggregate must be rejected");
        assert_eq!(diagnostics[0].code, RuleCode::S100, "{file}");
        assert_eq!(
            (diagnostics[0].pos.line, diagnostics[0].pos.col),
            (line, col),
            "{file}: diagnostic must point at the size-bearing construct"
        );
        assert!(
            diagnostics[0].message.contains("2147483647 bytes"),
            "{file}: diagnostic must state the aggregate byte limit: {}",
            diagnostics[0].message
        );
    }
}

#[test]
fn frame_and_synthesized_aggregate_rejections_are_checker_diagnostics() {
    let dir = corpus_dir().join("reject");
    for (file, line, col, required) in [
        (
            "r67-frame-local-boundary-too-large.ts",
            8,
            9,
            "2147483632 bytes",
        ),
        (
            "r68-cstruct-stack-frame-too-large.ts",
            13,
            9,
            "2147483632 bytes",
        ),
        (
            "r69-closure-environment-layout-too-large.ts",
            13,
            27,
            "closure environment",
        ),
        (
            "r70-generator-frame-layout-too-large.ts",
            9,
            3,
            "generator frame",
        ),
        (
            "r71-accumulated-frame-locals-too-large.ts",
            12,
            9,
            "2147483632 bytes",
        ),
    ] {
        let source = fs::read_to_string(dir.join(file))
            .unwrap_or_else(|e| panic!("read {file}: {e}"));
        let diagnostics = check_program(&[SourceFile::new(file, source)])
            .expect_err("oversized frame/environment must be rejected by the checker");
        assert_eq!(diagnostics[0].code, RuleCode::S100, "{file}");
        assert_eq!(
            (diagnostics[0].pos.line, diagnostics[0].pos.col),
            (line, col),
            "{file}: diagnostic must point at the source construct that crosses the limit"
        );
        assert!(
            diagnostics[0].message.contains(required),
            "{file}: diagnostic must contain {required:?}: {}",
            diagnostics[0].message
        );
    }
}

#[test]
fn frame_limit_is_abi_derived_without_lowering_the_heap_aggregate_limit() {
    let boundary = "\
function probe(input: FixedArray<u8, 2147483632>): void {
  const data: FixedArray<u8, 2147483632> = input;
}
export function main(): void {}
";
    check_program(&[SourceFile::new("boundary.ts", boundary)])
        .expect("the greatest 16-byte-aligned signed-i32 frame must check");

    let heap_only = "\
class RefBig {
  prefix: FixedArray<u8, 2147483640>;
  tag: i32;
}
export function main(): void {}
";
    check_program(&[SourceFile::new("heap-only.ts", heap_only)])
        .expect("a non-stack reference-class allocation retains the aggregate limit");
}

#[test]
fn q27_array_variadic_rejections_name_the_missing_prerequisite() {
    let dir = corpus_dir().join("reject");
    for file in ["r32-array-splice.ts", "r51-array-unshift-variadic.ts"] {
        let source = fs::read_to_string(dir.join(file))
            .unwrap_or_else(|e| panic!("read {file}: {e}"));
        let diagnostics = check_program(&[SourceFile::new(file, source)])
            .expect_err("variadic Array form must be rejected");
        let message = &diagnostics[0].message;
        assert!(
            message.contains("Variadic parameters") && message.contains("missing prerequisite"),
            "{file}: diagnostic does not name the prerequisite: {message}"
        );
    }
}

#[test]
fn q27_map_set_rejections_name_the_missing_language_shapes() {
    let dir = corpus_dir().join("reject");
    for (file, required) in [
        ("r52-object-groupby.ts", "null-prototype object"),
        ("r53-set-algebra-nonset.ts", "no set-like protocol"),
    ] {
        let source = fs::read_to_string(dir.join(file))
            .unwrap_or_else(|e| panic!("read {file}: {e}"));
        let diagnostics = check_program(&[SourceFile::new(file, source)])
            .expect_err("Q27 Map/Set form must be rejected");
        assert!(
            diagnostics[0].message.contains(required),
            "{file}: diagnostic does not explain the missing shape: {}",
            diagnostics[0].message
        );
    }
}

#[test]
fn q27_array_callback_container_rejection_names_c5_and_the_reason() {
    let file = "r55-array-callback-container.ts";
    let path = corpus_dir().join("reject").join(file);
    let source =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let diagnostics = check_program(&[SourceFile::new(file, source)])
        .expect_err("the Array callback container parameter must be rejected");
    let message = &diagnostics[0].message;
    for required in ["C5", "container reference", "non-escaping-by-construction"] {
        assert!(
            message.contains(required),
            "{file}: diagnostic does not name `{required}`: {message}"
        );
    }
}

#[test]
fn q30_rejections_name_the_actual_missing_prerequisite() {
    let dir = corpus_dir().join("reject");
    for (file, required) in [
        (
            "r72-for-of-user-class.ts",
            &["invariant 5", "Symbol.iterator", "stock `tsc`"][..],
        ),
        (
            "r43-map-iterable-constructor.ts",
            &["pair", "no tuple type"][..],
        ),
        ("r75-for-of-entries.ts", &["pair", "no tuple type"][..]),
        ("r79-assign-entries.ts", &["pair", "no tuple type"][..]),
        (
            "r42-map-iterator-member.ts",
            &["direct subject", "stateful iterator", "outlives"][..],
        ),
        (
            "r76-return-keys-view.ts",
            &["direct subject", "stateful iterator", "outlives"][..],
        ),
        (
            "r77-pass-keys-view.ts",
            &["direct subject", "stateful iterator", "outlives"][..],
        ),
        ("r78-call-spread-variadic.ts", &["variadic parameters"][..]),
    ] {
        let source = fs::read_to_string(dir.join(file))
            .unwrap_or_else(|e| panic!("read {file}: {e}"));
        let diagnostics = check_program(&[SourceFile::new(file, source)])
            .expect_err("Q30 rejection must fail");
        let message = &diagnostics[0].message;
        for needle in required {
            assert!(
                message.contains(needle),
                "{file}: diagnostic does not name {needle:?}: {message}"
            );
        }
    }
}
