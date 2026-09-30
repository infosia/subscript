//! A type-only import binds a type only (compiler.md §134).

use subscript_compiler::{check_program, divergence::Divergence, Diagnostic, RuleCode, SourceFile};

const LIB: &str = "export class Box { value: i32 = 1; static s: i32 = 2; static make(): Box { return new Box(); } }
export function mk(): i32 { return 2; }
export let g: i32 = 3;
export const arr: i32[] = [1, 2];
export enum E { A, B }
export class Msg { value: i32 = 0; }
export function entry(inbox: Inbox<Msg>, outbox: Outbox<Msg>): void {}
export async function af(): Promise<i32> { return 1; }";

fn files(main: &str) -> [SourceFile; 2] {
    [
        SourceFile::entry("main.ts", main),
        SourceFile::new("lib.ts", LIB),
    ]
}

/// Builds a program whose third line holds `line`, with `name` imported
/// by `import` (`{N}` stands for the name).
fn program(import: &str, name: &str, line: &str) -> String {
    format!(
        "{} from \"./lib\";\nimport type {{ Msg as M }} from \"./lib\";\n{line}\nexport function main(): void {{}}",
        import.replace("{N}", name)
    )
}

const TYPE_ONLY_SPELLINGS: [&str; 2] = ["import type { {N} }", "import { type {N} }"];

/// Each value use form of §134 rule 2: the imported name, the line, and
/// the text in the line where the use starts.
const USES: &[(&str, &str, &str)] = &[
    ("mk", "function u(): i32 { return mk(); }", "mk()"),
    (
        "Box",
        "function u(): i32 { const b: Box = new Box(); return b.value; }",
        "Box()",
    ),
    ("g", "function u(): void { print(`${g}`); }", "g}"),
    (
        "E",
        "function u(): i32 { const e: E = E.A; return 0; }",
        "E.A",
    ),
    ("Box", "function u(): i32 { return Box.s; }", "Box.s"),
    (
        "Box",
        "function u(): i32 { const b: Box = Box.make(); return b.value; }",
        "Box.make",
    ),
    (
        "mk",
        "function u(): i32 { const f: () => i32 = mk; return f(); }",
        "mk;",
    ),
    ("Box", "function u(): void { Box.s = 5; }", "Box.s"),
    ("arr", "function u(): i32[] { return [0, ...arr]; }", "arr]"),
    (
        "entry",
        "function u(): void { const w: Worker<M, M> = Worker.spawn(entry); w.close(); w.join(); }",
        "entry)",
    ),
    (
        "af",
        "async function u(): Promise<i32> { const v: i32 = await af(); return v; }",
        "af()",
    ),
];

#[test]
fn every_value_use_of_a_type_only_import_is_one_s100_at_the_use() {
    for (name, line, marker) in USES {
        for spelling in TYPE_ONLY_SPELLINGS {
            let main = program(spelling, name, line);
            let errors = check_program(&files(&main)).expect_err("a type-only value use");
            assert_eq!(errors.len(), 1, "{main}: {errors:?}");
            let error = &errors[0];
            assert_eq!(error.code, RuleCode::S100, "{main}");
            assert_eq!(
                error.message,
                format!(
                    "`{name}` cannot be used as a value because it was imported with `import type`"
                ),
                "{main}"
            );
            assert_eq!(error.divergence, None, "{main}");
            assert_eq!(error.pos.file, "main.ts");
            assert_eq!(error.pos.line, 3, "{main}");
            assert_eq!(
                error.pos.col,
                (line.find(marker).expect("marker") + 1) as u32,
                "{main}"
            );
        }
        let control = program("import { {N} }", name, line);
        check_program(&files(&control)).expect("an ordinary import is a value");
    }
}

#[test]
fn an_assignment_to_a_type_only_import_reports_only_the_type_only_use() {
    for line in [
        "function u(): void { g = 5; }",
        "function u(): void { g += 1; }",
        "function u(): void { g++; }",
    ] {
        for spelling in TYPE_ONLY_SPELLINGS {
            let main = program(spelling, "g", line);
            let errors = check_program(&files(&main)).expect_err("a type-only write");
            assert_eq!(
                diagnostic_list(&errors),
                vec![(
                    RuleCode::S100,
                    "`g` cannot be used as a value because it was imported with `import type`"
                        .to_string(),
                    3,
                    22
                )],
                "{main}"
            );
        }
        let control = program("import { {N} }", "g", line);
        let errors = check_program(&files(&control)).expect_err("an import is read-only");
        assert_eq!(
            diagnostic_list(&errors),
            vec![(
                RuleCode::S100,
                "cannot assign to `g` because it is an import".to_string(),
                3,
                22
            )],
            "{control}"
        );
    }
}

#[test]
fn a_renamed_type_only_import_reports_the_local_name() {
    for spelling in ["import type { Box as B }", "import { type Box as B }"] {
        let main = format!(
            "{spelling} from \"./lib\";\nexport function main(): void {{ const b: B = new B(); }}"
        );
        let errors = check_program(&files(&main)).expect_err("a renamed type-only use");
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(
            errors[0].message,
            "`B` cannot be used as a value because it was imported with `import type`"
        );
    }
}

#[test]
fn a_type_only_specifier_leaves_its_ordinary_neighbours_values() {
    let main = "import { type Box, mk } from \"./lib\";\n\
                export function main(): void { const v: i32 = mk(); const b: Box = new Box(); }";
    let errors = check_program(&files(main)).expect_err("only Box is type-only");
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0]
        .message
        .starts_with("`Box` cannot be used as a value"));
}

#[test]
fn a_type_only_binding_resolves_in_every_type_position() {
    for spelling in TYPE_ONLY_SPELLINGS {
        let imports = ["Box", "E", "Msg as M"]
            .map(|name| format!("{} from \"./lib\";", spelling.replace("{N}", name)))
            .join("\n");
        let main = format!(
            "{imports}\n\
             function annotation(): void {{ const b: Box | null = null; }}\n\
             function argument(): void {{ const m: Map<string, Box> = new Map<string, Box>(); }}\n\
             function ret(): Box | null {{ return null; }}\n\
             function param(b: Box, e: E): i32 {{ return b.value; }}\n\
             function list(bs: Box[]): i32 {{ return bs.length; }}\n\
             class GBox<T> {{ v: T; constructor(v: T) {{ this.v = v; }} }}\n\
             function generic(x: GBox<Box>): i32 {{ return x.v.value; }}\n\
             async function promised(): Promise<Box | null> {{ return null; }}\n\
             class Holder {{ b: Box | null = null; }}\n\
             function set(x: Set<E>): i32 {{ return x.size; }}\n\
             function worker(w: Worker<M, M>): void {{ w.close(); w.join(); }}\n\
             export function main(): void {{}}"
        );
        check_program(&files(&main)).expect("type positions resolve");
    }
}

#[test]
fn a_type_only_import_of_a_name_without_a_type_meaning_is_legal_until_used() {
    let body =
        "export function main(): void { const x: g | null = null; const y: mk | null = null; }";
    let control = format!("import {{ mk, g }} from \"./lib\";\n{body}");
    let control_errors = check_program(&files(&control)).expect_err("g and mk name no type");
    let expected = vec![
        (RuleCode::S016, "unknown type name `g`".to_string(), 2, 41),
        (RuleCode::S016, "unknown type name `mk`".to_string(), 2, 67),
    ];
    assert_eq!(diagnostic_list(&control_errors), expected, "{control}");
    for spelling in ["import type { mk, g }", "import { type mk, type g }"] {
        let main = format!("{spelling} from \"./lib\";\nexport function main(): void {{}}");
        check_program(&files(&main)).expect("an unused type-only import is legal");

        let main = format!("{spelling} from \"./lib\";\n{body}");
        let errors = check_program(&files(&main)).expect_err("g and mk name no type");
        assert_eq!(diagnostic_list(&errors), expected, "{main}");
    }
}

/// The code, message, line, and column of each diagnostic, in order.
fn diagnostic_list(errors: &[Diagnostic]) -> Vec<(RuleCode, String, u32, u32)> {
    errors
        .iter()
        .map(|error| {
            (
                error.code,
                error.message.clone(),
                error.pos.line,
                error.pos.col,
            )
        })
        .collect()
}

const MID: &str = "mid.ts";

fn re_export_program(mid: &str, main: &str) -> [SourceFile; 3] {
    [
        SourceFile::entry("main.ts", main),
        SourceFile::new(MID, mid),
        SourceFile::new("lib.ts", LIB),
    ]
}

#[test]
fn a_re_export_of_a_type_only_import_is_a_type_only_export_form() {
    let consumer = "import { Box } from \"./mid\";\n\
                    export function main(): void { const b: Box = new Box(); }";
    for (import, export, name) in [
        ("import type { Box }", "export { Box };", "Box"),
        ("import { type Box }", "export { Box };", "Box"),
        ("import type { Box as B }", "export { B as Box };", "B"),
    ] {
        let mid = format!("{import} from \"./lib\";\n{export}");
        let errors = check_program(&re_export_program(&mid, consumer))
            .expect_err("a type-only re-export is outside the surface");
        assert_eq!(errors.len(), 1, "{mid}: {errors:?}");
        let error = &errors[0];
        assert_eq!(error.code, RuleCode::S100);
        assert_eq!(
            error.message,
            format!(
                "`{name}` was imported with `import type`; its re-export is a type-only export, outside the named module surface"
            )
        );
        assert_eq!(error.divergence, Some(Divergence::NamedModuleSurface));
        assert!(!error.resolution);
        assert_eq!(
            (error.pos.file.as_str(), error.pos.line, error.pos.col),
            (MID, 2, 10)
        );
    }
    let control = "import { Box } from \"./lib\";\nexport { Box };";
    check_program(&re_export_program(control, consumer))
        .expect("a re-export of an ordinary import is in the surface");
}

#[test]
fn a_re_export_of_a_type_only_import_in_the_entry_module_is_the_rule_5_s100_only() {
    for spelling in ["import type { mk }", "import { type mk }"] {
        let main = format!(
            "{spelling} from \"./lib\";\nexport {{ mk }};\nexport function main(): void {{}}"
        );
        let errors = check_program(&files(&main)).expect_err("a type-only re-export");
        assert_eq!(errors.len(), 1, "{main}: {errors:?}");
        let error = &errors[0];
        assert_eq!(error.code, RuleCode::S100);
        assert_eq!(
            error.message,
            "`mk` was imported with `import type`; its re-export is a type-only export, outside the named module surface"
        );
        assert_eq!(error.divergence, Some(Divergence::NamedModuleSurface));
        assert_eq!(
            (error.pos.file.as_str(), error.pos.line, error.pos.col),
            ("main.ts", 2, 10)
        );
    }
    let control = "import { mk } from \"./lib\";\nexport { mk };\nexport function main(): void {}";
    let errors =
        check_program(&files(control)).expect_err("the entry module exports functions only");
    assert_eq!(
        diagnostic_list(&errors),
        vec![(
            RuleCode::S100,
            "entry export `mk`: host entries must return void; target `mk` in `lib.ts`".to_string(),
            2,
            10
        )]
    );
}

/// A catch binding and an Error-typed parameter as the left operand.
const INSTANCEOF_LINES: [&str; 2] = [
    "function u(o: Error): boolean { return o instanceof {N}; }",
    "function u(): void { try { print(\"x\"); } catch (e) { if (e instanceof {N}) { print(\"y\"); } } }",
];

#[test]
fn a_type_only_import_as_the_instanceof_right_operand_is_one_s100_at_the_operand() {
    for template in INSTANCEOF_LINES {
        let line = template.replace("{N}", "Box");
        let col = (line.find("instanceof Box").expect("marker") + "instanceof ".len() + 1) as u32;
        for spelling in TYPE_ONLY_SPELLINGS {
            let main = program(spelling, "Box", &line);
            let errors = check_program(&files(&main)).expect_err("a type-only value use");
            assert_eq!(
                diagnostic_list(&errors),
                vec![(
                    RuleCode::S100,
                    "`Box` cannot be used as a value because it was imported with `import type`"
                        .to_string(),
                    3,
                    col
                )],
                "{main}"
            );
            assert_eq!(errors[0].divergence, None, "{main}");
        }
        let control = program("import { {N} }", "Box", &line);
        let errors = check_program(&files(&control)).expect_err("Box is no Error class");
        assert_eq!(errors.len(), 1, "{control}: {errors:?}");
        assert_eq!(
            errors[0].message,
            "`instanceof` requires an Error-family class as its right operand"
        );
        assert_eq!(errors[0].divergence, Some(Divergence::InstanceofNonError));

        let error_class = program(
            "import type { {N} }",
            "Box",
            &template.replace("{N}", "RangeError"),
        );
        check_program(&files(&error_class)).expect("an Error-family class stays accepted");
    }
}
