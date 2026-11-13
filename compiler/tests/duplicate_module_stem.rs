//! Program input checks for compiler.md §267 rules 1 and 2.

use subscript_compiler::{check_program, RuleCode, SourceFile};

fn duplicate_sources() -> Vec<SourceFile> {
    vec![
        SourceFile::new(
            "one/foo.ts",
            "print('one'); export function value(): i32 { return 1; }",
        ),
        SourceFile::new(
            "two/foo.ts",
            "print('two'); export function value(): i32 { return 2; }",
        ),
        SourceFile::entry(
            "main.ts",
            "import { value } from './foo'; print(`${value()}`);",
        ),
    ]
}

#[test]
fn duplicate_stem_reports_the_second_file() {
    let result = check_program(&duplicate_sources());
    let errors = result.expect_err("duplicate module stem");
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code, RuleCode::S100);
    assert_eq!(
        errors[0].message,
        "duplicate module stem `foo`: `one/foo.ts` and `two/foo.ts`"
    );
    assert_eq!(errors[0].pos.file, "two/foo.ts");
    assert_eq!((errors[0].pos.line, errors[0].pos.col), (1, 1));
    assert_eq!(errors[0].divergence, None);
}

#[test]
fn different_stems_in_different_directories_check() {
    let mut files = duplicate_sources();
    files[1].name = "two/bar.ts".into();
    files[2].source.push_str("export function main(): void {}");
    let module = check_program(&files).expect("distinct stems");
    assert_eq!(
        module.initializer_modules,
        ["one/foo.ts", "main.ts", "two/bar.ts"]
    );
}

#[test]
fn ambient_files_can_share_a_stem() {
    check_program(&[
        SourceFile::ambient("one/foo.d.ts", ""),
        SourceFile::ambient("two/foo.d.ts", ""),
        SourceFile::entry("main.ts", "print('main');"),
    ])
    .expect("ambient files keep their input rules");
}

#[test]
fn duplicate_stem_precedes_parser_errors() {
    let errors = check_program(&[
        SourceFile::new("one/foo.ts", "const ="),
        SourceFile::new("two/foo.ts", "const ="),
    ])
    .expect_err("input guard precedes parsing");
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code, RuleCode::S100);
    assert_eq!(
        errors[0].message,
        "duplicate module stem `foo`: `one/foo.ts` and `two/foo.ts`"
    );
}
