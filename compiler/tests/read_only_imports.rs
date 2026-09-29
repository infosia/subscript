//! Import targets are read-only; member targets stay writable (compiler.md §127).

use subscript_compiler::{check_program, check_program_with, CheckOptions, RuleCode, SourceFile};

const LIB: &str = "export let count: i32 = 4;
export class Box { value: i32 = 4; static value: i32 = 4; }
export const box: Box = new Box();
export function bump(): void { count += 1; }";

fn files(main: &str, lib: &str) -> [SourceFile; 2] {
    [
        SourceFile::entry("main.ts", main),
        SourceFile::new("lib.ts", lib),
    ]
}

fn check_write(template: &str) {
    for (import, name) in [("count", "count"), ("count as local", "local")] {
        let write = template.replace("TARGET", name);
        let main = format!(
            "import {{ {import}, box, Box, bump }} from \"./lib\";\n\
             export function main(): void {{\n{write};\n}}"
        );
        let errors = check_program(&files(&main, LIB)).expect_err("an import is read-only");
        assert_eq!(errors.len(), 1, "{write}: {errors:?}");
        let error = &errors[0];
        assert_eq!(error.code, RuleCode::S100, "{write}");
        assert_eq!(
            error.message,
            format!("cannot assign to `{name}` because it is an import")
        );
        assert_eq!(error.pos.file, "main.ts");
        assert_eq!(error.pos.line, 3);
        assert_eq!(error.pos.col, (write.find(name).unwrap() + 1) as u32);

        let exporter = format!(
            "{LIB}\nexport function write(): void {{ {}; }}",
            template.replace("TARGET", "count")
        );
        let controls = format!(
            "import {{ {import}, box, Box, bump, write }} from \"./lib\";\n\
             export function main(): void {{\n\
             {}; {}; write(); bump(); const read: i32 = {name};\n\
             {{ let {name}: i32 = 4; {write}; }}\n}}",
            template.replace("TARGET", "box.value"),
            template.replace("TARGET", "Box.value"),
        );
        check_program(&files(&controls, &exporter))
            .expect("exporter, object fields, static fields, and shadow locals stay writable");
    }
}

macro_rules! write_tests {
    ($($name:ident => $write:literal),+ $(,)?) => {
        $(#[test]
        fn $name() {
            check_write($write);
        })+
    };
}

write_tests! {
    assign => "TARGET = 1",
    add => "TARGET += 1",
    subtract => "TARGET -= 1",
    multiply => "TARGET *= 1",
    divide => "TARGET /= 1",
    remainder => "TARGET %= 1",
    bit_and => "TARGET &= 1",
    bit_or => "TARGET |= 1",
    bit_xor => "TARGET ^= 1",
    shift_left => "TARGET <<= 1",
    shift_right => "TARGET >>= 1",
    shift_unsigned => "TARGET >>>= 1",
    prefix_increment => "++TARGET",
    postfix_increment => "TARGET++",
    prefix_decrement => "--TARGET",
    postfix_decrement => "TARGET--",
}

#[test]
fn imported_const_function_and_class_report_the_local_import_name() {
    for declaration in [
        "export const binding: i32 = 4;",
        "export function binding(): i32 { return 4; }",
        "export class binding {}",
    ] {
        for (import, name) in [("binding", "binding"), ("binding as local", "local")] {
            let main = format!("import {{ {import} }} from \"./lib\";\n{name} = 1;");
            let errors = check_program(&files(&main, declaration)).expect_err("import write");
            assert_eq!(errors.len(), 1, "{errors:?}");
            assert_eq!(errors[0].code, RuleCode::S100);
            assert_eq!(
                errors[0].message,
                format!("cannot assign to `{name}` because it is an import")
            );
            check_program(&files(
                &format!("import {{ {import} }} from \"./lib\";"),
                declaration,
            ))
            .expect("the declaration can be imported");
        }
    }
}

#[test]
fn imported_enum_write_reports_the_import_at_the_target() {
    let lib = "export enum Color { Red, Blue }";
    let main = "import { Color } from \"./lib\";\nColor = Color;";
    let errors = check_program(&files(main, lib)).expect_err("an imported enum is read-only");
    assert_eq!(errors.len(), 2, "{errors:?}");
    assert_eq!(errors[0].code, RuleCode::S100);
    assert_eq!(
        errors[0].message,
        "cannot assign to `Color` because it is an import"
    );
    assert_eq!(errors[0].pos.file, "main.ts");
    assert_eq!((errors[0].pos.line, errors[0].pos.col), (2, 1));
    assert_eq!(errors[1].code, RuleCode::S100);
    assert_eq!(
        errors[1].message,
        "enum `Color` used as a value; use a member"
    );
    assert_eq!((errors[1].pos.line, errors[1].pos.col), (2, 9));

    check_program(&files(
        "import { Color } from \"./lib\";\n\
         export function main(): void {\n\
         let color: Color = Color.Red; color = Color.Blue;\n\
         { let Color: i32 = 1; Color = Color; }\n}",
        lib,
    ))
    .expect("enum members and a mutable shadow binding remain usable");
}

#[test]
fn poisoned_import_write_does_not_add_a_diagnostic() {
    let main = "import { value } from \"./p.typegpu\";\nvalue = 1;";
    let mut options = CheckOptions::default();
    options.poison_missing_modules = vec!["./p.typegpu".to_string()];
    let module = check_program_with(&[SourceFile::entry("main.ts", main)], &options)
        .expect("discovery suppresses diagnostics for the missing module and its uses");
    assert_eq!(module.poisoned_imports.len(), 1);
    assert_eq!(module.poisoned_imports[0].module, "./p.typegpu");
    assert_eq!(
        module.poisoned_imports[0].names,
        [("value".to_string(), "value".to_string())]
    );

    let missing_export = [
        SourceFile::entry("main.ts", main),
        SourceFile::new("p.typegpu.ts", "export let other: i32 = 4;"),
    ];
    let errors = check_program_with(&missing_export, &options)
        .expect_err("an absent export reports only the import diagnostic");
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].code, RuleCode::S016);
    assert_eq!(
        errors[0].message,
        "`value` is not exported by `./p.typegpu`"
    );
    assert_eq!(errors[0].pos.file, "main.ts");
    assert_eq!((errors[0].pos.line, errors[0].pos.col), (1, 10));

    let present_export = [
        SourceFile::entry("main.ts", main),
        SourceFile::new("p.typegpu.ts", "export let value: i32 = 4;"),
    ];
    let errors = check_program_with(&present_export, &options)
        .expect_err("a resolved import remains read-only under discovery");
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].code, RuleCode::S100);
    assert_eq!(
        errors[0].message,
        "cannot assign to `value` because it is an import"
    );
    assert_eq!(errors[0].pos.file, "main.ts");
    assert_eq!((errors[0].pos.line, errors[0].pos.col), (2, 1));
}
