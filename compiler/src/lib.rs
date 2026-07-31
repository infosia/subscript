#![warn(missing_docs)]
//! subscript compiler front end (plan phase P1): SWC parse, semantic
//! checker for the collision rules (`specs/blocks/collisions.md`), and
//! the typed HIR.
//!
//! The primary public entry point is [`check_program`]: it takes one or
//! more source files (multi-file programs use `import`/`export`, e.g.
//! the `a19-modules` corpus entry) and returns either a typed
//! [`hir::Module`] or a non-empty list of [`Diagnostic`]s with stable
//! rule codes (S001–S013, S100) and TS positions. Loaders can use
//! [`parse_import_specifiers`] to discover imports with the same parser.

pub mod diag;
mod diag_render;
pub mod hir;
pub mod types;
pub mod api_reference;

mod ambient;
mod check;
mod parse;
mod provenance;
mod regex;
mod trap_sites;
mod warn;

pub use diag::{Diagnostic, Pos, RuleCode};
pub use diag_render::{render_diagnostics, render_warnings};
pub use parse::parse_import_specifiers;
pub use types::{ClassId, EnumId, FuncType, StringAliasId, Type};
pub use warn::{check_warnings, WarnCode, Warning};

/// One source file of a program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFile {
    /// File name used in diagnostics and for import resolution (the
    /// base name without `.ts` is the module stem).
    pub name: String,
    /// Full source text.
    pub source: String,
    /// True for an ambient declaration file (`.d.ts`): parsed in ambient
    /// mode, and its top-level declarations become a global ambient
    /// source (the generated C-header mirror, P5.2) rather than a
    /// checked program module.
    pub dts: bool,
}

impl SourceFile {
    /// Builds a program source file (ordinary `.ts`).
    #[must_use]
    pub fn new(name: impl Into<String>, source: impl Into<String>) -> Self {
        SourceFile {
            name: name.into(),
            source: source.into(),
            dts: false,
        }
    }

    /// Builds an ambient declaration source (`.d.ts`): parsed in ambient
    /// mode; its declarations join the global ambient surface (P5.2
    /// mirror ingestion), visible to every program file without import.
    #[must_use]
    pub fn ambient(name: impl Into<String>, source: impl Into<String>) -> Self {
        SourceFile {
            name: name.into(),
            source: source.into(),
            dts: true,
        }
    }
}

/// Checks a program.
///
/// On success every accepted construct resolves to a typed HIR module:
/// every expression carries its resolved type and TS position, and
/// generic declarations are already monomorphized. On rejection the
/// diagnostic list is non-empty; each entry carries a stable rule code
/// and the position of the offending construct.
///
/// # Errors
///
/// Returns the diagnostic list when the program parses with errors or
/// violates any language rule.
pub fn check_program(files: &[SourceFile]) -> Result<hir::Module, Vec<Diagnostic>> {
    if files.is_empty() {
        return Err(vec![Diagnostic::new(
            RuleCode::S100,
            "no source files given",
            Pos::new(String::new(), 1, 1),
        )]);
    }
    swc_common::GLOBALS.set(&swc_common::Globals::new(), || {
        let parsed = parse::parse_program(files)?;
        check::run(&parsed)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check_one(src: &str) -> Result<hir::Module, Vec<Diagnostic>> {
        check_program(&[SourceFile::new("test.ts", src)])
    }

    #[test]
    fn empty_program_list_is_an_error() {
        let err = check_program(&[]).unwrap_err();
        assert_eq!(err[0].code, RuleCode::S100);
    }

    #[test]
    fn minimal_program_checks_clean() {
        let module = check_one(
            "export function main(): void {\n  print(\"hello\");\n}\n",
        )
        .expect("clean check");
        assert_eq!(module.functions.len(), 1);
        assert_eq!(module.functions[0].name, "main");
        assert!(module.functions[0].exported);
        assert_eq!(module.functions[0].ret, Type::Void);
    }

    #[test]
    fn invalid_regex_literal_is_a_checker_diagnostic() {
        let diagnostics = check_one("export function main(): void {\n  const regex = /(/;\n}\n")
            .expect_err("invalid literal must be rejected by the checker");
        assert_eq!(diagnostics[0].code, RuleCode::S100);
        assert_eq!((diagnostics[0].pos.line, diagnostics[0].pos.col), (2, 17));
        assert!(
            diagnostics[0]
                .message
                .contains("invalid regular-expression literal"),
            "diagnostic: {}",
            diagnostics[0].message
        );
    }

    #[test]
    fn each_regex_literal_site_has_one_module_global_handle() {
        let module = check_one(
            "export function main(): void {\n\
             \x20 const first: RegExp = /x/g;\n\
             \x20 const second: RegExp = /x/g;\n\
             \x20 print(`${first.test(\"x\")} ${second.source}`);\n\
             }\n",
        )
        .expect("regex literals check");
        let regex_globals: Vec<&hir::Global> = module
            .globals
            .iter()
            .filter(|global| global.ty == Type::RegExp)
            .collect();
        assert_eq!(regex_globals.len(), 2);
        for global in regex_globals {
            assert!(global.name.starts_with("__subscript_regex_literal_"));
            assert!(matches!(
                &global.init.kind,
                hir::ExprKind::Call {
                    callee: hir::Callee::Regex(hir::RegexFn::New),
                    ..
                }
            ));
        }
    }

    #[test]
    fn replace_all_rejects_a_non_global_regex_literal_early() {
        let diagnostics =
            check_one("export function main(): void {\n  print(\"aaa\".replaceAll(/a/, \"Z\"));\n}\n")
                .expect_err("literal without g must be rejected by the checker");
        assert_eq!(diagnostics[0].code, RuleCode::S100);
        assert_eq!((diagnostics[0].pos.line, diagnostics[0].pos.col), (2, 26));
        assert!(diagnostics[0].message.contains("requires the `g` flag"));
    }

    #[test]
    fn bare_number_is_s007_with_position() {
        let err = check_one("const x: number = 1;\n").unwrap_err();
        assert_eq!(err[0].code, RuleCode::S007);
        assert_eq!(err[0].pos.file, "test.ts");
        assert_eq!(err[0].pos.line, 1);
        assert_eq!(err[0].pos.col, 10);
    }

    #[test]
    fn any_is_s001() {
        let err = check_one("const x: any = 1;\n").unwrap_err();
        assert_eq!(err[0].code, RuleCode::S001);
    }

    #[test]
    fn literal_overflow_is_s008() {
        let err = check_one("const x: i32 = 3000000000;\n").unwrap_err();
        assert_eq!(err[0].code, RuleCode::S008);
    }

    #[test]
    fn narrow_literal_ranges_are_checked() {
        for src in [
            "const x: i8 = 128;\n",
            "const x: u8 = -1;\n",
            "const x: i16 = 32768;\n",
            "const x: u16 = 65536;\n",
            "const x: f16 = 65520.0;\n",
        ] {
            let err = check_one(src).unwrap_err();
            assert_eq!(err[0].code, RuleCode::S008, "{src}");
        }
        check_one("const x: f16 = 65505.0;\nexport function main(): void {}\n")
            .expect("a finite-rounding f16 literal is accepted");
    }

    #[test]
    fn fractional_literal_in_integer_context_is_s008() {
        let err = check_one("const x: i32 = 1.5;\n").unwrap_err();
        assert_eq!(err[0].code, RuleCode::S008);
    }

    #[test]
    fn mixed_arithmetic_without_as_is_s007() {
        let err = check_one(
            "export function main(): void {\n  const a: i32 = 1;\n  const b: u32 = 2;\n  const c: i32 = a + b;\n  print(`${c}`);\n}\n",
        )
        .unwrap_err();
        assert_eq!(err[0].code, RuleCode::S007);
        assert_eq!(err[0].pos.line, 4);
    }

    #[test]
    fn f16_arithmetic_is_s014_with_compute_via_f32_guidance() {
        for body in [
            "const c: f16 = a + b;",
            "const c: f16 = a % b;",
            "const c: f16 = -a;",
            "a += b;",
            "a++;",
        ] {
            let src = format!(
                "export function main(): void {{\n  let a: f16 = 1.0;\n  const b: f16 = 2.0;\n  {body}\n}}\n"
            );
            let err = check_one(&src).unwrap_err();
            assert_eq!(err[0].code, RuleCode::S014, "{body}");
            assert!(err[0].message.contains("as f32"), "{body}");
        }
    }

    #[test]
    fn context_free_integer_literal_defaults_to_i32() {
        let module = check_one(
            "export function main(): void {\n  const x = 3;\n  print(`${x}`);\n}\n",
        )
        .expect("clean");
        let hir::Stmt::Let { ty, .. } = &module.functions[0].body[0] else {
            panic!("expected let");
        };
        assert_eq!(*ty, Type::I32);
    }

    #[test]
    fn context_free_fractional_literal_defaults_to_f64() {
        let module = check_one(
            "export function main(): void {\n  const x = 1.5;\n  print(`${x}`);\n}\n",
        )
        .expect("clean");
        let hir::Stmt::Let { ty, .. } = &module.functions[0].body[0] else {
            panic!("expected let");
        };
        assert_eq!(*ty, Type::F64);
    }

    #[test]
    fn undefined_is_s012() {
        let err = check_one("let x: i32 | undefined = undefined;\n").unwrap_err();
        assert_eq!(err[0].code, RuleCode::S012);
    }

    #[test]
    fn general_union_is_s011() {
        let err = check_one("let x: i32 | string = 1;\n").unwrap_err();
        assert_eq!(err[0].code, RuleCode::S011);
    }

    #[test]
    fn string_literal_union_members_are_contextually_typed() {
        let module = check_one(
            "type Format = \"a\" | \"b\";\n\
             @CStruct\n\
             class Box {\n\
               value: Format;\n\
               constructor(value: Format) { this.value = value; }\n\
             }\n\
             function pass(value: Format): Format { return value; }\n\
             export function main(): void {\n\
               const value: Format = \"a\";\n\
               const values: Format[] = [\"a\", \"b\"];\n\
               const box: Box = new Box(\"b\");\n\
               print(`${pass(value)}:${values[1]}:${box.value}:${value === \"a\"}`);\n\
             }\n",
        )
        .expect("Q32 member literals check cleanly in every contextual position");
        assert_eq!(module.string_aliases.len(), 1);
        assert_eq!(module.string_aliases[0].members, ["a", "b"]);
        let main = module
            .functions
            .iter()
            .find(|function| function.name == "main")
            .expect("main");
        let hir::Stmt::Let { ty, init, .. } = &main.body[0] else {
            panic!("first statement is a binding");
        };
        assert_eq!(*ty, Type::StringAlias(StringAliasId(0)));
        assert_eq!(init.kind, hir::ExprKind::Int(0));
    }

    #[test]
    fn string_literal_union_nonmember_is_rejected() {
        let diagnostics = check_one(
            "type Format = \"a\" | \"b\";\n\
             export function main(): void {\n\
               const value: Format = \"c\";\n\
             }\n",
        )
        .expect_err("non-member literal must be rejected");
        assert_eq!(diagnostics[0].code, RuleCode::S100);
        assert_eq!(diagnostics[0].pos.line, 3);
    }

    #[test]
    fn same_membered_string_literal_unions_are_nominal() {
        let diagnostics = check_one(
            "type Left = \"a\" | \"b\";\n\
             type Right = \"a\" | \"b\";\n\
             export function main(): void {\n\
               const left: Left = \"a\";\n\
               const right: Right = left;\n\
             }\n",
        )
        .expect_err("same-membered aliases must remain distinct");
        assert_eq!(diagnostics[0].code, RuleCode::S100);
        assert_eq!(diagnostics[0].pos.line, 5);
    }

    #[test]
    fn string_literal_union_is_rejected_in_a_boundary_signature() {
        let diagnostics = check_program(&[
            SourceFile::ambient(
                "boundary.d.ts",
                "type Format = \"a\" | \"b\";\n\
                 declare class Boundary {\n\
                   value: Format;\n\
                   constructor(value: Format);\n\
                 }\n",
            ),
            SourceFile::new(
                "test.ts",
                "export function main(): void { print(\"ok\"); }\n",
            ),
        ])
        .expect_err("Q32 aliases are not boundary types");
        assert_eq!(diagnostics[0].code, RuleCode::S100);
        assert!(
            diagnostics[0]
                .message
                .contains("cannot appear in a boundary signature"),
            "{}",
            diagnostics[0].message
        );

        let diagnostics = check_one(
            "type Format = \"a\" | \"b\";\n\
             export function boundary(value: Format): void { print(`${value}`); }\n\
             export function main(): void { print(\"ok\"); }\n",
        )
        .expect_err("Q32 aliases are not exported boundary types");
        assert_eq!(diagnostics[0].code, RuleCode::S100);
        assert!(
            diagnostics[0].message.contains("boundary signature"),
            "{}",
            diagnostics[0].message
        );
    }

    #[test]
    fn descriptor_required_members_and_defaults_reach_hir() {
        let module = check_one(
            "type Mode = \"fast\" | \"safe\";\n\
             @Descriptor\n\
             class Child {\n\
               value?: i32 = 7;\n\
             }\n\
             @Descriptor\n\
             class Options {\n\
               count!: i32;\n\
               child?: Child = {};\n\
               mode?: Mode = \"safe\";\n\
             }\n\
             export function main(): void {\n\
               const options: Options = { count: 3 };\n\
               print(`${options.count}:${options.mode}`);\n\
             }\n",
        )
        .expect("required-present descriptor literal and defaults check cleanly");
        let options = module
            .classes
            .iter()
            .find(|class| class.name == "Options")
            .expect("Options class");
        assert!(options.is_descriptor);
        assert!(!options.is_value);
        assert!(!options.fields[0].is_defaulted);
        assert!(options.fields[1].is_defaulted);
        assert!(matches!(
            options.fields[1].init.as_ref().map(|expr| &expr.kind),
            Some(hir::ExprKind::DescriptorLit { .. })
        ));
        let main = module
            .functions
            .iter()
            .find(|function| function.name == "main")
            .expect("main");
        let hir::Stmt::Let { init, .. } = &main.body[0] else {
            panic!("first statement is the descriptor binding");
        };
        let hir::ExprKind::DescriptorLit { fields, .. } = &init.kind else {
            panic!("object literal lowered to DescriptorLit HIR");
        };
        assert!(fields[0].is_some(), "required member is explicit");
        assert!(fields[1].is_none(), "nested member takes its default");
        assert!(fields[2].is_none(), "Q32 alias member takes its default");
    }

    #[test]
    fn descriptor_missing_and_excess_members_are_rejected() {
        let missing = check_one(
            "@Descriptor\n\
             class Options { count!: i32; }\n\
             export function main(): void {\n\
               const options: Options = {};\n\
             }\n",
        )
        .expect_err("missing required descriptor member");
        assert_eq!(missing[0].code, RuleCode::S100);
        assert!(missing[0].message.contains("missing required member `count`"));

        let excess = check_one(
            "@Descriptor\n\
             class Options { count!: i32; }\n\
             export function main(): void {\n\
               const options: Options = { count: 1, extra: 2 };\n\
             }\n",
        )
        .expect_err("excess descriptor member");
        assert_eq!(excess[0].code, RuleCode::S004);
        assert!(excess[0].message.contains("no declared property `extra`"));
    }

    #[test]
    fn object_literal_for_unmarked_class_remains_nominally_rejected() {
        let diagnostics = check_one(
            "class Options { count!: i32; }\n\
             export function main(): void {\n\
               const options: Options = { count: 1 };\n\
             }\n",
        )
        .expect_err("unmarked class must not be literal-constructible");
        assert_eq!(diagnostics[0].code, RuleCode::S005);
    }

    #[test]
    fn descriptor_member_forms_are_exact() {
        let optional_without_default = check_one(
            "@Descriptor\n\
             class Options { count?: i32; }\n\
             export function main(): void {}\n",
        )
        .expect_err("optional descriptor member without default");
        assert_eq!(optional_without_default[0].code, RuleCode::S012);

        let definite_with_default = check_one(
            "@Descriptor\n\
             class Options { count!: i32 = 1; }\n\
             export function main(): void {}\n",
        )
        .expect_err("required descriptor member with initializer");
        assert_eq!(definite_with_default[0].code, RuleCode::S100);

        let initializer_without_optional = check_one(
            "@Descriptor\n\
             class Options { count: i32 = 1; }\n\
             export function main(): void {}\n",
        )
        .expect_err("descriptor initializer without optional spelling");
        assert_eq!(initializer_without_optional[0].code, RuleCode::S100);
    }

    #[test]
    fn descriptor_explicit_undefined_stays_rejected() {
        let diagnostics = check_one(
            "@Descriptor\n\
             class Options { count?: i32 = 1; }\n\
             export function main(): void {\n\
               const options: Options = { count: undefined };\n\
             }\n",
        )
        .expect_err("undefined cannot be supplied explicitly");
        assert_eq!(diagnostics[0].code, RuleCode::S012);
    }

    #[test]
    fn descriptor_methods_are_rejected() {
        let diagnostics = check_one(
            "@Descriptor\n\
             class Options {\n\
               count?: i32 = 1;\n\
               getCount(): i32 { return this.count; }\n\
             }\n\
             export function main(): void {}\n",
        )
        .expect_err("descriptor method");
        assert_eq!(diagnostics[0].code, RuleCode::S100);
        assert!(diagnostics[0].message.contains("cannot declare methods"));
    }

    #[test]
    fn throw_is_s010() {
        let err =
            check_one("export function main(): void {\n  throw \"x\";\n}\n").unwrap_err();
        assert_eq!(err[0].code, RuleCode::S010);
        assert_eq!(
            err[0].message,
            "exceptions are not in the language; return a result value"
        );
        assert_eq!(err[0].pos.line, 2);
    }

    #[test]
    fn async_is_s013() {
        let err = check_one("async function f(): Promise<void> {}\n").unwrap_err();
        assert_eq!(err[0].code, RuleCode::S013);
    }

    #[test]
    fn eval_is_s002() {
        let err = check_one("export function main(): void {\n  eval(\"1\");\n}\n").unwrap_err();
        assert_eq!(err[0].code, RuleCode::S002);
    }

    #[test]
    fn nonwhitelisted_array_member_is_s100_naming_the_member() {
        let err = check_one(
            "export function main(): void {\n  const xs: i32[] = [1];\n  xs.map;\n}\n",
        )
        .unwrap_err();
        assert_eq!(err[0].code, RuleCode::S100);
        assert!(err[0].message.contains("map"));
    }

    #[test]
    fn nonwhitelisted_string_member_is_s100_naming_the_member() {
        // A member outside both the accepted §8 surface and the named
        // Q21 rejected set stays the generic S100 surface diagnostic
        // (`toUpperCase`, this test's former subject, joined the
        // accepted surface in P10).
        let err = check_one(
            "export function main(): void {\n  const s: string = \"a\";\n  print(s.reverse());\n}\n",
        )
        .unwrap_err();
        assert_eq!(err[0].code, RuleCode::S100);
        assert!(err[0].message.contains("reverse"));
    }

    #[test]
    fn string_methods_type_and_normalize_optional_arguments() {
        // stdlib.md §8: every accepted method resolves to a Callee::Str
        // intrinsic with the receiver first; the optional arguments are
        // normalized at check time (start positions → 0, ending
        // positions → i32::MAX, `pad` → " ") so each runtime symbol
        // has a fixed arity.
        let module = check_one(
            "export function main(): void {\n  const s: string = \"ab\";\n  const sl: string = s.slice();\n  const i: i32 = s.indexOf(\"a\");\n  const b: boolean = s.includes(\"a\", 1);\n  const p: string = s.padStart(5);\n  const parts: string[] = s.split(\"a\");\n  const c: i32 = s.charCodeAt(0);\n  const sub: string = s.substring(1);\n  const at: string = s.charAt(0);\n  const cp: i32 = s.codePointAt(0);\n  const cat: string = s.concat(at);\n  const start: boolean = s.startsWith(\"a\");\n  const end: boolean = s.endsWith(\"b\");\n  print(`${sl}${i}${b}${p}${parts.length}${c}${sub}${cp}${cat}${start}${end}`);\n}\n",
        )
        .expect("clean check");
        let mut found = Vec::new();
        fn walk(e: &hir::Expr, found: &mut Vec<(hir::StrFn, usize)>) {
            if let hir::ExprKind::Call { callee, args } = &e.kind {
                if let hir::Callee::Str(f) = callee {
                    found.push((*f, args.len()));
                }
                for a in args {
                    walk(a, found);
                }
            }
        }
        for s in &module.functions[0].body {
            match s {
                hir::Stmt::Let { init, .. } => walk(init, &mut found),
                hir::Stmt::Expr(e) => walk(e, &mut found),
                _ => {}
            }
        }
        for f in [
            hir::StrFn::Slice,
            hir::StrFn::IndexOf,
            hir::StrFn::Includes,
            hir::StrFn::PadStart,
            hir::StrFn::Split,
            hir::StrFn::CharCodeAt,
            hir::StrFn::Substring,
            hir::StrFn::CharAt,
            hir::StrFn::CodePointAt,
            hir::StrFn::Concat,
            hir::StrFn::StartsWith,
            hir::StrFn::EndsWith,
        ] {
            let (_, arity) = found
                .iter()
                .find(|(g, _)| *g == f)
                .unwrap_or_else(|| panic!("no Callee::Str({}) call", f.name()));
            assert_eq!(*arity, 1 + f.params().len(), "arity of {}", f.name());
        }
    }

    #[test]
    fn rejected_string_member_is_s014_naming_the_member() {
        for (member, call, q_rule) in [
            ("normalize", "s.normalize()", "Q21"),
            ("localeCompare", "s.localeCompare(s)", "Q21"),
            ("toLocaleLowerCase", "s.toLocaleLowerCase()", "Q21"),
            ("matchAll", "s.matchAll(s)", "Q31"),
            ("search", "s.search(s)", "Q31"),
        ] {
            let err = check_one(&format!(
                "export function main(): void {{\n  const s: string = \"a\";\n  {call};\n}}\n"
            ))
            .unwrap_err();
            assert_eq!(err[0].code, RuleCode::S014, "{member}");
            assert!(err[0].message.contains(member), "{member}: {}", err[0].message);
            assert!(
                err[0].message.contains(q_rule),
                "{member}: {}",
                err[0].message
            );
        }
    }

    #[test]
    fn string_method_read_as_a_value_is_rejected() {
        let err = check_one(
            "export function main(): void {\n  const s: string = \"a\";\n  s.indexOf;\n}\n",
        )
        .unwrap_err();
        assert_eq!(err[0].code, RuleCode::S100);
        assert!(err[0].message.contains("only be called"));
    }

    #[test]
    fn string_as_a_global_value_or_constructor_is_rejected() {
        // stdlib.md §8: `String` is not an accepted global. It resolves
        // nowhere, so each use fails on the standing unknown-name /
        // unsupported-construct paths — no dedicated S-code needed.
        for src in [
            "export function main(): void {\n  print(String(3));\n}\n",
            "export function main(): void {\n  print(String.fromCharCode(65));\n}\n",
            "export function main(): void {\n  print(String.raw`x`);\n}\n",
            "export function main(): void {\n  const s = new String(\"a\");\n  print(\"x\");\n}\n",
        ] {
            let err = check_one(src).unwrap_err();
            assert!(!err.is_empty(), "{src} was accepted");
        }
    }

    #[test]
    fn array_methods_type_and_normalize_optional_arguments() {
        // stdlib.md §9: every accepted method resolves to a Callee::Arr
        // intrinsic with the receiver first; the optional arguments are
        // normalized at check time (join separator -> ",",
        // slice/fill/copyWithin end -> the END_SENTINEL) so each runtime
        // symbol has a fixed arity; map's `U` is inferred from the
        // closure.
        let module = check_one(
            "export function main(): void {\n  const xs: i32[] = [1, 2, 3];\n  const i: i32 = xs.indexOf(2);\n  const s: string = xs.join();\n  const sl: i32[] = xs.slice(1);\n  const fl: i32[] = xs.fill(0);\n  const m: string[] = xs.map((v: i32) => `${v}`);\n  const r: string = xs.reduce((acc: string, v: i32): string => acc + `${v}`, \"#\");\n  const rr: string = xs.reduceRight((acc: string, v: i32): string => acc + `${v}`, \"#\");\n  const sp: i32[] = xs.splice(0, 1);\n  const sh: i32 = xs.shift();\n  const us: i32 = xs.unshift(0);\n  const cw: i32[] = xs.copyWithin(0, 1);\n  print(`${i}${s}${sl.length}${fl.length}${m.length}${r}${rr}${sp.length}${sh}${us}${cw.length}`);\n}\n",
        )
        .expect("clean check");
        let mut found = Vec::new();
        fn walk(e: &hir::Expr, found: &mut Vec<(hir::ArrFn, usize, Type)>) {
            if let hir::ExprKind::Call { callee, args } = &e.kind {
                if let hir::Callee::Arr(f) = callee {
                    found.push((*f, args.len(), e.ty.clone()));
                }
                for a in args {
                    walk(a, found);
                }
            }
        }
        for s in &module.functions[0].body {
            match s {
                hir::Stmt::Let { init, .. } => walk(init, &mut found),
                hir::Stmt::Expr(e) => walk(e, &mut found),
                _ => {}
            }
        }
        let get = |f: hir::ArrFn| {
            found
                .iter()
                .find(|(g, _, _)| *g == f)
                .unwrap_or_else(|| panic!("no Callee::Arr({}) call", f.name()))
                .clone()
        };
        assert_eq!(get(hir::ArrFn::IndexOf).1, 2); // recv + needle
        assert_eq!(get(hir::ArrFn::Join).1, 2); // recv + defaulted ","
        assert_eq!(get(hir::ArrFn::Slice).1, 3); // recv + start + end
        assert_eq!(get(hir::ArrFn::Fill).1, 4); // recv + x + start + end
        // map's U is inferred from the closure: string[].
        assert_eq!(get(hir::ArrFn::Map).2, Type::Array(Box::new(Type::Str)));
        // reduce's result is the init's type.
        assert_eq!(get(hir::ArrFn::Reduce).2, Type::Str);
        assert_eq!(get(hir::ArrFn::Reduce).1, 3); // recv + callback + init
        assert_eq!(get(hir::ArrFn::ReduceRight).2, Type::Str);
        assert_eq!(get(hir::ArrFn::ReduceRight).1, 3);
        assert_eq!(get(hir::ArrFn::Splice).1, 3); // recv + start + deleteCount
        assert_eq!(get(hir::ArrFn::Shift).1, 1); // receiver only
        assert_eq!(get(hir::ArrFn::Unshift).1, 2); // recv + value
        assert_eq!(get(hir::ArrFn::CopyWithin).1, 4); // recv + target + start + end
    }

    #[test]
    fn rejected_array_member_is_s014_naming_the_member() {
        for (member, call, q_rule) in [
            ("sort", "xs.sort()", "Q22"),
            (
                "reduce",
                "xs.reduce((acc: i32, v: i32): i32 => acc + v)",
                "Q22",
            ),
            ("find", "xs.find((v: i32): boolean => v > 1)", "Q22"),
            (
                "findLast",
                "xs.findLast((v: i32): boolean => v > 1)",
                "Q22",
            ),
            ("flat", "xs.flat()", "Q22"),
            ("keys", "xs.keys()", "Q30"),
        ] {
            let err = check_one(&format!(
                "export function main(): void {{\n  const xs: i32[] = [1, 2, 3];\n  {call};\n}}\n"
            ))
            .unwrap_err();
            assert_eq!(err[0].code, RuleCode::S014, "{member}");
            assert!(err[0].message.contains(member), "{member}: {}", err[0].message);
            assert!(
                err[0].message.contains(q_rule),
                "{member}: {}",
                err[0].message
            );
        }
    }

    #[test]
    fn array_callbacks_accept_the_q27_index_arity() {
        check_one(
            "function indexedMap(v: i32, i: i32): i32 { return v + i; }\n\
             export function main(): void {\n\
               const xs: i32[] = [1, 2, 3];\n\
               xs.forEach((v: i32, i: i32): void => { print(`${v}:${i}`); });\n\
               const m: i32[] = xs.map(indexedMap);\n\
               xs.filter((v: i32, i: i32): boolean => v > i);\n\
               xs.some((v: i32, i: i32): boolean => v === i);\n\
               xs.every((v: i32, i: i32): boolean => v > i);\n\
               xs.findIndex((v: i32, i: i32): boolean => v === i);\n\
               xs.reduce((acc: i32, v: i32, i: i32): i32 => acc + v + i, 0);\n\
               xs.reduceRight((acc: i32, v: i32, i: i32): i32 => acc + v + i, 0);\n\
               print(`${m.length}`);\n\
             }\n",
        )
        .expect("Q27 indexed Array callbacks check");
    }

    #[test]
    fn fixed_array_callback_family_accepts_both_q27_arities_and_dynamic_results() {
        check_one(
            "function indexedMap(v: i32, i: i32): string { return `${i}:${v}`; }\n\
             export function main(): void {\n\
               const xs: FixedArray<i32, 3> = [1, 2, 3];\n\
               xs.forEach((v: i32): void => { print(`${v}`); });\n\
               xs.forEach((v: i32, i: i32): void => { print(`${i}:${v}`); });\n\
               const m1: i32[] = xs.map((v: i32): i32 => v * 2);\n\
               const m2: string[] = xs.map(indexedMap);\n\
               const f1: i32[] = xs.filter((v: i32): boolean => v > 1);\n\
               const f2: i32[] = xs.filter((v: i32, i: i32): boolean => v > i);\n\
               xs.some((v: i32): boolean => v === 2);\n\
               xs.some((v: i32, i: i32): boolean => v === i);\n\
               xs.every((v: i32): boolean => v > 0);\n\
               xs.every((v: i32, i: i32): boolean => v > i);\n\
               xs.findIndex((v: i32): boolean => v === 3);\n\
               xs.findIndex((v: i32, i: i32): boolean => v === i);\n\
               const r1: i32 = xs.reduce((a: i32, v: i32): i32 => a + v, 0);\n\
               const r2: string = xs.reduce((a: string, v: i32, i: i32): string => a + `${i}:${v}`, \"\");\n\
               const rr1: i32 = xs.reduceRight((a: i32, v: i32): i32 => a + v, 0);\n\
               const rr2: string = xs.reduceRight((a: string, v: i32, i: i32): string => a + `${i}:${v}`, \"\");\n\
               print(`${m1.length}${m2.length}${f1.length}${f2.length}${r1}${r2}${rr1}${rr2}`);\n\
             }\n",
        )
        .expect("Q27 FixedArray callback family checks");
    }

    #[test]
    fn array_callback_container_parameter_is_s014_naming_c5() {
        let err = check_one(
            "export function main(): void {\n  const xs: i32[] = [1, 2, 3];\n  const m: i32[] = xs.map((v: i32, i: i32, arr: i32[]): i32 => v + i + arr.length);\n  print(`${m.length}`);\n}\n",
        )
        .unwrap_err();
        assert_eq!(err[0].code, RuleCode::S014);
        assert!(err[0].message.contains("container"));
        assert!(err[0].message.contains("C5"));
        assert!(err[0].message.contains("non-escaping-by-construction"));
        assert!(err[0].message.contains("Q27"));
    }

    #[test]
    fn array_method_on_fixed_array_is_s014() {
        // Q27 adds only the callback family; formatting and the other
        // checker-owned Array methods remain dynamic-array-only.
        let err = check_one(
            "export function main(): void {\n  const xs: FixedArray<i32, 3> = [1, 2, 3];\n  print(xs.join(\",\"));\n}\n",
        )
        .unwrap_err();
        assert_eq!(err[0].code, RuleCode::S014);
        assert!(err[0].message.contains("dynamic-array-only"));
        assert!(err[0].message.contains("Q22/Q27"));
    }

    #[test]
    fn push_and_pop_on_a_fixed_array_are_not_blamed_on_q22() {
        // P11 review MINOR 3: `push`/`pop` are not Q22 Array methods
        // (they are deliberately outside `ambient::arr_method`), so the
        // FixedArray rejection keeps the standing S100 "no method"
        // diagnostic rather than citing Q22.
        for call in ["xs.push(4)", "xs.pop()"] {
            let err = check_one(&format!(
                "export function main(): void {{\n  const xs: FixedArray<i32, 3> = [1, 2, 3];\n  {call};\n}}\n"
            ))
            .unwrap_err();
            assert_eq!(err[0].code, RuleCode::S100, "{call}: {}", err[0].message);
            assert!(
                !err[0].message.contains("Q22"),
                "{call}: {}",
                err[0].message
            );
            assert!(
                err[0].message.contains("has no method"),
                "{call}: {}",
                err[0].message
            );
        }
    }

    #[test]
    fn reduce_init_takes_its_contextual_type_from_the_callback() {
        // P11 review MINOR 1 (C4): the callback's annotated accumulator
        // type is `init`'s contextual type, so a plain literal init does
        // not default to `i32` and poison `U`.
        for (acc, cb) in [
            ("i64", "(a: i64, v: i32): i64 => a + (v as i64)"),
            ("f64", "(a: f64, v: i32): f64 => a + (v as f64)"),
            ("u32", "(a: u32, v: i32): u32 => a + (v as u32)"),
        ] {
            let src = format!(
                "export function main(): void {{\n  const xs: i32[] = [1, 2, 3];\n  const total: {acc} = xs.reduce({cb}, 0);\n  print(`${{total}}`);\n}}\n"
            );
            let module = check_one(&src)
                .unwrap_or_else(|e| panic!("{acc} accumulator rejected: {e:?}"));
            assert_eq!(module.functions.len(), 1, "{acc}");
        }
    }

    #[test]
    fn reduce_init_takes_its_type_from_a_function_value_callback() {
        // The same rule when the callback is a function value: its
        // declared accumulator type is `init`'s context.
        let src = "function add(acc: i64, v: i32): i64 {\n  return acc + (v as i64);\n}\nexport function main(): void {\n  const xs: i32[] = [1, 2, 3];\n  const total: i64 = xs.reduce(add, 0);\n  print(`${total}`);\n}\n";
        check_one(src).unwrap_or_else(|e| panic!("function-value callback rejected: {e:?}"));
    }

    #[test]
    fn reduce_init_that_does_not_fit_the_accumulator_names_the_init() {
        // A genuine mismatch still errors — against the init, which is
        // the offending argument, not the callback.
        let err = check_one(
            "export function main(): void {\n  const xs: i32[] = [1, 2, 3];\n  const total: i64 = xs.reduce((a: i64, v: i32): i64 => a + (v as i64), \"x\");\n  print(`${total}`);\n}\n",
        )
        .unwrap_err();
        assert!(
            err[0].message.contains("`reduce` init"),
            "{}",
            err[0].message
        );
        assert_eq!(err[0].pos.line, 3);
    }

    #[test]
    fn reduce_without_an_annotated_accumulator_still_types_from_the_init() {
        // An un-annotated arrow does not spell `U`; `init` keeps giving
        // it, as before (contextual typing then flows to the callback).
        let module = check_one(
            "export function main(): void {\n  const xs: i32[] = [1, 2, 3];\n  const joined: string = xs.reduce((acc, v) => acc + `${v}`, \"#\");\n  print(joined);\n}\n",
        )
        .expect("clean check");
        assert_eq!(module.functions.len(), 1);
    }

    #[test]
    fn array_callback_over_value_class_elements_is_s014() {
        // Value-class elements cannot cross the runtime->script element
        // boundary (stdlib.md §9); the checker gates them.
        let err = check_one(
            "@CStruct\nclass V { x: i32; constructor(x: i32) { this.x = x; } }\nexport function main(): void {\n  const xs: V[] = [new V(1)];\n  xs.forEach((v: V): void => {});\n}\n",
        )
        .unwrap_err();
        assert_eq!(err[0].code, RuleCode::S014);
        assert!(err[0].message.contains("Q22"));
    }

    #[test]
    fn array_join_on_date_elements_is_s014() {
        // Date is not interpolatable (Q20); join follows the Q14 rules.
        let err = check_one(
            "export function main(): void {\n  const ds: Date[] = [new Date(0)];\n  print(ds.join(\",\"));\n}\n",
        )
        .unwrap_err();
        assert_eq!(err[0].code, RuleCode::S014);
        assert!(err[0].message.contains("join"));
    }

    #[test]
    fn array_method_read_as_a_value_is_rejected() {
        let err = check_one(
            "export function main(): void {\n  const xs: i32[] = [1];\n  xs.map;\n}\n",
        )
        .unwrap_err();
        assert_eq!(err[0].code, RuleCode::S100);
        assert!(err[0].message.contains("only be called"));
    }

    #[test]
    fn unknown_array_member_keeps_the_surface_diagnostic() {
        let err = check_one(
            "export function main(): void {\n  const xs: i32[] = [1];\n  xs.frobnicate();\n}\n",
        )
        .unwrap_err();
        assert_eq!(err[0].code, RuleCode::S100);
        assert!(err[0].message.contains("Q22"), "{}", err[0].message);
    }

    #[test]
    fn map_set_key_whitelist_rejections_name_q24() {
        for key in [
            "f16",
            "i32[]",
            "FixedArray<i32, 2>",
            "object",
            "(x: i32) => i32",
            "C | null",
            "void",
            "Map<i32, i32>",
            "Set<i32>",
        ] {
            let src = format!(
                "class C {{ x: i32; constructor() {{ this.x = 1; }} }}\n\
                 export function main(): void {{\n\
                   const map: Map<{key}, i32> = new Map<{key}, i32>();\n\
                   print(`${{map.size}}`);\n\
                 }}\n"
            );
            let err = check_one(&src).unwrap_err();
            assert_eq!(err[0].code, RuleCode::S014, "{key}: {err:?}");
            assert!(err[0].message.contains("Q24"), "{key}: {}", err[0].message);
        }
        let err = check_one(
            "@CStruct\nclass V { x: i32; constructor() { this.x = 1; } }\n\
             export function main(): void {\n\
               const set: Set<V> = new Set<V>();\n\
               print(`${set.size}`);\n\
             }\n",
        )
        .unwrap_err();
        assert_eq!(err[0].code, RuleCode::S014);
        assert!(err[0].message.contains("Q24"));
    }

    #[test]
    fn nested_container_value_does_not_inherit_key_resolution_context() {
        let err = check_one(
            "export function main(): void {\n\
               const map: Map<Map<i32, object>, i32> = \
                 new Map<Map<i32, object>, i32>();\n\
               print(`${map.size}`);\n\
             }\n",
        )
        .unwrap_err();
        assert_eq!(err[0].code, RuleCode::S011);
        assert!(err[0].message.contains("boundary-only"));
    }

    #[test]
    fn literal_and_computed_nan_keys_are_accepted() {
        check_one(
            "export function main(): void {\n\
               const map: Map<f64, i32> = new Map<f64, i32>();\n\
               map.set(NaN, 1);\n\
               const set: Set<f64> = new Set<f64>();\n\
               set.add(Number.NaN);\n\
               print(`${map.has(NaN)} ${set.has(NaN)}`);\n\
             }\n",
        )
        .expect("literal NaN keys are accepted");
        check_one(
            "export function main(): void {\n\
               const map: Map<f64, i32> = new Map<f64, i32>();\n\
               const zero: f64 = 0.0;\n\
               const nan: f64 = zero / zero;\n\
               map.set(nan, 1);\n\
               print(`${map.has(nan)} ${map.size}`);\n\
             }\n",
        )
        .expect("computed NaN key is accepted");
    }

    #[test]
    fn number_q25_q26_surface_types_and_rejections() {
        check_one(
            "export function main(): void {\n\
               const parsed: f64 = parseInt(\"ff\", 16);\n\
               const decimal: f64 = parseFloat(\"1.5tail\");\n\
               const parsedStatic: f64 = Number.parseInt(\"ff\", 16);\n\
               const decimalStatic: f64 = Number.parseFloat(\"1.5tail\");\n\
               const f: f32 = 1.25;\n\
               print(`${Number.MAX_SAFE_INTEGER} ${Number.isNaN(Number.NaN)} \
                        ${Number.isFinite(parsed)} ${Number.isInteger(decimal)} \
                        ${Number.isSafeInteger(parsed)}`);\n\
               print(f.toFixed(1));\n\
               print(f.toString(16));\n\
               print(parsed.toExponential());\n\
               print(decimal.toPrecision(2));\n\
               const leading: i32 = Math.clz32(0 as u32);\n\
               const wrapped: i32 = Math.imul(2147483647, 2);\n\
               const rounded: f64 = Math.fround(1.1);\n\
               print(`${leading} ${wrapped} ${rounded} ${parsedStatic} ${decimalStatic}`);\n\
             }\n",
        )
        .expect("accepted Q25/Q26 surface");

        for body in [
            "isNaN(1.0);",
            "isFinite(1.0);",
            "Number(1.0);",
            "parseInt(\"1\");",
            "Number.parseInt(\"1\");",
        ] {
            let err = check_one(&format!(
                "export function main(): void {{\n  {body}\n}}\n"
            ))
            .unwrap_err();
            assert_eq!(err[0].code, RuleCode::S014, "{body}: {err:?}");
            assert!(err[0].message.contains("Q25"), "{body}: {}", err[0].message);
        }

        for body in [
            "(1.0 as f64).toPrecision();",
            "(1.0 as f64).toString();",
        ] {
            let err = check_one(&format!(
                "export function main(): void {{\n  {body}\n}}\n"
            ))
            .unwrap_err();
            assert_eq!(err[0].code, RuleCode::S014, "{body}: {err:?}");
            assert!(err[0].message.contains("Q26"), "{body}: {}", err[0].message);
        }
    }

    #[test]
    fn map_get_is_nullable_only_for_reference_values() {
        let module = check_one(
            "class C { x: i32; constructor() { this.x = 1; } }\n\
             export function main(): void {\n\
               const map: Map<string, C> = new Map<string, C>();\n\
               const value = map.get(\"x\");\n\
               print(`${value === null}`);\n\
             }\n",
        )
        .expect("reference-valued get checks");
        let main = module
            .functions
            .iter()
            .find(|function| function.name == "main")
            .expect("main");
        let hir::Stmt::Let { ty, .. } = &main.body[1] else {
            panic!("expected get binding");
        };
        assert!(matches!(ty, Type::Nullable(inner) if matches!(**inner, Type::Class(_))));

        let err = check_one(
            "export function main(): void {\n\
               const map: Map<i32, i32> = new Map<i32, i32>();\n\
               print(`${map.get(1)}`);\n\
             }\n",
        )
        .unwrap_err();
        assert_eq!(err[0].code, RuleCode::S014);
        assert!(err[0].message.contains("getOr"));
    }

    #[test]
    fn q30_accepts_fused_container_iteration_and_array_literal_spread() {
        check_one(
            "export function main(): void {\n\
               const map: Map<i32, i32> = new Map<i32, i32>();\n\
               const set: Set<i32> = new Set<i32>();\n\
               for (const key of map) { print(`${key}`); }\n\
               for (const value of map.values()) { print(`${value}`); }\n\
               const values: i32[] = [...set];\n\
               print(`${values.length}`);\n\
             }\n",
        )
        .expect("Q30 container traversal and literal spread");

        let err = check_one(
            "export function main(): void {\n\
               const map: Map<i32, i32> = new Map<i32, i32>();\n\
               const keys = map.keys();\n\
             }\n",
        )
        .unwrap_err();
        assert_eq!(err[0].code, RuleCode::S014);
        assert!(err[0].message.contains("direct subject"));
    }

    #[test]
    fn map_group_by_and_set_algebra_are_accepted_by_q27() {
        check_one(
            "export function main(): void {\n\
               const set: Set<i32> = new Set<i32>();\n\
               const grouped: Map<i32, i32[]> = Map.groupBy(\n\
                 [1],\n\
                 (value: i32): i32 => value,\n\
               );\n\
               set.union(set);\n\
               set.intersection(set);\n\
               set.difference(set);\n\
               set.symmetricDifference(set);\n\
               print(`${grouped.size} ${set.isSubsetOf(set)} ${set.isSupersetOf(set)} ${set.isDisjointFrom(set)}`);\n\
             }\n",
        )
        .expect("Q27 stage 4 Map/Set surface checks");
    }

    #[test]
    fn q27_array_index_arity_does_not_reach_map_or_set_callbacks() {
        for (surface, q_rule, source) in [
            (
                "Map.forEach",
                "Q24",
                "export function main(): void {\n\
                   const map: Map<i32, i32> = new Map<i32, i32>();\n\
                   map.forEach((value: i32, key: i32, index: i32): void => {});\n\
                 }\n",
            ),
            (
                "Set.forEach",
                "Q24",
                "export function main(): void {\n\
                   const set: Set<i32> = new Set<i32>();\n\
                   set.forEach((value: i32, index: i32): void => {});\n\
                 }\n",
            ),
            (
                "Map.groupBy",
                "Q27",
                "export function main(): void {\n\
                   Map.groupBy([1], (value: i32, index: i32): i32 => value + index);\n\
                 }\n",
            ),
        ] {
            let err = check_one(source).unwrap_err();
            assert_eq!(err[0].code, RuleCode::S014, "{surface}");
            assert!(
                err[0].message.contains(q_rule),
                "{surface}: {}",
                err[0].message
            );
        }
    }

    #[test]
    fn capturing_lambda_may_not_capture_mutable_locals() {
        let err = check_one(
            "export function main(): void {\n  let n: i32 = 1;\n  const f: (x: i32) => i32 = (x: i32): i32 => x + n;\n  print(`${f(1)}`);\n}\n",
        )
        .unwrap_err();
        assert_eq!(err[0].code, RuleCode::S009);
    }

    #[test]
    fn member_access_on_nullable_without_narrowing_is_s011() {
        let err = check_one(
            "class C { x: i32; constructor() { this.x = 1; } }\nfunction f(c: C | null): i32 {\n  return c.x;\n}\nexport function main(): void {\n  print(`${f(null)}`);\n}\n",
        )
        .unwrap_err();
        assert_eq!(err[0].code, RuleCode::S011);
        assert_eq!(err[0].pos.line, 3);
    }

    #[test]
    fn two_file_program_with_import_checks_clean() {
        let module = check_program(&[
            SourceFile::new(
                "main.ts",
                "import { double } from \"./util\";\nexport function main(): void {\n  print(`${double(2)}`);\n}\n",
            ),
            SourceFile::new(
                "util.ts",
                "export function double(x: i32): i32 {\n  return x * 2;\n}\n",
            ),
        ])
        .expect("clean two-file check");
        assert_eq!(module.functions.len(), 2);
    }

    #[test]
    fn value_class_is_nominal_and_marked_value() {
        let module = check_one(
            "@CStruct\nclass V { x: f32; constructor(x: f32) { this.x = x; } }\nexport function main(): void {\n  const v: V = new V(1.0);\n  print(`${v.x}`);\n}\n",
        )
        .expect("clean");
        assert_eq!(module.classes.len(), 1);
        assert!(module.classes[0].is_value);
        assert_eq!(module.classes[0].fields[0].ty, Type::F32);
    }

    #[test]
    fn fixed_array_length_mismatch_is_rejected() {
        let err = check_one(
            "const xs: FixedArray<f32, 4> = [1.0, 2.0, 3.0];\nexport function main(): void {\n  print(`${xs[0]}`);\n}\n",
        )
        .unwrap_err();
        assert_eq!(err[0].code, RuleCode::S100);
        assert!(err[0].message.contains("FixedArray"));
    }

    #[test]
    fn fixed_array_literal_constructs_with_matching_length() {
        let module = check_one(
            "export function main(): void {\n  const xs: FixedArray<i32, 3> = [1, 2, 3];\n  print(`${xs[2]}`);\n}\n",
        )
        .expect("clean");
        let hir::Stmt::Let { ty, .. } = &module.functions[0].body[0] else {
            panic!("expected let");
        };
        assert_eq!(*ty, Type::FixedArray(Box::new(Type::I32), 3));
    }

    #[test]
    fn const_rebinding_is_rejected_but_field_writes_are_not() {
        // Q17: `const` blocks rebinding only.
        let err = check_one(
            "export function main(): void {\n  const x: i32 = 1;\n  x = 2;\n}\n",
        )
        .unwrap_err();
        assert_eq!(err[0].code, RuleCode::S100);
        assert!(err[0].message.contains("rebind"));

        check_one(
            "@CStruct\nclass V { x: f32; constructor(x: f32) { this.x = x; } }\nexport function main(): void {\n  const v: V = new V(1.0);\n  v.x = 2.0;\n  print(`${v.x}`);\n}\n",
        )
        .expect("field writes through const value bindings are legal");
    }

    #[test]
    fn mixed_width_bitwise_requires_as() {
        // Q18.
        let err = check_one(
            "export function main(): void {\n  const a: u64 = 1;\n  const b: u32 = 2;\n  const c: u64 = a | b;\n  print(`${c}`);\n}\n",
        )
        .unwrap_err();
        assert_eq!(err[0].code, RuleCode::S007);
        assert!(err[0].message.contains("mixed-type bitwise"));
        assert!(!err[0].message.contains("arithmetic"));

        check_one(
            "export function main(): void {\n  const a: u64 = 1;\n  const b: u32 = 2;\n  const c: u64 = a | (b as u64);\n  print(`${c}`);\n}\n",
        )
        .expect("same-width bitwise after `as` is legal");
    }

    #[test]
    fn literal_overshift_is_s008_but_nonliteral_is_accepted() {
        let err = check_one(
            "export function main(): void {\n  const one: u8 = 1;\n  const x: u8 = one << 8;\n  print(`${x}`);\n}\n",
        )
        .unwrap_err();
        assert_eq!(err[0].code, RuleCode::S008);
        assert!(err[0].message.contains("shift amount 8"));
        assert!(err[0].message.contains("`u8` width 8"));

        check_one(
            "export function main(): void {\n  const one: u8 = 1;\n  const amount: u8 = 8;\n  const x: u8 = one << amount;\n  print(`${x}`);\n}\n",
        )
        .expect("nonliteral shift amounts are masked at runtime");
    }

    #[test]
    fn returning_a_local_holding_a_capturing_lambda_is_s009() {
        let err = check_one(
            "function make(): (x: i32) => i32 {\n  const k: i32 = 1;\n  const f: (x: i32) => i32 = (x: i32): i32 => x + k;\n  return f;\n}\nexport function main(): void {\n  print(`${make()(1)}`);\n}\n",
        )
        .unwrap_err();
        assert_eq!(err[0].code, RuleCode::S009);
        assert_eq!(err[0].pos.line, 4);
    }

    #[test]
    fn enum_casts_to_integer_but_not_the_reverse() {
        let module = check_one(
            "enum E { A = 1 }\nexport function main(): void {\n  const e: E = E.A;\n  print(`${e as i32}`);\n}\n",
        )
        .expect("enum to integer cast is legal");
        assert_eq!(module.enums.len(), 1);

        let err = check_one(
            "enum E { A = 1 }\nexport function main(): void {\n  const e: E = 1 as E;\n  print(`${e as i32}`);\n}\n",
        )
        .unwrap_err();
        assert_eq!(err[0].code, RuleCode::S100);
    }

    #[test]
    fn optional_parameters_are_s012() {
        let err = check_one("function f(x?: i32): void {}\nexport function main(): void { f(); }\n")
            .unwrap_err();
        assert_eq!(err[0].code, RuleCode::S012);
    }

    #[test]
    fn narrowing_is_invalidated_by_reassignment() {
        let err = check_one(
            "class C { x: i32; constructor() { this.x = 1; } }\nexport function main(): void {\n  let c: C | null = new C();\n  if (c !== null) {\n    print(`${c.x}`);\n  }\n  c = null;\n  print(`${c.x}`);\n}\n",
        )
        .unwrap_err();
        assert_eq!(err[0].code, RuleCode::S011);
        assert_eq!(err[0].pos.line, 8);
    }

    #[test]
    fn context_free_takes_reference_instances_only() {
        check_one(
            "class C { x: i32; constructor() { this.x = 1; } }\nexport function main(): void {\n  const c: C = new C();\n  Context.free(c);\n}\n",
        )
        .expect("reference instances cross into `object`");

        let err = check_one(
            "@CStruct\nclass V { x: i32; constructor() { this.x = 1; } }\nexport function main(): void {\n  const v: V = new V();\n  Context.free(v);\n}\n",
        )
        .unwrap_err();
        assert_eq!(err[0].code, RuleCode::S100);
    }

    #[test]
    fn context_namespace_is_neither_a_value_nor_a_class() {
        let value_err = check_one(
            "export function main(): void {\n  const value = Context;\n}\n",
        )
        .unwrap_err();
        assert_eq!(value_err[0].code, RuleCode::S014);
        assert_eq!(
            value_err[0].message,
            "`Context` is an ambient namespace, not a value; use \
             `Context.collect()` or `Context.free(value)` (Q6/Q7)"
        );

        let construct_err =
            check_one("export function main(): void {\n  const value = new Context();\n}\n")
                .unwrap_err();
        assert_eq!(construct_err[0].code, RuleCode::S100);
        assert_eq!(construct_err[0].message, "unknown class `Context`");
    }

    // ----- P1 phase-review regression tests -----

    #[test]
    fn m1_enum_implicit_value_overflow_is_s008_not_a_panic() {
        let err = check_one(
            "enum E { A = 9223372036854775807, B }\nexport function main(): void {\n  print(`${E.A as i32}`);\n}\n",
        )
        .unwrap_err();
        assert_eq!(err[0].code, RuleCode::S008);
        assert!(err[0].message.contains("B"), "message names the member: {}", err[0].message);
    }

    #[test]
    fn m2a_push_of_a_capturing_lambda_is_s009() {
        let err = check_one(
            "export function main(): void {\n  const k: i32 = 1;\n  const f: (x: i32) => i32 = (x: i32): i32 => x + k;\n  const xs: ((x: i32) => i32)[] = [];\n  xs.push(f);\n}\n",
        )
        .unwrap_err();
        assert_eq!(err[0].code, RuleCode::S009);
        assert_eq!(err[0].pos.line, 5);
    }

    #[test]
    fn m2b_array_literals_reject_capturing_lambdas_in_every_context() {
        // Inferred element type.
        let err = check_one(
            "export function main(): void {\n  const k: i32 = 1;\n  const fs = [(x: i32): i32 => x + k];\n  print(`${fs[0](1)}`);\n}\n",
        )
        .unwrap_err();
        assert_eq!(err[0].code, RuleCode::S009);

        // FixedArray context.
        let err = check_one(
            "export function main(): void {\n  const k: i32 = 1;\n  const fs: FixedArray<(x: i32) => i32, 1> = [(x: i32): i32 => x + k];\n  print(`${fs[0](1)}`);\n}\n",
        )
        .unwrap_err();
        assert_eq!(err[0].code, RuleCode::S009);
    }

    #[test]
    fn m2b_returning_a_local_bound_to_a_capturing_array_literal_is_s009() {
        let diags = check_one(
            "function make(): ((x: i32) => i32)[] {\n  const k: i32 = 1;\n  const fs = [(x: i32): i32 => x + k];\n  return fs;\n}\nexport function main(): void {\n  print(`${make()[0](1)}`);\n}\n",
        )
        .unwrap_err();
        assert!(
            diags.iter().any(|d| d.code == RuleCode::S009),
            "expected an S009 among: {:?}",
            diags.iter().map(|d| d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn m2c_returning_a_conditional_over_capturing_lambdas_is_s009() {
        let err = check_one(
            "function pick(flag: boolean): (x: i32) => i32 {\n  const a: i32 = 1;\n  const f: (x: i32) => i32 = (x: i32): i32 => x + a;\n  const g: (x: i32) => i32 = (x: i32): i32 => x - a;\n  return flag ? f : g;\n}\nexport function main(): void {\n  print(`${pick(true)(1)}`);\n}\n",
        )
        .unwrap_err();
        assert_eq!(err[0].code, RuleCode::S009);
        assert_eq!(err[0].pos.line, 5);
    }

    #[test]
    fn m3_missing_return_path_is_s100_at_the_function() {
        let err = check_one(
            "function f(flag: boolean): i32 {\n  if (flag) {\n    return 1;\n  }\n}\nexport function main(): void {\n  print(`${f(true)}`);\n}\n",
        )
        .unwrap_err();
        assert_eq!(err[0].code, RuleCode::S100);
        assert!(err[0].message.contains("return"));
        assert_eq!(err[0].pos.line, 1);
    }

    #[test]
    fn m3_all_return_shapes_check_clean() {
        // if/else with both arms returning.
        check_one(
            "function f(flag: boolean): i32 {\n  if (flag) {\n    return 1;\n  } else {\n    return 2;\n  }\n}\nexport function main(): void {\n  print(`${f(true)}`);\n}\n",
        )
        .expect("if/else return");
        // Infinite loop with no break never falls through.
        check_one(
            "function f(): i32 {\n  while (true) {\n    return 1;\n  }\n}\nexport function main(): void {\n  print(`${f()}`);\n}\n",
        )
        .expect("while(true) return");
    }

    #[test]
    fn m3_while_true_with_break_does_not_count_as_returning() {
        let err = check_one(
            "function f(flag: boolean): i32 {\n  while (true) {\n    if (flag) {\n      return 1;\n    }\n    break;\n  }\n}\nexport function main(): void {\n  print(`${f(true)}`);\n}\n",
        )
        .unwrap_err();
        assert_eq!(err[0].code, RuleCode::S100);
        assert!(err[0].message.contains("return"));
    }

    #[test]
    fn m3_lambda_block_bodies_need_all_paths_to_return() {
        let err = check_one(
            "export function main(): void {\n  const f: (x: i32) => i32 = (x: i32): i32 => {\n    if (x > 0) {\n      return x;\n    }\n  };\n  print(`${f(1)}`);\n}\n",
        )
        .unwrap_err();
        assert_eq!(err[0].code, RuleCode::S100);
        assert!(err[0].message.contains("return"));
    }

    #[test]
    fn minor1_update_operators_respect_const_bindings() {
        let err = check_one(
            "export function main(): void {\n  const x: i32 = 1;\n  x++;\n  print(`${x}`);\n}\n",
        )
        .unwrap_err();
        assert_eq!(err[0].code, RuleCode::S100);
        assert!(err[0].message.contains("rebind"));
    }

    #[test]
    fn minor2_user_written_object_annotations_are_s011() {
        let err = check_one("let o: object | null = null;\n").unwrap_err();
        assert_eq!(err[0].code, RuleCode::S011);

        let err = check_one("function f(o: object): void {}\nexport function main(): void {}\n")
            .unwrap_err();
        assert_eq!(err[0].code, RuleCode::S011);

        // The ambient `Context.free(value: object)` stays callable.
        check_one(
            "class C { x: i32; constructor() { this.x = 1; } }\nexport function main(): void {\n  const c: C = new C();\n  Context.free(c);\n}\n",
        )
        .expect("ambient object parameter unaffected");
    }

    #[test]
    fn minor3_cross_file_duplicate_class_names_are_s100() {
        let err = check_program(&[
            SourceFile::new("a.ts", "export class C { x: i32 = 1; }\nexport function main(): void {}\n"),
            SourceFile::new("b.ts", "export class C { x: i32 = 1; }\n"),
        ])
        .unwrap_err();
        assert!(
            err.iter()
                .any(|d| d.code == RuleCode::S100 && d.message.contains("duplicate class")),
            "expected a duplicate-class diagnostic, got: {:?}",
            err.iter().map(|d| d.message.clone()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn minor4_fixed_array_length_beyond_u32_is_s008() {
        let err = check_one(
            "function f(xs: FixedArray<i32, 4294967296>): void {}\nexport function main(): void {}\n",
        )
        .unwrap_err();
        assert_eq!(err[0].code, RuleCode::S008);
        assert!(err[0].message.contains("FixedArray length"));
    }

    // ----- P9.2 Date (stdlib.md §3, Q20) -----

    #[test]
    fn date_round_trip_program_checks_clean_with_nominal_types() {
        let module = check_one(
            "export function main(): void {\n  const d: Date = new Date(Date.UTC(2020, 0, 2));\n  const t: i64 = d.getTime();\n  print(`${t},${d.getUTCFullYear()}`);\n  print(d.toISOString());\n}\n",
        )
        .expect("clean");
        let hir::Stmt::Let { ty, .. } = &module.functions[0].body[0] else {
            panic!("expected let");
        };
        assert_eq!(*ty, Type::Date);
        // getTime folds to the receiver retyped i64 — no call survives.
        let hir::Stmt::Let { ty, init, .. } = &module.functions[0].body[1] else {
            panic!("expected let");
        };
        assert_eq!(*ty, Type::I64);
        assert!(
            matches!(init.kind, hir::ExprKind::Local(_)),
            "getTime must fold to the receiver, got {:?}",
            init.kind
        );
    }

    #[test]
    fn date_utc_normalizes_missing_trailing_arguments_to_defaults() {
        let module = check_one(
            "export function main(): void {\n  const t: i64 = Date.UTC(2020, 0);\n  print(`${t}`);\n}\n",
        )
        .expect("clean");
        let hir::Stmt::Let { init, .. } = &module.functions[0].body[0] else {
            panic!("expected let");
        };
        let hir::ExprKind::Call { callee, args } = &init.kind else {
            panic!("expected a call");
        };
        assert_eq!(*callee, hir::Callee::Date(hir::DateFn::Utc));
        assert_eq!(args.len(), 7, "the runtime signature is always 7-argument");
        // day defaults to 1, the time components to 0.
        let values: Vec<i64> = args
            .iter()
            .skip(2)
            .map(|a| match a.kind {
                hir::ExprKind::Int(v) => v,
                ref other => panic!("expected an int default, got {other:?}"),
            })
            .collect();
        assert_eq!(values, vec![1, 0, 0, 0, 0]);
    }

    #[test]
    fn date_is_not_interchangeable_with_i64() {
        // i64 → Date needs `new Date(ms)`.
        let err = check_one(
            "export function main(): void {\n  const d: Date = 0;\n  print(d.toISOString());\n}\n",
        )
        .unwrap_err();
        assert_eq!(err[0].code, RuleCode::S100);
        // Date → i64 needs `getTime()`.
        let err = check_one(
            "export function main(): void {\n  const d: Date = new Date(0);\n  const t: i64 = d;\n  print(`${t}`);\n}\n",
        )
        .unwrap_err();
        assert_eq!(err[0].code, RuleCode::S100);
    }

    #[test]
    fn date_equality_is_s014_with_the_gettime_hint() {
        let err = check_one(
            "export function main(): void {\n  const a: Date = new Date(0);\n  const b: Date = new Date(0);\n  if (a === b) {\n    print(\"same\");\n  }\n}\n",
        )
        .unwrap_err();
        assert_eq!(err[0].code, RuleCode::S014);
        assert!(err[0].message.contains("getTime"), "message: {}", err[0].message);
        // Relational comparison is the same rejection.
        let err = check_one(
            "export function main(): void {\n  const a: Date = new Date(0);\n  const b: Date = new Date(1);\n  if (a < b) {\n    print(\"before\");\n  }\n}\n",
        )
        .unwrap_err();
        assert_eq!(err[0].code, RuleCode::S014);
    }

    #[test]
    fn date_nullable_union_is_s011() {
        let err = check_one("let d: Date | null = null;\n").unwrap_err();
        assert_eq!(err[0].code, RuleCode::S011);
    }

    #[test]
    fn date_as_a_value_and_static_member_reads_are_s014() {
        let err = check_one("export function main(): void {\n  const d = Date;\n}\n").unwrap_err();
        assert_eq!(err[0].code, RuleCode::S014);
        let err = check_one(
            "export function main(): void {\n  const f = Date.now;\n}\n",
        )
        .unwrap_err();
        assert_eq!(err[0].code, RuleCode::S014);
        let err = check_one(
            "export function main(): void {\n  const t: i64 = Date.parse(\"2020\");\n}\n",
        )
        .unwrap_err();
        assert_eq!(err[0].code, RuleCode::S014);
        assert!(err[0].message.contains("parse"));
    }

    #[test]
    fn a_user_class_named_date_shadows_the_builtin() {
        // Same rule as Math: program declarations win. The user class's
        // own constructor and methods apply, including local-sounding
        // names the ambient subset would reject.
        let module = check_one(
            "class Date { ms: i32;\n  constructor(ms: i32) { this.ms = ms; }\n  getFullYear(): i32 { return 1970; }\n}\nexport function main(): void {\n  const d: Date = new Date(3);\n  print(`${d.getFullYear()},${d.ms}`);\n}\n",
        )
        .expect("shadowing class checks clean");
        assert_eq!(module.classes.len(), 1);
        assert_eq!(module.classes[0].name, "Date");
    }

    #[test]
    fn a_function_local_const_named_date_shadows_the_builtin_ctor() {
        // Stock tsc rejects this (TS2351: the local i32 is not
        // constructable); the ambient constructor must not apply once a
        // function-local binding shadows the name.
        let err = check_one(
            "export function main(): void {\n  const Date: i32 = 5;\n  const d = new Date(1);\n  print(`${Date}`);\n}\n",
        )
        .unwrap_err();
        assert_eq!(err[0].code, RuleCode::S100);
        assert!(
            err[0].message.contains("unknown class"),
            "message: {}",
            err[0].message
        );
        assert_eq!(err[0].pos.line, 3);
    }

    #[test]
    fn a_function_local_let_named_date_shadows_the_builtin_ctor() {
        let err = check_one(
            "export function main(): void {\n  let Date: i32 = 5;\n  Date += 1;\n  const d = new Date(1);\n  print(`${Date}`);\n}\n",
        )
        .unwrap_err();
        assert_eq!(err[0].code, RuleCode::S100);
        assert!(
            err[0].message.contains("unknown class"),
            "message: {}",
            err[0].message
        );
        assert_eq!(err[0].pos.line, 4);
    }

    #[test]
    fn a_parameter_named_date_shadows_the_builtin_ctor() {
        let err = check_one(
            "function f(Date: i32): i32 {\n  const d = new Date(1);\n  return Date;\n}\nexport function main(): void {\n  print(`${f(1)}`);\n}\n",
        )
        .unwrap_err();
        assert_eq!(err[0].code, RuleCode::S100);
        assert!(
            err[0].message.contains("unknown class"),
            "message: {}",
            err[0].message
        );
        assert_eq!(err[0].pos.line, 2);
    }

    #[test]
    fn same_shaped_classes_do_not_substitute() {
        let err = check_one(
            "class A { x: i32 = 1; }\nclass B { x: i32 = 1; }\nfunction f(a: A): i32 { return a.x; }\nexport function main(): void {\n  const b: B = new B();\n  print(`${f(b)}`);\n}\n",
        )
        .unwrap_err();
        assert_eq!(err[0].code, RuleCode::S005);
    }
}
