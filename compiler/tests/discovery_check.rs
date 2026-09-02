use subscript_compiler::{
    check_program, check_program_with, CheckOptions, RuleCode, SourceFile, Type,
};

const DISCOVERY_SOURCE: &str = r#"
import { A_SIZE, B_WGSL } from "./p.typegpu";

@CStruct
class Header {
  value: i32;
}

export function main(): void {
  const size: i32 = A_SIZE;
  const shader: B_WGSL = A_SIZE;
}
"#;

fn discovery_options(specifier: &str) -> CheckOptions {
    let mut options = CheckOptions::default();
    options.poison_missing_modules = vec![specifier.to_string()];
    options
}

#[test]
fn discovery_check_keeps_hir_and_records_named_imports() {
    let files = [SourceFile::new("main.ts", DISCOVERY_SOURCE)];
    let options = discovery_options("p.typegpu.ts");
    let module = check_program_with(&files, &options).expect("discovery check");

    assert_eq!(module.classes.len(), 1);
    assert_eq!(module.classes[0].name, "Header");
    assert!(module.classes[0].is_value);
    assert_eq!(module.classes[0].fields.len(), 1);
    assert_eq!(module.poisoned_imports.len(), 1);
    assert_eq!(module.poisoned_imports[0].module, "./p.typegpu");
    assert_eq!(
        module.poisoned_imports[0].names,
        [
            ("A_SIZE".to_string(), "A_SIZE".to_string()),
            ("B_WGSL".to_string(), "B_WGSL".to_string()),
        ]
    );
    assert_eq!(module.poisoned_imports[0].pos.file, "main.ts");

    let function = module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("main function");
    assert!(function.body.iter().any(|statement| {
        matches!(
            statement,
            subscript_compiler::hir::Stmt::Let {
                ty: Type::Error,
                ..
            }
        )
    }));
}

#[test]
fn default_check_reports_the_absent_module() {
    let diagnostics = check_program(&[SourceFile::new("main.ts", DISCOVERY_SOURCE)])
        .expect_err("default check must reject the absent module");

    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == RuleCode::S100
            && diagnostic.message
                == "imported module `./p.typegpu` is not among the program's files"
    }));
}

#[test]
fn discovery_check_records_import_aliases() {
    let source = r#"
import { A as B } from "./p.typegpu";
export function main(): void {
  const value: i32 = B;
}
"#;
    let options = discovery_options("./p.typegpu");
    let module = check_program_with(&[SourceFile::new("main.ts", source)], &options)
        .expect("discovery check");

    assert_eq!(
        module.poisoned_imports[0].names,
        [("A".to_string(), "B".to_string())]
    );
}

#[test]
fn present_listed_module_resolves_normally() {
    let files = [
        SourceFile::new(
            "main.ts",
            "import { A_SIZE } from \"./p.typegpu.ts\";\n\
             export function main(): void { const size: i32 = A_SIZE; }\n",
        ),
        SourceFile::new("p.typegpu.ts", "export const A_SIZE: i32 = 4;\n"),
    ];
    let options = discovery_options("./p.typegpu");
    let module = check_program_with(&files, &options).expect("normal module resolution");

    assert!(module.poisoned_imports.is_empty());
}

#[test]
fn discovery_option_does_not_hide_unrelated_diagnostics() {
    let source = r#"
import { A_SIZE } from "./p.typegpu";
export function main(): void {
  const bad: i32 = "bad";
  const size: i32 = A_SIZE;
}
"#;
    let options = discovery_options("./p.typegpu");
    let diagnostics = check_program_with(&[SourceFile::new("main.ts", source)], &options)
        .expect_err("unrelated diagnostic must reject the program");

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("type mismatch")),
        "diagnostics: {diagnostics:?}"
    );
}

#[test]
fn default_import_from_listed_absent_module_is_rejected() {
    let source = "import Value from \"./p.typegpu\";\nexport function main(): void {}\n";
    let options = discovery_options("./p.typegpu");
    let diagnostics = check_program_with(&[SourceFile::new("main.ts", source)], &options)
        .expect_err("default import must remain rejected");

    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == RuleCode::S100
            && diagnostic.message == "only named imports are in the decided surface"
    }));
}

#[test]
fn poisoned_call_and_construction_keep_argument_diagnostics() {
    let source = r#"
import { A_SIZE } from "./p.typegpu";
export function main(): void {
  A_SIZE(undefinedName);
  new A_SIZE(undefinedName);
}
"#;
    let options = discovery_options("./p.typegpu");
    let diagnostics = check_program_with(&[SourceFile::new("main.ts", source)], &options)
        .expect_err("unknown arguments must reject the program");
    let unknown_names: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.message == "unknown name `undefinedName`")
        .collect();

    assert_eq!(unknown_names.len(), 2, "diagnostics: {diagnostics:?}");
    assert!(unknown_names
        .iter()
        .all(|diagnostic| diagnostic.code == RuleCode::S016));
}

#[test]
fn poisoned_type_keeps_type_argument_diagnostics() {
    let source = r#"
import { B_WGSL } from "./p.typegpu";
export function main(): void {
  const shader: B_WGSL<NotAType> = B_WGSL;
}
"#;
    let options = discovery_options("./p.typegpu");
    let diagnostics = check_program_with(&[SourceFile::new("main.ts", source)], &options)
        .expect_err("unknown type argument must reject the program");

    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == RuleCode::S016 && diagnostic.message == "unknown type name `NotAType`"
    }));
}
