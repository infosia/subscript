//! Module keys and path resolution: compiler.md §354 rules 2, 3, 5, and 7,
//! and §267 rule 2.

use subscript_compiler::{check_program, check_program_with, CheckOptions, RuleCode, SourceFile};

fn same_base_name_sources() -> Vec<SourceFile> {
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
            "import { value } from './one/foo'; import './two/foo'; print(`${value()}`); export function main(): void {}",
        ),
    ]
}

// Cost (debug): below 0.01 s; one check, without subprocesses.
#[test]
fn two_files_with_one_base_name_are_two_modules() {
    let module = check_program(&same_base_name_sources()).expect("two keys");
    assert_eq!(
        module.initializer_modules,
        ["one/foo.ts", "two/foo.ts", "main.ts"]
    );
    let value = module
        .functions
        .iter()
        .filter(|function| function.name == "value")
        .collect::<Vec<_>>();
    assert_eq!(value.len(), 2);
    assert_eq!(value[0].pos.file, "one/foo.ts");
    assert_ne!(value[0].symbol, value[1].symbol);
    // The import reaches the first module: its symbol is the callee in main.ts.
    let text = format!("{:?}", module.top_level);
    assert!(text.contains(&format!("{:?}", value[0].symbol)), "{text}");
    assert!(!text.contains(&format!("{:?}", value[1].symbol)), "{text}");
}

// Cost (debug): below 0.01 s; two input checks, without a parse.
#[test]
fn two_names_with_one_key_report_the_second_file() {
    for (first, second) in [("a/x.ts", "./a/x.ts"), ("a/x.ts", "a/b/../x.ts")] {
        let errors = check_program(&[
            SourceFile::new(first, "export const x: i32 = 1;"),
            SourceFile::new(second, "export const x: i32 = 2;"),
            SourceFile::entry("main.ts", "export function main(): void {}"),
        ])
        .expect_err("one key");
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].code, RuleCode::S100);
        assert_eq!(
            errors[0].message,
            format!("duplicate module key `a/x`: `{first}` and `{second}`")
        );
        assert_eq!(errors[0].pos.file, second);
        assert_eq!((errors[0].pos.line, errors[0].pos.col), (1, 1));
        assert_eq!(errors[0].divergence, None);
    }
}

// Cost (debug): below 0.01 s; one input check, without a parse.
#[test]
fn duplicate_key_precedes_parser_errors() {
    let errors = check_program(&[
        SourceFile::new("a/x.ts", "const ="),
        SourceFile::new("./a/x.ts", "const ="),
    ])
    .expect_err("input guard precedes parsing");
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code, RuleCode::S100);
    assert_eq!(
        errors[0].message,
        "duplicate module key `a/x`: `a/x.ts` and `./a/x.ts`"
    );
}

// Cost (debug): below 0.01 s; one check, without subprocesses.
#[test]
fn ambient_files_can_share_a_name() {
    check_program(&[
        SourceFile::ambient("one/foo.d.ts", ""),
        SourceFile::ambient("./one/foo.d.ts", ""),
        SourceFile::entry("main.ts", "print('main');"),
    ])
    .expect("ambient files have no key");
}

// Cost (debug): below 0.01 s; two checks, without subprocesses.
#[test]
fn a_specifier_resolves_against_the_directory_of_its_importer() {
    // The importer is in sub/, so `./x` names sub/x, and the only x module is elsewhere.
    let files = [
        SourceFile::entry(
            "sub/a.ts",
            "import { x } from './x'; export function main(): void { print(`${x}`); }",
        ),
        SourceFile::new("unrelated/x.ts", "export const x: i32 = 1;"),
    ];
    let errors = check_program(&files).expect_err("sub/x is absent");
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].code, RuleCode::S100);
    assert_eq!(
        errors[0].message,
        "imported module `./x` is not among the program's files"
    );
    assert_eq!(
        (
            errors[0].pos.file.as_str(),
            errors[0].pos.line,
            errors[0].pos.col
        ),
        ("sub/a.ts", 1, 19)
    );
    // Control: the specifier that reaches unrelated/x.ts checks.
    let mut control = files.to_vec();
    control[0].source = control[0].source.replace("'./x'", "'../unrelated/x'");
    check_program(&control).expect("../unrelated/x names unrelated/x.ts");
}

// Cost (debug): below 0.01 s; two checks, without subprocesses.
#[test]
fn a_missing_module_in_a_subdirectory_is_s100_at_the_specifier() {
    let files = [
        SourceFile::entry(
            "main.ts",
            "import { x } from './sub/missing'; export function main(): void { print(`${x}`); }",
        ),
        SourceFile::new("sub/present.ts", "export const x: i32 = 1;"),
    ];
    let errors = check_program(&files).expect_err("sub/missing is absent");
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].code, RuleCode::S100);
    assert_eq!(
        errors[0].message,
        "imported module `./sub/missing` is not among the program's files"
    );
    assert_eq!((errors[0].pos.line, errors[0].pos.col), (1, 19));
    let mut control = files.to_vec();
    control[0].source = control[0].source.replace("missing", "present");
    check_program(&control).expect("./sub/present names sub/present.ts");
}

// Cost (debug): below 0.01 s; two checks, without subprocesses.
#[test]
fn a_bare_specifier_names_no_file() {
    let files = [
        SourceFile::entry(
            "main.ts",
            "import { x } from 'x'; export function main(): void { print(`${x}`); }",
        ),
        SourceFile::new("x.ts", "export const x: i32 = 1;"),
    ];
    let errors = check_program(&files).expect_err("a bare specifier names no file");
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(
        errors[0].message,
        "imported module `x` is not among the program's files"
    );
    // Rule 4: a `.ts` suffix keeps its treatment and names x.ts.
    let mut suffix = files.to_vec();
    suffix[0].source = suffix[0].source.replace("'x'", "'./x.ts'");
    check_program(&suffix).expect("./x.ts names x.ts");
}

// Cost (debug): below 0.01 s; three checks, without subprocesses.
#[test]
fn a_poison_entry_is_a_key_for_an_importer_in_a_subdirectory() {
    let files = |specifier: &str| {
        vec![
            SourceFile::entry(
                "main.ts",
                "import { size } from './sub/a'; export function main(): void { print(`${size()}`); }",
            ),
            SourceFile::new(
                "sub/a.ts",
                format!(
                    "import {{ A_SIZE }} from '{specifier}'; export function size(): i32 {{ return A_SIZE; }}"
                ),
            ),
        ]
    };
    let mut options = CheckOptions::default();
    options.poison_missing_modules = vec!["./p.typegpu".to_string()];
    let module = check_program_with(&files("../p.typegpu"), &options).expect("poisoned import");
    assert_eq!(module.poisoned_imports.len(), 1);
    assert_eq!(module.poisoned_imports[0].module, "../p.typegpu");
    assert_eq!(module.poisoned_imports[0].pos.file, "sub/a.ts");
    // Control: `./p.typegpu` in sub/ has the key sub/p.typegpu, which the list does not hold.
    let errors = check_program_with(&files("./p.typegpu"), &options).expect_err("another key");
    assert_eq!(
        errors[0].message,
        "imported module `./p.typegpu` is not among the program's files"
    );
    options.poison_missing_modules = vec!["./sub/p.typegpu".to_string()];
    check_program_with(&files("./p.typegpu"), &options).expect("the key sub/p.typegpu");
}
