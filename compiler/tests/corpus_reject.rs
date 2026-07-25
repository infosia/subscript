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
    ("r14-async.ts", RuleCode::S013, 7),
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
    ("r27-string-match.ts", RuleCode::S014, 10),
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
    ("r52-object-groupby.ts", RuleCode::S014, 8),
    ("r53-set-algebra-nonset.ts", RuleCode::S014, 9),
    ("r54-map-groupby-key.ts", RuleCode::S014, 8),
    ("r55-array-callback-container.ts", RuleCode::S014, 10),
    ("r56-json-stringify-map.ts", RuleCode::S014, 8),
    ("r57-json-stringify-set.ts", RuleCode::S014, 8),
    ("r58-json-stringify-object.ts", RuleCode::S014, 11),
    ("r59-json-stringify-function.ts", RuleCode::S014, 12),
];

#[test]
fn every_reject_entry_fails_with_its_rule_code_at_the_offending_line() {
    let dir = corpus_dir().join("reject");
    for (file, code, line) in EXPECTED {
        let path = dir.join(file);
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {}", path.display(), e));
        let result = check_program(&[SourceFile::new(*file, source)]);
        let diags = match result {
            Err(diags) => diags,
            Ok(_) => panic!("{} was accepted; expected {}", file, code),
        };
        assert!(!diags.is_empty(), "{}: empty diagnostic list", file);
        let first = &diags[0];
        assert_eq!(
            first.code, *code,
            "{}: expected first diagnostic {}, got {} ({})",
            file, code, first.code, first.message
        );
        assert_eq!(
            first.pos.file, *file,
            "{}: diagnostic points at wrong file {}",
            file, first.pos.file
        );
        assert_eq!(
            first.pos.line, *line,
            "{}: expected line {}, got {}:{} ({})",
            file, line, first.pos.line, first.pos.col, first.message
        );
    }
}

#[test]
fn reject_table_covers_every_corpus_entry() {
    let dir = corpus_dir().join("reject");
    let mut entries: Vec<String> = fs::read_dir(&dir)
        .expect("read corpus/reject")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".ts"))
        .collect();
    entries.sort();
    let mut expected: Vec<String> = EXPECTED.iter().map(|(f, _, _)| f.to_string()).collect();
    expected.sort();
    assert_eq!(entries, expected, "reject corpus and test table disagree");
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
