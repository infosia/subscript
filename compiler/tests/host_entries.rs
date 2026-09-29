//! §129 host entry table and explicit program input controls.
use subscript_compiler::{check_program, lir, SourceFile, Type};

fn entry(name: &str, source: &str) -> SourceFile {
    SourceFile::entry(name, source)
}

#[test]
fn explicit_entry_survives_order_and_rejects_ambiguous_input() {
    let files = [
        entry("api.ts", "export { update as advance } from './lib';"),
        SourceFile::new("lib.ts", "export function update(n: i32): void {}"),
    ];
    let checked = check_program(&files).unwrap();
    let e = &checked.host_entries[0];
    assert_eq!(e.name, "advance");
    assert_eq!(e.pos.file, "api.ts");
    assert_eq!(e.signature.parameters, [Type::I32]);
    assert!(!e.signature.is_async);
    assert_eq!(
        checked
            .functions
            .iter()
            .find(|f| f.symbol == e.target)
            .unwrap()
            .name,
        "update"
    );
    let lowered = lir::HostEntry::from_checked(e, lir::FunctionId(7));
    assert_eq!(lowered.target, lir::FunctionId(7));
    assert_eq!(lowered.name, e.name);
    assert_eq!(lowered.signature, e.signature);
    assert_eq!(lowered.pos, e.pos);
    let mut reversed = files.to_vec();
    reversed.reverse();
    assert_eq!(
        check_program(&reversed).unwrap().host_entries,
        checked.host_entries
    );
    reversed[1].entry = false;
    let errors = check_program(&reversed).unwrap_err();
    assert!(errors[0].message.contains("must name its entry module"));
    reversed.reverse();
    assert_eq!(check_program(&reversed).unwrap_err(), errors);
    reversed.reverse();
    reversed[0].entry = true;
    reversed[1].entry = true;
    assert!(check_program(&reversed).unwrap_err()[0]
        .message
        .contains("exactly one"));
    let mut ambient = SourceFile::ambient("mirror.d.ts", "declare function foreign(): void;");
    ambient.entry = true;
    assert!(check_program(&[ambient]).unwrap_err()[0]
        .message
        .contains("ambient"));
    let ambient_entry = SourceFile::entry("mirror.d.ts", "declare function foreign(): void;");
    assert!(check_program(&[ambient_entry]).unwrap_err()[0]
        .message
        .contains("ambient"));
    for mut files in [
        vec![SourceFile::new("a.ts", ""), SourceFile::entry("z.d.ts", "")],
        vec![
            SourceFile::new("a.ts", ""),
            entry("z.ts", ""),
            entry("y.ts", ""),
        ],
    ] {
        let expected_file = if files.len() == 2 { "z.d.ts" } else { "y.ts" };
        let diagnostics = check_program(&files).unwrap_err();
        assert_eq!(diagnostics[0].pos.file, expected_file);
        files.reverse();
        assert_eq!(check_program(&files).unwrap_err(), diagnostics);
        for file in &mut files {
            file.entry = file.name == "a.ts";
        }
        assert!(check_program(&files).is_ok());
    }
    let single = check_program(&[SourceFile::new(
        "single.ts",
        "export function main(): void {}",
    )])
    .unwrap();
    assert_eq!(single.host_entries[0].name, "main");
}

#[test]
fn every_invalid_entry_kind_has_a_legal_module_export_control() {
    for (declaration, name) in [
        ("export class State { value: i32 = 0; }", "State"),
        ("export enum Kind { A = 1 }", "Kind"),
        ("export let count: i32 = 0;", "count"),
        ("export type Mode = 'a' | 'b';", "Mode"),
        (
            "export class Box<T> { value: T; constructor(value: T) { this.value = value; } }",
            "Box",
        ),
        ("export function generic<T>(): void {}", "generic"),
        ("export function read(): i32 { return 1; }", "read"),
        ("export function text(value: string): void {}", "text"),
        (
            "export function* sequence(): Generator<i32> { yield 1; }",
            "sequence",
        ),
        (
            "export async function step(value: i32): Promise<void> {}",
            "step",
        ),
    ] {
        let sources = [
            entry("api.ts", "import './lib';"),
            SourceFile::new("lib.ts", declaration),
        ];
        let module = check_program(&sources).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        assert!(module.host_entries.is_empty());
        let mut exposed = sources.to_vec();
        exposed[0].source = format!("export {{ {name} as exposed }} from './lib';");
        let errors = check_program(&exposed).expect_err(name);
        assert_eq!(errors.len(), 1, "{name}: {errors:?}");
        assert_eq!(errors[0].pos.file, "api.ts");
        assert!(errors[0].message.contains("entry export `exposed`"));
        assert!(
            check_program(&[SourceFile::new("direct.ts", declaration)]).is_err(),
            "{name}"
        );
    }
}

#[test]
fn runner_query_and_generic_target_diagnostics_have_source_positions() {
    let files = [
        SourceFile::entry("api.ts", "export { step as update } from './lib';"),
        SourceFile::new("lib.ts", "export function step(): void {}"),
    ];
    let module = check_program(&files).unwrap();
    assert_eq!(module.entry_pos.file, "api.ts");
    let error = module.runner_main().unwrap_err();
    assert_eq!(error.code, subscript_compiler::RuleCode::S100);
    assert_eq!(error.pos, module.entry_pos);
    assert_eq!(error.message, "entry module exports no host entry `main`");
    let mut good = files.clone();
    good[0].source = "export { step as main } from './lib';".into();
    assert_eq!(
        check_program(&good).unwrap().runner_main().unwrap().name,
        "main"
    );
    let arguments = check_program(&[SourceFile::entry(
        "api.ts",
        "export function main(n: i32): void {}",
    )])
    .unwrap();
    assert!(arguments
        .runner_main()
        .unwrap_err()
        .message
        .contains("main(): void"));
    let generic = [
        SourceFile::entry("api.ts", "export { work as launch } from './lib';"),
        SourceFile::new("lib.ts", "export function work<T>(): void {}"),
    ];
    let errors = check_program(&generic).unwrap_err();
    assert_eq!(errors[0].pos.file, "api.ts");
    assert!(errors[0].message.contains("`work` in `lib.ts`"));
    let mut private = generic.clone();
    private[0].source =
        "import { work } from './lib'; export function main(): void { work<i32>(); }".into();
    assert!(check_program(&private).is_ok());
}
