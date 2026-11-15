//! §299 acceptance 3: literal operands keep integer equality, mixed arrays
//! widen to string, and an array of alias values keeps its element type.
//! Measured cost: 78 ms in debug for the two aliases; checker/C emission,
//! including the string controls and rejection checks; compilation excluded.

use std::time::Instant;
use subscript_codegen::emit_c;
use subscript_compiler::{check_program, hir, RuleCode, SourceFile, StringAliasId, Type};

#[test]
fn literal_comparisons_and_array_types_with_a_string_comparison_control() {
    let started = Instant::now();
    for alias in ["\"a\" | \"bb\"", "CEnum<{ a: -3; bb: 7; }>"] {
        let declaration = format!("type Mode = {alias};");
        let source = format!("{declaration} function same(m: Mode, m2: Mode): boolean {{ const c = \"a\"; const only = [m, m2]; const mixed = [m, \"a\"]; const reverse = [\"a\", m]; return m === \"a\" || m === m2 || m === c; }} export function main(): void {{ print(`${{same(\"bb\", \"bb\")}}`); }}");
        let module = check_program(&[SourceFile::new("alias.ts", source)]).unwrap();
        let function = module.functions.iter().find(|f| f.name == "same").unwrap();
        for (name, element) in [
            ("only", Type::StringAlias(StringAliasId(0))),
            ("mixed", Type::Str),
            ("reverse", Type::Str),
        ] {
            let (ty, array) = function
                .body
                .iter()
                .find_map(|s| match s {
                    hir::Stmt::Let {
                        name: binding,
                        ty,
                        init,
                        ..
                    } if binding == name => Some((ty, init)),
                    _ => None,
                })
                .unwrap();
            assert_eq!(*ty, Type::array(element.clone()), "{name}");
            assert_eq!(array.ty, *ty);
            let hir::ExprKind::ArrayLit(elements) = &array.kind else {
                panic!("array literal")
            };
            assert!(elements.iter().all(|e| e.ty == element), "{name}");
        }
        let c = emit_c(&module).unwrap().source;
        assert!(!c.contains("subscript_rt_str_eq("));

        for (body, message) in [
            (
                "const c = \"zzz\"; return m === c;",
                "operator `===` not defined for `Mode` and `string`",
            ),
            (
                "return m === `zzz`;",
                "operator `===` not defined for `Mode` and `string`",
            ),
        ] {
            let source = format!("{declaration} function same(m: Mode): boolean {{ {body} }}");
            let errors = check_program(&[SourceFile::new("nonmember.ts", source)]).unwrap_err();
            assert_eq!(errors.len(), 1);
            assert_eq!(errors[0].code, RuleCode::S100);
            assert_eq!(errors[0].message, message);
        }

        let source = format!("{declaration} function same(m: Mode, s: string): boolean {{ return m === s; }} export function main(): void {{ print(`${{same(\"bb\", \"bb\")}}`); }}");
        let module = check_program(&[SourceFile::new("string.ts", source)]).unwrap();
        let c = emit_c(&module).unwrap().source;
        assert!(c.contains("subscript_rt_str_eq("));
    }
    eprintln!("§299 acceptance 3: {:?}", started.elapsed());
}

// Measured cost: 60 ms in debug for two checker/LIR/interpreter programs;
// no native compilation or JIT process.
#[test]
fn literal_switches_preserve_fallthrough_and_the_no_match_exit() {
    let started = Instant::now();
    for alias in ["\"a\" | \"bb\"", "CEnum<{ a: -3; bb: 7; }>"] {
        let source = format!(
            r#"
type Mode = {alias};
const moduleMember = "a";
function show(m: Mode, s: string): void {{
    const c = "a";
    switch (c) {{ case m: print("alias"); break; }}
    print("after");
    switch (c) {{
        case "a": print("literal");
        case m: print("fall"); break;
        default: print("unreachable");
    }}
    switch (c) {{
        case s: print("string"); break;
        case m: print("alias2"); break;
        default: print("miss");
    }}
    switch (m) {{ case moduleMember: print("member"); break; default: print("other"); }}
}}
export function main(): void {{ show("a", "a"); show("bb", "bb"); }}
"#
        );
        let module = check_program(&[SourceFile::new("switch.ts", source)]).unwrap();
        let lir = subscript_codegen::lir::lower_module(&module).unwrap();
        assert_eq!(
            subscript_codegen::interpreter::interpret(&lir).unwrap(),
            b"alias\nafter\nliteral\nfall\nstring\nmember\nafter\nliteral\nfall\nmiss\nother\n"
        );
        emit_c(&module).unwrap();
    }
    eprintln!("§299 literal switches: {:?}", started.elapsed());
}
