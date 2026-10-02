//! Static namespace qualifier rules (§148).
use subscript_compiler::{check_program, RuleCode, SourceFile};

fn files(main: &str) -> Vec<SourceFile> {
    vec![
        SourceFile::entry("main.ts", main),
        SourceFile::new(
            "lib.ts",
            "export let count: i32 = 4; export function get(): i32 { return count; }",
        ),
    ]
}

#[test]
fn local_binding_shadows_namespace() {
    let module = check_program(&files(
        "import * as ns from './lib'; class Local { count: string = 'local'; } export function main(): void { const ns = new Local(); const value: string = ns.count; print(value); }",
    )).unwrap();
    let text = format!(
        "{:?}",
        module
            .functions
            .iter()
            .find(|function| function.name == "main")
            .unwrap()
    );
    assert!(text.contains("Field"), "{text}");
    assert!(!text.contains("Global("), "{text}");
    check_program(&files(
        "import * as ns from './lib'; export function main(): void { const value: i32 = ns.count; print(`${value}`); }",
    )).unwrap();
}

#[test]
fn namespace_import_exports_no_host_entry() {
    let module = check_program(&files("import * as ns from './lib';")).unwrap();
    assert!(module.host_entries.is_empty());
    let module = check_program(&files(
        "import * as ns from './lib'; export function main(): void { print(`${ns.get()}`); }",
    ))
    .unwrap();
    assert_eq!(module.host_entries.len(), 1);
    assert_eq!(module.host_entries[0].name, "main");
}

#[test]
fn namespace_writes_and_missing_members_name_the_source() {
    for form in ["ns.count = 1", "ns.count += 1", "ns.count++", "--ns.count"] {
        let errors = check_program(&files(&format!(
            "import * as ns from './lib'; export function main(): void {{ {form}; }}"
        )))
        .unwrap_err();
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].code, RuleCode::S100);
        assert_eq!(
            errors[0].message,
            "cannot assign to `ns.count` because it is an import"
        );
    }
    for form in [
        "ns.nope()",
        "new ns.nope()",
        "const x: ns.nope = 1",
        "print(`${ns.nope}`)",
    ] {
        let errors = check_program(&files(&format!(
            "import * as ns from './lib'; export function main(): void {{ {form}; }}"
        )))
        .unwrap_err();
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].code, RuleCode::S016);
        assert_eq!(errors[0].message, "`nope` is not exported by `./lib`");
    }
}

#[test]
fn missing_namespace_module_has_one_origin_and_discovery_keeps_poison() {
    use subscript_compiler::{check_program_with, CheckOptions};
    let sources = [SourceFile::entry("main.ts", "import * as ns from './absent'; export function main(): void { ns.call(); const c: ns.C = new ns.C(); }")];
    let errors = check_program(&sources).unwrap_err();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(
        errors[0].message,
        "imported module `./absent` is not among the program's files"
    );
    let mut options = CheckOptions::default();
    options.poison_missing_modules = vec!["./absent".to_string()];
    let module = check_program_with(&sources, &options).unwrap();
    assert_eq!(
        module.poisoned_imports[0].names,
        [("*".to_string(), "ns".to_string())]
    );
    let sources = [SourceFile::entry(
        "main.ts",
        "import type * as ns from './absent'; export function main(): void {}",
    )];
    let errors = check_program_with(&sources, &options).unwrap_err();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(
        errors[0].message,
        "`import type * as` is outside the decided surface"
    );
}
