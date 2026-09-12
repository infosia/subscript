//! §99: byte-data representation, ownership, and three execution witnesses.
#[path = "support/pool.rs"]
mod pool;

use std::time::Instant;

use subscript_codegen::{emit_c, interpreter::interpret, lir::lower_module, run_c_aot, run_jit};
use subscript_compiler::{check_program, SourceFile};

fn payload(bytes: usize, mixed: bool) -> (String, Vec<u8>) {
    if !mixed {
        return ("a".repeat(bytes), vec![b'a'; bytes]);
    }
    // The source escapes and the expected decoded bytes have separate forms.
    let unit = "é\0\n\\\"A".as_bytes();
    let count = bytes / unit.len();
    let tail = bytes % unit.len();
    let source = format!("{}{}", r#"é\0\n\\\"A"#.repeat(count), "B".repeat(tail));
    let mut decoded = unit.repeat(count);
    decoded.extend(std::iter::repeat_n(b'B', tail));
    (source, decoded)
}

fn source(expression: &str) -> String {
    format!(
        r#"function text(): string {{
  const suffix: string = "";
  return {expression};
}}
export function main(): void {{
  for (let call: i32 = 0; call < 2; call = call + 1) {{
    const s: string = text();
    let sum: i32 = 0;
    for (let i: i32 = 0; i < s.length; i = i + 1) {{
      sum = (sum + s.charCodeAt(i)) & 65535;
    }}
    print(`${{s.length}} ${{sum}}`);
    Context.collect();
  }}
}}
"#
    )
}

#[derive(Clone, Copy, Debug)]
enum StringForm {
    Literal,
    Template,
    Alias,
}

/// Runs one case in all three witnesses and returns the line the caller
/// prints. The caller prints the lines in case order.
fn check_case(bytes: usize, mixed: bool, form: StringForm) -> String {
    let start = Instant::now();
    let (encoded, decoded) = payload(bytes, mixed);
    assert_eq!(decoded.len(), bytes);
    let program = match form {
        StringForm::Literal => source(&format!("\"{encoded}\"")),
        StringForm::Template => source(&format!("`{encoded}${{suffix}}`")),
        StringForm::Alias => format!(
            "type Word = \"{encoded}\" | \"b\";\nfunction word(): Word {{ return \"{encoded}\"; }}\n{}",
            source("`${word()}`")
        ),
    };
    let files = [SourceFile::new("long-string.ts", program)];
    let hir = check_program(&files).expect("long string checks");
    let c = emit_c(&hir).expect("long string emits").source;
    let marker = "static const unsigned char sub_long_string_";
    if bytes <= 65_000 {
        assert!(!c.contains(marker), "short data must keep the literal form");
        assert!(c.contains("(const unsigned char*)\""));
    } else {
        assert_eq!(c.matches(marker).count(), 1, "one declaration per constant");
        let declaration = c.find(marker).unwrap();
        let data_start = c[declaration..].find('{').unwrap() + declaration;
        let data_end = c[data_start..].find("};").unwrap() + data_start;
        // The declaration is at file scope, before all uses.
        assert_eq!(
            c[..declaration].matches('{').count(),
            c[..declaration].matches('}').count()
        );
        if matches!(form, StringForm::Alias) {
            let table = c
                .find("static const SubStringAliasMember sub_alias_0")
                .unwrap();
            assert!(data_end < table, "data must precede its table initializer");
            assert!(c[table..].contains(&format!("{{ sub_long_string_0, {bytes}ull }}")));
            assert!(
                c.lines().any(|line| {
                    line.contains("sub_alias_0[") && line.contains(".data") && line.contains(".len")
                }),
                "alias formatting must read the table through its discriminant"
            );
        } else {
            assert!(c[data_end..].contains("ctx, sub_long_string_0,"));
            assert!(c.contains(&format!("sub_long_string_0, {bytes}ull,")));
        }
        let actual: Vec<u8> = c[data_start + 1..data_end]
            .split(',')
            .map(str::trim)
            .filter(|byte| !byte.is_empty())
            .map(|byte| u8::from_str_radix(byte.strip_prefix("0x").unwrap(), 16).unwrap())
            .collect();
        // Compare every byte, including both sides of each generated line break.
        assert_eq!(actual, decoded);
    }
    let checksum = decoded
        .iter()
        .fold(0u32, |sum, byte| (sum + u32::from(*byte)) & 65535);
    let expected = format!("{bytes} {checksum}\n").repeat(2).into_bytes();
    let dev = run_jit(&files).expect("dev runs");
    let ship = run_c_aot(&files).expect("ship runs");
    let lir = lower_module(&hir).expect("long string lowers");
    let interpreted = interpret(&lir).expect("interpreter runs");
    for (tier, output) in [("dev", dev), ("ship", ship), ("interpreter", interpreted)] {
        assert_eq!(
            output, expected,
            "{tier}: bytes={bytes}, mixed={mixed}, form={form:?}"
        );
    }
    format!("s99 bytes={bytes} mixed={mixed} form={form:?} C={} elapsed={:.3}s output={bytes} {checksum} (twice)", c.len(), start.elapsed().as_secs_f64())
}

#[test]
fn generated_lengths_run_in_all_three_witnesses() {
    let mut cases = Vec::new();
    for bytes in [64_999, 65_000, 65_001, 65_535, 65_536, 1_048_576] {
        cases.push((bytes, false, StringForm::Literal));
        cases.push((bytes, true, StringForm::Literal));
    }
    cases.push((65_001, true, StringForm::Template));
    // One case per work item; this thread prints the lines in case order.
    let lines = pool::map_in_order(&cases, |(bytes, mixed, form)| {
        check_case(*bytes, *mixed, *form)
    });
    for line in lines {
        eprintln!("{line}");
    }
}

#[test]
fn alias_members_cross_the_representation_boundary_in_all_three_witnesses() {
    eprintln!("{}", check_case(65_000, true, StringForm::Alias));
    eprintln!("{}", check_case(65_001, true, StringForm::Alias));
}

#[test]
fn standing_witness_has_the_declared_length_and_checksum() {
    let source = include_str!("../../corpus/accept/a204-static-long-string.ts");
    let literal = source
        .split("type Word = \"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap();
    assert_eq!(literal.len(), 65_001);
    assert_eq!(literal.bytes().map(u32::from).sum::<u32>() & 65535, 13_641);
    let files = [SourceFile::new("a204-static-long-string.ts", source)];
    let measured = run_jit(&files).expect("standing witness runs");
    assert_eq!(
        measured,
        include_bytes!("../../corpus/accept/a204-static-long-string.expected")
    );
}

#[test]
fn long_data_keeps_context_interning_and_allocation_traps() {
    use subscript_codegen::{run_c_aot_with_alloc_failure, run_jit_with_alloc_failure};

    let encoded = "a".repeat(65_001);
    for form in [StringForm::Literal, StringForm::Template, StringForm::Alias] {
        let (prefix, expression) = match form {
            StringForm::Literal => (String::new(), format!("\"{encoded}\"")),
            StringForm::Template => (String::new(), format!("`{encoded}`")),
            StringForm::Alias => (
                format!("type Word = \"{encoded}\" | \"b\";\nfunction word(): Word {{ return \"{encoded}\"; }}\n"),
                "`${word()}`".to_owned(),
            ),
        };
        let program = format!("{prefix}function text(): string {{\n  return {expression};\n}}\nexport function main(): void {{\n  if (text().length !== 65001) {{ unreachable(); }}\n  Context.collect();\n  if (text().length !== 65001) {{ unreachable(); }}\n}}\n");
        let files = [SourceFile::new("intern.ts", program)];
        for allocation in [1, 2] {
            for (tier, result) in [
                ("dev", run_jit_with_alloc_failure(&files, allocation)),
                ("ship", run_c_aot_with_alloc_failure(&files, allocation)),
            ] {
                if allocation == 1 {
                    let subscript_codegen::RunError::Trap(report) = result.unwrap_err() else {
                        panic!("{tier}: the first evaluation must report an allocation trap");
                    };
                    let (line, col) = match form {
                        StringForm::Alias => (4, 13),
                        StringForm::Literal | StringForm::Template => (2, 10),
                    };
                    assert_eq!(report.rule, subscript_runtime::TrapKind::AllocationFailure);
                    assert_eq!(report.message, "injected allocation failure");
                    assert_eq!(
                        report.pos,
                        subscript_compiler::Pos::new("intern.ts", line, col)
                    );
                    assert!(report.stdout.is_empty());
                } else {
                    // No second allocation: the same literal survives collection as an intern root.
                    assert!(result
                        .expect("repeated evaluation must reuse the intern")
                        .is_empty());
                }
            }
        }
    }
}
