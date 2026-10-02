//! Static namespace qualifier rules (§148): nine debug tests cost 13.098 ms.
//! Cost is the median process duration over five runs, excluding compilation.
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
    assert!(module.poisoned_imports[0].names.is_empty());
    assert_eq!(module.poisoned_imports[0].namespace.as_deref(), Some("ns"));
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

#[test]
fn namespace_re_export_has_one_c18_origin() {
    let errors = check_program(&[
        SourceFile::entry("main.ts", "export { ns } from './bridge';"),
        SourceFile::new("bridge.ts", "import * as ns from './lib'; export { ns };"),
        SourceFile::new("lib.ts", "export function value(): void {}"),
    ])
    .unwrap_err();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].code, RuleCode::S100);
    assert_eq!(errors[0].pos.file, "bridge.ts");
    assert_eq!(
        errors[0].divergence,
        Some(subscript_compiler::divergence::Divergence::NamedModuleSurface)
    );
    check_program(&[
        SourceFile::entry("main.ts", "export { value } from './bridge';"),
        SourceFile::new(
            "bridge.ts",
            "import { value } from './lib'; export { value };",
        ),
        SourceFile::new("lib.ts", "export function value(): void {}"),
    ])
    .unwrap();
}

#[test]
fn generic_templates_receive_resolved_namespace_members() {
    check_program(&files("import * as ns from './lib'; function f<T>(x: T): i32 { return ns.get(); } class C<T> { get(x: T): i32 { return ns.get(); } } export function main(): void { print(`${f<i32>(1)} ${new C<i32>().get(1)}`); }")).unwrap();
    let errors = check_program(&files(
        "import * as ns from './lib'; function f<T>(x: T): i32 { return ns.nope(); }",
    ))
    .unwrap_err();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].code, RuleCode::S016);
}

#[test]
fn named_discovery_poison_has_no_namespace_local() {
    use subscript_compiler::{check_program_with, CheckOptions};
    let mut options = CheckOptions::default();
    options.poison_missing_modules = vec!["./absent".to_string()];
    let sources = [SourceFile::entry(
        "main.ts",
        "import { value as local } from './absent'; export function main(): void { local(); }",
    )];
    let module = check_program_with(&sources, &options).unwrap();
    assert_eq!(module.poisoned_imports[0].namespace, None);
    assert_eq!(
        module.poisoned_imports[0].names,
        [("value".to_string(), "local".to_string())]
    );
}

#[test]
fn value_shadows_keep_namespace_types_in_signatures_and_bodies() {
    let sources = [
        SourceFile::entry(
            "main.ts",
            include_str!("../../corpus/accept/a322-namespace-type-shadow/main.ts"),
        ),
        SourceFile::new(
            "lib.ts",
            include_str!("../../corpus/accept/a322-namespace-type-shadow/lib.ts"),
        ),
    ];
    check_program(&sources).unwrap();
}

#[test]
fn qualified_namespace_types_ignore_type_parameters() {
    for declaration in [
        "function qualified<ns>(value: ns.x): void {}",
        "class Qualified<ns> { value: ns.x; constructor(value: ns.x) { this.value = value; } }",
        "function outer(): void { const qualified = <ns>(value: ns.x): void => {}; }",
        "function qualified<ns extends ns.x>(): void {}",
    ] {
        let sources = [
            SourceFile::entry(
                "main.ts",
                format!("import * as ns from './lib'; {declaration}"),
            ),
            SourceFile::new("lib.ts", "export class x { value: i32 = 1; }"),
        ];
        check_program(&sources).unwrap_or_else(|errors| panic!("{declaration}: {errors:?}"));
    }
    check_program(&[
        SourceFile::entry("main.ts", "import * as ns from './lib'; function generic<ns>(value: ns): ns { return value; } class Generic<ns> { value: ns; constructor(value: ns) { this.value = value; } } function after(): ns.x { return new ns.x(); } export function main(): void { print(`${generic<i32>(2)} ${new Generic<i32>(3).value} ${after().value}`); }"),
        SourceFile::new("lib.ts", "export class x { value: i32 = 1; }"),
    ]).unwrap();
}
