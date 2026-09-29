//! Renamed imports resolve exports and retain local scope rules (compiler.md §126).

use subscript_compiler::{check_program, RuleCode, SourceFile};

fn files(main: &str, lib: &str) -> [SourceFile; 2] {
    [
        SourceFile::new("main.ts", main),
        SourceFile::new("lib.ts", lib),
    ]
}

#[test]
fn imported_name_selects_the_declaration_and_local_name_binds_it() {
    let lib = "export function f(): string { return \"wrong\"; }\n\
               export function g(): i32 { return 2; }";
    for import in ["g as f", "g as h", "g"] {
        let local = import.split_whitespace().last().unwrap();
        let main = format!(
            "import {{ {import} }} from \"./lib\";\n\
             export function main(): void {{ const value: i32 = {local}(); }}"
        );
        check_program(&files(&main, lib)).expect("the imported declaration returns i32");
        let wrong = main.replace("value: i32", "value: string");
        let errors =
            check_program(&files(&wrong, lib)).expect_err("the imported result is not string");
        assert!(errors.iter().any(|error| error.code == RuleCode::S100));
    }
    let errors = check_program(&files(
        "import { g as h } from \"./lib\"; export function main(): void { g(); }",
        lib,
    ))
    .expect_err("only the local name binds in the importing module");
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code, RuleCode::S016);
}

#[test]
fn missing_export_reports_the_imported_name_and_position() {
    for (import, name) in [("missing as present", "missing"), ("missing", "missing")] {
        let main = format!("import {{ {import} }} from \"./lib\";");
        let lib = "export function present(): i32 { return 1; }";
        let errors = check_program(&files(&main, lib)).expect_err("the imported name is absent");
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].code, RuleCode::S016);
        assert_eq!(
            errors[0].message,
            format!("`{name}` is not exported by `./lib`")
        );
        assert_eq!(errors[0].pos.file, "main.ts");
        assert_eq!((errors[0].pos.line, errors[0].pos.col), (1, 10));
        check_program(&files(&main.replace("missing", "present"), lib))
            .expect("the target exports present");
    }
}

#[test]
fn duplicate_local_imports_report_s017_at_the_second_local_name() {
    let lib = "export function a(): i32 { return 1; }\n\
               export function b(): i32 { return 2; }";
    let duplicate = "import { a as local } from \"./lib\";\n\
                     import { b as local } from \"./lib\";";
    let errors = check_program(&files(duplicate, lib)).expect_err("duplicate local binding");
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code, RuleCode::S017);
    assert!(errors[0].message.contains("local"));
    assert_eq!((errors[0].pos.line, errors[0].pos.col), (2, 15));
    check_program(&files(&duplicate.replace("b as local", "b as other"), lib))
        .expect("distinct local names");
}
