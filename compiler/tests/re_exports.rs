//! Re-exports preserve scope, diagnostics, and host entry rules (compiler.md §128).

use subscript_compiler::{
    check_program, check_program_with, divergence::Divergence, CheckOptions, RuleCode, SourceFile,
};

fn sources(surface: &str, lib: &str) -> [SourceFile; 2] {
    [
        SourceFile::new("surface.ts", surface),
        SourceFile::new("lib.ts", lib),
    ]
}

#[test]
fn a_remote_export_binds_no_local_name() {
    let lib = "export function value(): i32 { return 1; }";
    for name in ["value", "other"] {
        let source = format!(
            "export {{ value as other }} from \"./lib\";\nfunction read(): i32 {{ return {name}(); }}"
        );
        let errors = check_program(&sources(&source, lib)).expect_err("no local binding");
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].code, RuleCode::S016);
        assert!(errors[0].message.contains(name));
        let control = format!("import {{ value as {name} }} from \"./lib\";\n{source}");
        check_program(&sources(&control, lib)).expect("an import binds the local name");
    }
}

#[test]
fn duplicate_exports_report_the_second_export_name() {
    for (surface, line, col) in [
        (
            "export { value as x } from \"./lib\";\nexport { value as x } from \"./lib\";",
            2,
            19,
        ),
        (
            "export const x: i32 = 1;\nexport { value as x } from \"./lib\";",
            2,
            19,
        ),
        (
            "export { value as x } from \"./lib\";\nexport const x: i32 = 1;",
            2,
            14,
        ),
    ] {
        let errors = check_program(&sources(surface, "export const value: i32 = 1;"))
            .expect_err("duplicate export");
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].code, RuleCode::S017);
        assert_eq!((errors[0].pos.line, errors[0].pos.col), (line, col));
        assert_eq!(errors[0].message, "duplicate export name `x`");
    }
    check_program(&sources(
        "const x: i32 = 2; export { value as x } from \"./lib\";",
        "export const value: i32 = 1;",
    ))
    .expect("local names and export names are separate namespaces");
}

#[test]
fn missing_re_export_reports_the_original_name() {
    let errors = check_program(&sources(
        "export { missing as present } from \"./lib\";",
        "export const present: i32 = 1;",
    ))
    .expect_err("missing source export");
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code, RuleCode::S016);
    assert_eq!(errors[0].message, "`missing` is not exported by `./lib`");
    assert_eq!((errors[0].pos.line, errors[0].pos.col), (1, 10));
}

#[test]
fn a_cycle_reports_once_at_its_first_member_and_poisons_its_prefix() {
    let files = [
        SourceFile::new("head.ts", "export { x } from \"./left\";"),
        SourceFile::new("left.ts", "export { y as x } from \"./right\";"),
        SourceFile::new(
            "right.ts",
            "import { x as local } from \"./left\"; export { local as y };",
        ),
    ];
    for order in [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ] {
        let mut ordered: Vec<_> = order.iter().map(|i| files[*i].clone()).collect();
        ordered.push(SourceFile::new(
            "main.ts",
            "import { x } from './head'; function read(): i32 { return x; }",
        ));
        let errors = check_program(&ordered).expect_err("cycle without a declaration");
        let first = order.iter().find(|i| **i != 0).unwrap();
        let (file, col, chain) = if *first == 1 {
            ("left.ts", 10, "left.ts::x -> right.ts::y -> left.ts::x")
        } else {
            ("right.ts", 47, "right.ts::y -> left.ts::x -> right.ts::y")
        };
        assert_eq!(
            errors,
            vec![resolution_diagnostic(
                RuleCode::S016,
                format!("export alias chain reaches no declaration: {chain}"),
                subscript_compiler::Pos::new(file, 1, col),
            )]
        );
        let right = ordered.iter_mut().find(|f| f.name == "right.ts").unwrap();
        right.source = "import { x as local } from './left'; export const y: i32 = 1;".into();
        check_program(&ordered).expect("a module cycle that reaches a declaration");
    }
}

#[test]
fn re_exported_imports_stay_read_only() {
    for target in ["value = 2", "value += 2", "++value", "value--"] {
        let files = [
            SourceFile::new("main.ts", format!("import {{ other as value }} from \"./surface\"; function write(): void {{ {target}; }}")),
            SourceFile::new("surface.ts", "export { value as other } from \"./lib\";"),
            SourceFile::new("lib.ts", "export let value: i32 = 1; export function bump(): void { value++; }"),
        ];
        let errors = check_program(&files).expect_err("import is read-only");
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(
            errors[0].message,
            "cannot assign to `value` because it is an import"
        );
    }
}

#[test]
fn re_exports_do_not_change_the_host_entry_predicate() {
    let module = check_program(&sources(
        "function local(): void {} export { local as entry }; export { value as remote } from \"./lib\";",
        "export function value(): void {}",
    )).expect("re-exports preserve declaration flags");
    let local = module.functions.iter().find(|f| f.name == "local").unwrap();
    let remote = module.functions.iter().find(|f| f.name == "value").unwrap();
    assert!(local.host_entry_trap_sites(&module).is_none());
    assert!(remote.host_entry_trap_sites(&module).is_some());
    assert_eq!(module.functions.len(), 2);
}

#[test]
fn a_poisoned_local_re_export_keeps_the_import_record() {
    let files = sources(
        "import { value as local } from \"./missing\"; export { local as other };",
        "import { other } from \"./surface\"; function useValue(): i32 { return other; }",
    );
    let mut options = CheckOptions::default();
    options.poison_missing_modules.push("missing".into());
    let module = check_program_with(&files, &options).expect("discovery import");
    assert_eq!(module.poisoned_imports.len(), 1);
    assert_eq!(
        module.poisoned_imports[0].names,
        [("value".into(), "local".into())]
    );
    assert_eq!(
        check_program(&files).expect_err("an absent module needs the discovery option"),
        vec![resolution_diagnostic(
            RuleCode::S100,
            "imported module `./missing` is not among the program's files",
            subscript_compiler::Pos::new("surface.ts", 1, 32)
        )]
    );
}

#[test]
fn unnamed_export_forms_carry_the_module_surface_divergence() {
    for source in [
        "export * from \"./lib\";",
        "export * as ns from \"./lib\";",
        "export default 1;",
        "export default function value(): i32 { return 1; }",
    ] {
        let errors = check_program(&sources(source, "export const value: i32 = 1;"))
            .expect_err("named surface");
        assert_eq!(errors[0].code, RuleCode::S100);
        assert_eq!(errors[0].divergence, Some(Divergence::NamedModuleSurface));
        assert_eq!(Divergence::NamedModuleSurface.entry().collision, "C18");
    }
}

#[test]
fn rejected_named_forms_have_controls() {
    let lib = "export type Label = \"ready\" | \"done\"; export class Box { value: i32 = 4; } export const value: i32 = 4;";
    for source in [
        "export type { Label, Box } from \"./lib\";",
        "import { Label, Box } from \"./lib\"; export type { Label, Box };",
        "export { type Label } from \"./lib\";",
        "import { Label } from \"./lib\"; export { type Label };",
        "export { value as default } from \"./lib\";",
        "const value: i32 = 4; export { value as default };",
        "export { default } from \"./lib\";",
        "export { default as value } from \"./lib\";",
        "export { value as \"default\" } from \"./lib\";",
        "export { \"default\" as value } from \"./lib\";",
    ] {
        let errors = check_program(&sources(source, lib)).expect_err(source);
        assert_eq!(errors.len(), 1, "{source}: {errors:?}");
        assert_eq!(errors[0].code, RuleCode::S100);
        assert_eq!(errors[0].divergence, Some(Divergence::NamedModuleSurface));
    }
    for source in [
        "export { Label, Box, value as other } from \"./lib\";",
        "import { Label, Box, value } from \"./lib\"; export { Label, Box, value as other };",
    ] {
        check_program(&sources(source, lib)).expect("ordinary named exports");
    }
}

#[test]
fn failed_export_chains_report_only_the_failure_site_in_every_file_order() {
    use subscript_compiler::Pos;
    let files = [
        SourceFile::new(
            "main.ts",
            "import { y } from \"./surface\"; function read(): i32 { return y; }",
        ),
        SourceFile::new("surface.ts", "export { x as y } from \"./bridge\";"),
        SourceFile::new("bridge.ts", "export { nope as x } from \"./lib\";"),
        SourceFile::new("lib.ts", "export const value: i32 = 4;"),
    ];
    for a in 0..4 {
        for b in 0..4 {
            for c in 0..4 {
                for d in 0..4 {
                    let order = [a, b, c, d];
                    if (0..4).any(|i| order[..i].contains(&order[i])) {
                        continue;
                    }
                    let mut ordered: Vec<_> = order.iter().map(|i| files[*i].clone()).collect();
                    assert_eq!(
                        check_program(&ordered).expect_err("missing export"),
                        vec![resolution_diagnostic(
                            RuleCode::S016,
                            "`nope` is not exported by `./lib`",
                            Pos::new("bridge.ts", 1, 10)
                        )]
                    );
                    let bridge = ordered.iter_mut().find(|f| f.name == "bridge.ts").unwrap();
                    bridge.source = bridge.source.replace("nope", "value");
                    check_program(&ordered).expect("the real export resolves");
                }
            }
        }
    }
}

#[test]
fn discovery_re_exports_poison_missing_sources_and_record_names() {
    use subscript_compiler::Pos;
    let files = sources(
        "export { x as y } from \"./absent\";",
        "import { y } from \"./surface\"; function read(): i32 { return y; }",
    );
    let mut options = CheckOptions::default();
    options.poison_missing_modules.push("./absent.ts".into());
    let module = check_program_with(&files, &options).expect("discovery re-export");
    assert_eq!(module.poisoned_imports.len(), 1);
    assert_eq!(module.poisoned_imports[0].module, "./absent");
    assert_eq!(module.poisoned_imports[0].names, [("x".into(), "y".into())]);
    assert_eq!(
        check_program(&files).expect_err("missing source"),
        vec![resolution_diagnostic(
            RuleCode::S100,
            "export source module `./absent` is not among the program's files",
            Pos::new("surface.ts", 1, 24)
        )]
    );
    let mut present = files.to_vec();
    present.push(SourceFile::new("absent.ts", "export const x: i32 = 4;"));
    assert!(check_program_with(&present, &options)
        .expect("present source")
        .poisoned_imports
        .is_empty());
}

#[test]
fn mirror_export_lists_stay_outside_the_surface() {
    for source in ["export {};", "declare const value: i32; export { value };"] {
        let errors = check_program(&[SourceFile::ambient("mirror.d.ts", source)])
            .expect_err("mirror export list");
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].code, RuleCode::S100);
        assert_eq!(
            errors[0].message,
            "export lists are outside the mirror surface"
        );
    }
    check_program(&[SourceFile::ambient(
        "mirror.d.ts",
        "declare const value: i32;",
    )])
    .expect("mirror declaration");
    check_program(&[SourceFile::new("main.ts", "export {};")]).expect("program export list");
}

#[test]
fn missing_modules_have_one_origin_per_statement() {
    use subscript_compiler::Pos;
    let files = [
        SourceFile::new("main.ts", "import { x, y } from './surface';"),
        SourceFile::new(
            "surface.ts",
            "export { a as x, b as y } from './absent';\nexport { c, d } from './absent';",
        ),
    ];
    assert_eq!(
        check_program(&files).unwrap_err(),
        vec![
            resolution_diagnostic(
                RuleCode::S100,
                "export source module `./absent` is not among the program's files",
                Pos::new("surface.ts", 1, 32)
            ),
            resolution_diagnostic(
                RuleCode::S100,
                "export source module `./absent` is not among the program's files",
                Pos::new("surface.ts", 2, 22)
            ),
        ]
    );
    let mut options = CheckOptions::default();
    options.poison_missing_modules.push("absent".into());
    check_program_with(&files, &options).expect("discovery control");
}

#[test]
fn declaration_duplicates_belong_to_the_scope() {
    use subscript_compiler::Pos;
    for (source, name, column) in [
        (
            "export function f(): void {}\nexport const f: i32 = 2;",
            "f",
            14,
        ),
        ("export class K {}\nexport enum K { A }", "K", 13),
    ] {
        assert_eq!(
            check_program(&[SourceFile::new("main.ts", source)]).unwrap_err(),
            vec![subscript_compiler::Diagnostic::new(
                RuleCode::S017,
                format!("duplicate top-level name `{name}`"),
                Pos::new("main.ts", 2, column)
            ),]
        );
    }
}

#[test]
fn cycles_use_source_order_and_keep_separate_origins() {
    use subscript_compiler::Pos;
    let files = [
        SourceFile::new("main.ts", "import { a, z, loop } from './cycle';"),
        SourceFile::new(
            "cycle.ts",
            "export { a as z, z as a } from './cycle';\nexport { loop } from './cycle';",
        ),
    ];
    assert_eq!(check_program(&files).unwrap_err(), vec![
        resolution_diagnostic(RuleCode::S016, "export alias chain reaches no declaration: cycle.ts::z -> cycle.ts::a -> cycle.ts::z", Pos::new("cycle.ts", 1, 10)),
        resolution_diagnostic(RuleCode::S016, "export alias chain reaches no declaration: cycle.ts::loop -> cycle.ts::loop", Pos::new("cycle.ts", 2, 10)),
    ]);
}

#[test]
fn missing_import_module_poisons_local_and_remote_consumers() {
    use subscript_compiler::Pos;
    let files = [
        SourceFile::new("main.ts", "import { y } from './surface'; function read(): i32 { return y; }"),
        SourceFile::new("surface.ts", "import { a, b } from './absent'; export { a as y }; function read(): i32 { return b; }"),
    ];
    assert_eq!(
        check_program(&files).unwrap_err(),
        vec![resolution_diagnostic(
            RuleCode::S100,
            "imported module `./absent` is not among the program's files",
            Pos::new("surface.ts", 1, 22)
        ),]
    );
}

fn resolution_diagnostic(
    code: RuleCode,
    message: impl Into<String>,
    pos: subscript_compiler::Pos,
) -> subscript_compiler::Diagnostic {
    let mut diagnostic = subscript_compiler::Diagnostic::new(code, message, pos);
    diagnostic.resolution = true;
    diagnostic
}

#[test]
fn rejected_declarations_keep_names_through_every_export_edge() {
    use subscript_compiler::{Diagnostic, Pos};
    // Eight rejected programs and eight controls check export edges without code generation.
    let start = std::time::Instant::now();
    for (declaration, names, message, col, divergence) in [
        ("type Num = i32;", "Num", "type aliases are limited to a union of two or more string literals", 6, None),
        ("type Label<T> = 'ready' | 'done';", "Label", "string-literal union aliases cannot be generic", 6, None),
        ("interface I {}", "I", "declaration form outside the decided surface", 1, None),
        ("const [a, b] = [1, 2];", "a, b", "a binding pattern binds inside a function body; a declaration outside one binds one name", 7, Some(Divergence::ModuleLevelPattern)),
    ] {
        for local in [false, true] {
            let source = if local { format!("{declaration}\nexport {{ {names} }};") } else { format!("export {declaration}") };
            let mut files = [
                SourceFile::new("main.ts", format!("import {{ {names} }} from './surface';")),
                SourceFile::new("direct.ts", format!("import {{ {names} }} from './lib';")),
                SourceFile::new("surface.ts", format!("export {{ {names} }} from './lib';")),
                SourceFile::new("lib.ts", source),
            ];
            let mut expected = Diagnostic::new(RuleCode::S100, message, Pos::new("lib.ts", 1, col + if local { 0 } else { 7 }));
            expected.divergence = divergence;
            assert_eq!(check_program(&files).unwrap_err(), vec![expected], "{declaration}, local={local}");
            files[3].source = names.split(", ").map(|name| format!("export const {name}: i32 = 1;")).collect::<Vec<_>>().join("\n");
            check_program(&files).expect("supported declaration control");
        }
    }
    eprintln!("rejected declaration edges: {:?}", start.elapsed());
}

#[test]
fn rejected_export_forms_do_not_resolve_missing_sources() {
    use subscript_compiler::{Diagnostic, Pos};
    // Five checker calls cover the rejected forms without loading or code generation.
    let start = std::time::Instant::now();
    for (source, message, col) in [
        (
            "export type { T } from './absent';",
            "type-only exports are outside the named module surface",
            1,
        ),
        (
            "export { type T } from './absent';",
            "type-only and default exports are outside the named module surface",
            10,
        ),
        (
            "export { default } from './absent';",
            "type-only and default exports are outside the named module surface",
            10,
        ),
        (
            "export * from './absent';",
            "the module surface requires named exports",
            1,
        ),
        (
            "export * as ns from './absent';",
            "the module surface requires named exports",
            8,
        ),
    ] {
        let mut expected = Diagnostic::new(RuleCode::S100, message, Pos::new("main.ts", 1, col));
        expected.divergence = Some(Divergence::NamedModuleSurface);
        assert_eq!(
            check_program(&[SourceFile::new("main.ts", source)]).unwrap_err(),
            vec![expected],
            "{source}"
        );
    }
    eprintln!("rejected export sources: {:?}", start.elapsed());
}
