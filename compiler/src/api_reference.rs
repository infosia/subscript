//! Generated API-reference projection and executable divergence
//! witnesses.
//!
//! The accepted and rejected rows come from [`crate::ambient`], the
//! same tables the checker consults. This module does not read a spec
//! file.

use std::fmt::Write as _;

use crate::ambient;

/// One possible observable result of a divergence witness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum WitnessOutcome {
    /// The program completes and writes these exact UTF-8 bytes.
    Value(&'static str),
    /// The program produces no value because execution traps or throws.
    Trap,
}

impl WitnessOutcome {
    fn markdown(self) -> &'static str {
        match self {
            WitnessOutcome::Value(value) => value,
            WitnessOutcome::Trap => "Trap",
        }
    }
}

/// An executable subscript/Node pair that demonstrates one recorded
/// result divergence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct DivergenceWitness {
    /// Stable witness identifier used in diagnostics.
    pub id: &'static str,
    /// Accepted surface whose result differs.
    pub surface: &'static str,
    /// Collision-register entry recording the difference.
    pub q_rule: &'static str,
    /// Concise description of the result difference.
    pub summary: &'static str,
    /// Complete subscript source executed by the test.
    pub subscript: &'static str,
    /// Complete JavaScript source executed by Node.
    pub javascript: &'static str,
    /// Required subscript execution outcome.
    pub subscript_outcome: WitnessOutcome,
    /// Required Node execution outcome.
    pub javascript_outcome: WitnessOutcome,
}

/// Date on which the listed checker-owned built-in surfaces were last
/// adversarially compared with Node.
pub const DIVERGENCE_SWEEP_DATE: &str = "2026-07-26";

/// Every recorded divergence and its executable witness.
///
/// Tests execute both complete fragments. Outcome equality is defined
/// over `Value(bytes)` and `Trap`: values agree only byte-for-byte,
/// two traps agree, and a trap never agrees with a value.
pub const DIVERGENCE_WITNESSES: &[DivergenceWitness] = &[
    DivergenceWitness {
        id: "q14-negative-zero",
        surface: "template interpolation, `T[].join`, `f32/f64.toString(10)`",
        q_rule: "Q14 (the method is accepted by Q26)",
        summary: "The shared decimal formatter preserves negative zero; JS renders it as zero.",
        subscript: r#"export function main(): void {
  const single: f32 = -0.0;
  const values: f64[] = [-0.0];
  print(`${-0.0}|${values.join(",")}|${single.toString(10)}|${(-0.0).toString(10)}`);
}
"#,
        javascript: r#"console.log(`${-0}|${[-0].join(",")}|${Math.fround(-0).toString(10)}|${(-0).toString(10)}`);
"#,
        subscript_outcome: WitnessOutcome::Value("-0|-0|-0|-0\n"),
        javascript_outcome: WitnessOutcome::Value("0|0|0|0\n"),
    },
    DivergenceWitness {
        id: "q5-string-length",
        surface: "`string.length`",
        q_rule: "Q5",
        summary: "String length counts UTF-8 bytes rather than UTF-16 code units.",
        subscript: r#"export function main(): void {
  print(`${"é".length}`);
}
"#,
        javascript: r#"console.log(`${"é".length}`);
"#,
        subscript_outcome: WitnessOutcome::Value("2\n"),
        javascript_outcome: WitnessOutcome::Value("1\n"),
    },
    DivergenceWitness {
        id: "q5-string-slice",
        surface: "`string.slice`, `string.substring`, and `string.substr`",
        q_rule: "Q5",
        summary: "String slicing offsets are UTF-8 byte offsets rather than UTF-16 code-unit offsets.",
        subscript: r#"export function main(): void {
  const value: string = "éx";
  print(`${value.slice(0, 2)}|${value.substring(0, 2)}|${value.substr(0, 2)}`);
}
"#,
        javascript: r#"const value = "éx";
console.log(`${value.slice(0, 2)}|${value.substring(0, 2)}|${value.substr(0, 2)}`);
"#,
        subscript_outcome: WitnessOutcome::Value("é|é|é\n"),
        javascript_outcome: WitnessOutcome::Value("éx|éx|éx\n"),
    },
    DivergenceWitness {
        id: "q5-string-search-indices",
        surface: "`string` search and positioned prefix/suffix methods",
        q_rule: "Q5 / Q21",
        summary: "Search positions and returned indices use UTF-8 byte offsets.",
        subscript: r#"export function main(): void {
  const value: string = "éx";
  print(`${value.indexOf("x")}|${value.lastIndexOf("x")}|${value.includes("x", 2)}|${value.startsWith("x", 2)}|${value.endsWith("é", 2)}`);
}
"#,
        javascript: r#"const value = "éx";
console.log(`${value.indexOf("x")}|${value.lastIndexOf("x")}|${value.includes("x", 2)}|${value.startsWith("x", 2)}|${value.endsWith("é", 2)}`);
"#,
        subscript_outcome: WitnessOutcome::Value("2|2|true|true|true\n"),
        javascript_outcome: WitnessOutcome::Value("1|1|false|false|false\n"),
    },
    DivergenceWitness {
        id: "q5-char-code-byte",
        surface: "`string.charCodeAt`, `string.charAt`, and `string.codePointAt`",
        q_rule: "Q5 / Q21",
        summary: "`charCodeAt` returns one UTF-8 byte, while all three methods use UTF-8 byte indices.",
        subscript: r#"export function main(): void {
  const value: string = "éx";
  print(`${value.charCodeAt(0)}|${value.charAt(2)}|${value.codePointAt(2)}`);
}
"#,
        javascript: r#"const value = "éx";
console.log(`${value.charCodeAt(0)}|${value.charAt(2)}|${String(value.codePointAt(2))}`);
"#,
        subscript_outcome: WitnessOutcome::Value("195|x|120\n"),
        javascript_outcome: WitnessOutcome::Value("233||undefined\n"),
    },
    DivergenceWitness {
        id: "q5-pad-start-byte-length",
        surface: "`string.padStart`",
        q_rule: "Q5 / Q21",
        summary: "The target length is a UTF-8 byte length.",
        subscript: r#"export function main(): void {
  print("é".padStart(3, "x"));
}
"#,
        javascript: r#"console.log("é".padStart(3, "x"));
"#,
        subscript_outcome: WitnessOutcome::Value("xé\n"),
        javascript_outcome: WitnessOutcome::Value("xxé\n"),
    },
    DivergenceWitness {
        id: "q5-pad-end-byte-length",
        surface: "`string.padEnd`",
        q_rule: "Q5 / Q21",
        summary: "The target length is a UTF-8 byte length.",
        subscript: r#"export function main(): void {
  print("é".padEnd(3, "x"));
}
"#,
        javascript: r#"console.log("é".padEnd(3, "x"));
"#,
        subscript_outcome: WitnessOutcome::Value("éx\n"),
        javascript_outcome: WitnessOutcome::Value("éxx\n"),
    },
    DivergenceWitness {
        id: "q21-pad-start-empty",
        surface: "`string.padStart` with an empty pad",
        q_rule: "Q21",
        summary: "subscript traps while JS returns the unchanged string.",
        subscript: r#"export function main(): void {
  print("x".padStart(3, ""));
}
"#,
        javascript: r#"console.log("x".padStart(3, ""));
"#,
        subscript_outcome: WitnessOutcome::Trap,
        javascript_outcome: WitnessOutcome::Value("x\n"),
    },
    DivergenceWitness {
        id: "q21-pad-end-empty",
        surface: "`string.padEnd` with an empty pad",
        q_rule: "Q21",
        summary: "subscript traps while JS returns the unchanged string.",
        subscript: r#"export function main(): void {
  print("x".padEnd(3, ""));
}
"#,
        javascript: r#"console.log("x".padEnd(3, ""));
"#,
        subscript_outcome: WitnessOutcome::Trap,
        javascript_outcome: WitnessOutcome::Value("x\n"),
    },
    DivergenceWitness {
        id: "q21-split-empty",
        surface: "`string.split(\"\")`",
        q_rule: "Q21",
        summary: "subscript traps while JS splits into UTF-16 units.",
        subscript: r#"export function main(): void {
  const pieces: string[] = "ab".split("");
  print(`${pieces.length}`);
}
"#,
        javascript: r#"console.log(`${"ab".split("").length}`);
"#,
        subscript_outcome: WitnessOutcome::Trap,
        javascript_outcome: WitnessOutcome::Value("2\n"),
    },
    DivergenceWitness {
        id: "q21-char-code-oob",
        surface: "`string.charCodeAt` out of range",
        q_rule: "Q21",
        summary: "subscript traps while JS returns NaN.",
        subscript: r#"export function main(): void {
  print(`${"a".charCodeAt(1)}`);
}
"#,
        javascript: r#"console.log(`${"a".charCodeAt(1)}`);
"#,
        subscript_outcome: WitnessOutcome::Trap,
        javascript_outcome: WitnessOutcome::Value("NaN\n"),
    },
    DivergenceWitness {
        id: "q27-code-point-oob",
        surface: "`string.codePointAt` out of range",
        q_rule: "Q27",
        summary: "subscript traps where JS returns undefined.",
        subscript: r#"export function main(): void {
  print(`${"a".codePointAt(1)}`);
}
"#,
        javascript: r#"console.log(String("a".codePointAt(1)));
"#,
        subscript_outcome: WitnessOutcome::Trap,
        javascript_outcome: WitnessOutcome::Value("undefined\n"),
    },
    DivergenceWitness {
        id: "q21-replace-all-empty",
        surface: "`string.replaceAll` with an empty pattern",
        q_rule: "Q21",
        summary: "subscript traps while JS inserts the replacement at every boundary.",
        subscript: r#"export function main(): void {
  print("ab".replaceAll("", "-"));
}
"#,
        javascript: r#"console.log("ab".replaceAll("", "-"));
"#,
        subscript_outcome: WitnessOutcome::Trap,
        javascript_outcome: WitnessOutcome::Value("-a-b-\n"),
    },
    DivergenceWitness {
        id: "q19-context-random",
        surface: "`Math.random`",
        q_rule: "Q19",
        summary: "A fresh Context starts a fixed host-reseedable sequence; Node uses its host PRNG.",
        subscript: r#"export function main(): void {
  const value: f64 = Math.random();
  if (value === 0.7085450778517304) {
    print("context-sequence");
  } else {
    print("host-sequence");
  }
}
"#,
        javascript: r#"const value = Math.random();
console.log(value === 0.7085450778517304 ? "context-sequence" : "host-sequence");
"#,
        subscript_outcome: WitnessOutcome::Value("context-sequence\n"),
        javascript_outcome: WitnessOutcome::Value("host-sequence\n"),
    },
    DivergenceWitness {
        id: "q20-invalid-date",
        surface: "`new Date(milliseconds)` outside the TimeClip range",
        q_rule: "Q20",
        summary: "subscript traps instead of constructing JS's Invalid-Date value.",
        subscript: r#"export function main(): void {
  const value: Date = new Date(8640000000000001);
  print(`${value.getTime()}`);
}
"#,
        javascript: r#"console.log(Number.isNaN(new Date(8640000000000001).getTime()));
"#,
        subscript_outcome: WitnessOutcome::Trap,
        javascript_outcome: WitnessOutcome::Value("true\n"),
    },
    DivergenceWitness {
        id: "q20-date-utc-out-of-range",
        surface: "`Date.UTC` outside the TimeClip range",
        q_rule: "Q20",
        summary: "subscript traps instead of returning JS's NaN time value.",
        subscript: r#"export function main(): void {
  print(`${Date.UTC(275760, 8, 14)}`);
}
"#,
        javascript: r#"console.log(Number.isNaN(Date.UTC(275760, 8, 14)));
"#,
        subscript_outcome: WitnessOutcome::Trap,
        javascript_outcome: WitnessOutcome::Value("true\n"),
    },
    DivergenceWitness {
        id: "q20-expanded-iso-year",
        surface: "`Date.toISOString` outside years 0000–9999",
        q_rule: "Q20",
        summary: "subscript traps where JS emits an expanded signed year.",
        subscript: r#"export function main(): void {
  const value: Date = new Date(253402300800000);
  print(value.toISOString());
}
"#,
        javascript: r#"console.log(new Date(253402300800000).toISOString());
"#,
        subscript_outcome: WitnessOutcome::Trap,
        javascript_outcome: WitnessOutcome::Value("+010000-01-01T00:00:00.000Z\n"),
    },
    DivergenceWitness {
        id: "q25-precise-parse-int",
        surface: "`parseInt` at radix 36",
        q_rule: "Q25",
        summary: "The exact integer conversion rounds correctly where Node's permitted approximation is one ulp away.",
        subscript: r#"export function main(): void {
  print(`${parseInt("hg3u5kjup4fqpcnqlor6do3ilczi9cbixkrepc", 36)}`);
}
"#,
        javascript: r#"console.log(`${parseInt("hg3u5kjup4fqpcnqlor6do3ilczi9cbixkrepc", 36)}`);
"#,
        subscript_outcome: WitnessOutcome::Value("6.682260239067032e+58\n"),
        javascript_outcome: WitnessOutcome::Value("6.682260239067033e+58\n"),
    },
    DivergenceWitness {
        id: "q4-empty-array-pop",
        surface: "`T[].pop` on an empty array",
        q_rule: "Q4",
        summary: "subscript traps because `T` cannot represent JS's undefined result.",
        subscript: r#"export function main(): void {
  const values: i32[] = [];
  print(`${values.pop()}`);
}
"#,
        javascript: r#"console.log(String([].pop()));
"#,
        subscript_outcome: WitnessOutcome::Trap,
        javascript_outcome: WitnessOutcome::Value("undefined\n"),
    },
    DivergenceWitness {
        id: "q27-empty-array-shift",
        surface: "`T[].shift` on an empty array",
        q_rule: "Q27",
        summary: "subscript traps because `T` cannot represent JS's undefined result.",
        subscript: r#"export function main(): void {
  const values: i32[] = [];
  print(`${values.shift()}`);
}
"#,
        javascript: r#"console.log(String([].shift()));
"#,
        subscript_outcome: WitnessOutcome::Trap,
        javascript_outcome: WitnessOutcome::Value("undefined\n"),
    },
    DivergenceWitness {
        id: "q24-map-get-miss",
        surface: "`Map.get` miss for a reference-class value",
        q_rule: "Q24",
        summary: "subscript returns null where JS returns undefined.",
        subscript: r#"class Box {
  value: i32;
  constructor(value: i32) {
    this.value = value;
  }
}

export function main(): void {
  const values: Map<string, Box> = new Map<string, Box>();
  const missing: Box | null = values.get("missing");
  print(`${missing === null}`);
}
"#,
        javascript: r#"console.log(`${new Map().get("missing") === null}`);
"#,
        subscript_outcome: WitnessOutcome::Value("true\n"),
        javascript_outcome: WitnessOutcome::Value("false\n"),
    },
    DivergenceWitness {
        id: "q11-generator-done-value",
        surface: "`IteratorResult.value` after generator completion",
        q_rule: "Q11",
        summary: "The completed result carries a zero-initialized `T` where JS carries undefined.",
        subscript: r#"function* values(): Generator<i32> {
  yield 7;
}

export function main(): void {
  const iterator: Generator<i32> = values();
  iterator.next();
  const result = iterator.next();
  print(`${result.done}|${result.value}`);
}
"#,
        javascript: r#"function* values() {
  yield 7;
}

const iterator = values();
iterator.next();
const result = iterator.next();
console.log(`${result.done}|${String(result.value)}`);
"#,
        subscript_outcome: WitnessOutcome::Value("true|0\n"),
        javascript_outcome: WitnessOutcome::Value("true|undefined\n"),
    },
];

/// Renders the complete generated Markdown reference from checker-owned
/// tables and executable witness metadata.
#[must_use]
pub fn render_markdown() -> String {
    let mut out = String::new();
    macro_rules! emit {
        ($($arg:tt)*) => {
            let _ = writeln!(out, $($arg)*);
        };
    }
    emit!(
        "<!-- DO NOT EDIT. Generated by `cargo run -p subscript-compiler --bin generate-api-reference`. -->"
    );
    emit!("\n# subscript API compatibility reference");
    emit!(
        "\n`tsconfig.json` loads the stock ES2022 library. This reference records which completed members the subscript checker accepts, which ES members it rejects, and where accepted results differ from JavaScript."
    );

    emit!("\n## Accepted surface");
    let accepted = ambient::accepted_api();
    let mut current_group = "";
    for item in accepted {
        if item.group != current_group {
            current_group = item.group;
            emit!("\n### {current_group}");
            emit!("\n| subscript signature | Behavior |\n|---|---|");
        }
        emit!(
            "| `{}` | {} |",
            escape_table(&item.signature),
            escape_table(item.summary)
        );
    }

    emit!("\n## Rejected surface");
    emit!(
        "\nThese are the checker's named S-code rejections, not a list of every unknown property. A blank replacement means the accepted language has no direct spelling."
    );
    emit!(
        "\n| Receiver / form | Rejected surface | S-code | Q-rule | Replacement | Reason | Reject corpus |\n|---|---|---|---|---|---|---|"
    );
    for rejection in ambient::rejected_api() {
        emit!(
            "| {} | `{}` | {} | {} | {} | {} | {} |",
            escape_table(rejection.group),
            escape_table(rejection.surface),
            rejection.code,
            rejection.q_rule,
            rejection
                .replacement
                .map(|value| format!("`{}`", escape_table(value)))
                .unwrap_or_else(|| "—".to_string()),
            escape_table(rejection.summary),
            rejection
                .corpus
                .map(|value| format!("`{value}`"))
                .unwrap_or_else(|| "—".to_string()),
        );
    }

    emit!("\n## Divergences from ECMA");
    emit!(
        "\nLast adversarial sweep of the listed checker-owned built-in surfaces against Node: **{DIVERGENCE_SWEEP_DATE}**. Each entry below is executed by the test suite. An outcome is either `Value(stdout bytes)` or `Trap`; two values agree only byte-for-byte, two traps agree, and `Trap` never agrees with a value."
    );
    for witness in DIVERGENCE_WITNESSES {
        emit!("\n### {} — {}", witness.surface, witness.q_rule);
        emit!("\n{}", witness.summary);
        emit!("\nsubscript:\n\n```ts\n{}```", witness.subscript);
        emit!("\nNode:\n\n```js\n{}```", witness.javascript);
        emit!(
            "\n- subscript result: `{}`\n- Node result: `{}`",
            escape_code(witness.subscript_outcome.markdown()),
            escape_code(witness.javascript_outcome.markdown()),
        );
    }
    out
}

fn escape_table(value: &str) -> String {
    value.replace('|', "\\|").replace('\n', " ")
}

fn escape_code(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('`', "\\`")
        .replace('\n', "\\n")
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use super::*;
    use crate::{check_program, SourceFile};

    #[test]
    fn checker_api_metadata_has_no_blank_summary_or_signature() {
        for item in ambient::accepted_api() {
            assert!(
                !item.signature.trim().is_empty(),
                "blank signature in {}",
                item.group
            );
            assert!(
                !item.summary.trim().is_empty(),
                "blank summary for {}",
                item.signature
            );
        }
        for rejection in ambient::rejected_api() {
            assert!(
                !rejection.surface.trim().is_empty(),
                "blank rejected surface"
            );
            assert!(
                !rejection.summary.trim().is_empty(),
                "blank summary for {}",
                rejection.surface
            );
            assert!(
                rejection.q_rule.starts_with('Q'),
                "bad Q-rule for {}",
                rejection.surface
            );
        }
    }

    #[test]
    fn generated_reference_is_byte_identical() {
        let committed = include_str!("../../generated-docs/api-reference.md");
        assert_eq!(render_markdown().as_bytes(), committed.as_bytes());
    }

    #[test]
    fn documented_reject_corpus_codes_are_checker_codes() {
        let reject_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../corpus/reject");
        for rejection in ambient::rejected_api() {
            let Some(file) = rejection.corpus else {
                continue;
            };
            let path = reject_dir.join(file);
            let source = fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
            let diagnostics = match check_program(&[SourceFile::new(file, source)]) {
                Err(diagnostics) => diagnostics,
                Ok(_) => panic!("{file}: checker unexpectedly accepted corpus"),
            };
            assert_eq!(
                diagnostics[0].code, rejection.code,
                "{file}: generated rejection code does not match the checker"
            );
        }
    }

    #[test]
    fn every_generated_rejection_is_rejected_by_the_checker() {
        for rejection in ambient::rejected_api() {
            let source = rejection_witness(rejection);
            let diagnostics = match check_program(&[SourceFile::new("api-rejection.ts", source)]) {
                Err(diagnostics) => diagnostics,
                Ok(_) => panic!(
                    "{} {}: checker unexpectedly accepted generated rejection",
                    rejection.group, rejection.surface
                ),
            };
            assert_eq!(
                diagnostics[0].code, rejection.code,
                "{} {}: generated code does not match the checker",
                rejection.group, rejection.surface
            );
        }
    }

    fn rejection_witness(rejection: ambient::ApiRejection) -> String {
        match (rejection.group, rejection.surface) {
            ("string", member) => format!(
                "export function main(): void {{\n  const value: string = \"x\";\n  value.{member}();\n}}\n"
            ),
            ("T[]", "sort()") => {
                "export function main(): void {\n  const values: i32[] = [1];\n  values.sort();\n}\n"
                    .to_string()
            }
            ("T[]", "reduce(callback)") => {
                "export function main(): void {\n  const values: i32[] = [1];\n  values.reduce((a: i32, b: i32): i32 => a + b);\n}\n"
                    .to_string()
            }
            ("T[]", "reduceRight(callback)") => {
                "export function main(): void {\n  const values: i32[] = [1];\n  values.reduceRight((a: i32, b: i32): i32 => a + b);\n}\n"
                    .to_string()
            }
            ("T[]", "splice(start, deleteCount, ...items)") => {
                "export function main(): void {\n  const values: i32[] = [1];\n  values.splice(0, 0, 2);\n}\n"
                    .to_string()
            }
            ("T[]", "unshift(value, ...values)") => {
                "export function main(): void {\n  const values: i32[] = [1];\n  values.unshift(2, 3);\n}\n"
                    .to_string()
            }
            ("T[]", member) => format!(
                "export function main(): void {{\n  const values: i32[] = [1];\n  values.{member}();\n}}\n"
            ),
            ("Date", "Date.parse") => {
                "export function main(): void {\n  Date.parse(\"2020\");\n}\n".to_string()
            }
            ("Date", "new Date()") => {
                "export function main(): void {\n  const value: Date = new Date();\n}\n"
                    .to_string()
            }
            ("Date", "new Date(year, month, ...)") => {
                "export function main(): void {\n  const value: Date = new Date(2020, 0);\n}\n"
                    .to_string()
            }
            ("Date", "template interpolation") => {
                "export function main(): void {\n  const value: Date = new Date(0);\n  print(`${value}`);\n}\n"
                    .to_string()
            }
            ("Date", "direct comparison") => {
                "export function main(): void {\n  const a: Date = new Date(0);\n  const b: Date = new Date(1);\n  print(`${a === b}`);\n}\n"
                    .to_string()
            }
            ("Date", "set*") => {
                "export function main(): void {\n  const value: Date = new Date(0);\n  value.setTime(1);\n}\n"
                    .to_string()
            }
            ("Date", member) => format!(
                "export function main(): void {{\n  const value: Date = new Date(0);\n  value.{member}();\n}}\n"
            ),
            ("Map<K, V>", member) => format!(
                "export function main(): void {{\n  const value: Map<i32, i32> = new Map<i32, i32>();\n  value.{member}();\n}}\n"
            ),
            ("Set<K>", member) => format!(
                "export function main(): void {{\n  const value: Set<i32> = new Set<i32>();\n  value.{member}();\n}}\n"
            ),
            ("global", "isNaN(value)") => {
                "export function main(): void {\n  print(`${isNaN(1)}`);\n}\n".to_string()
            }
            ("global", "isFinite(value)") => {
                "export function main(): void {\n  print(`${isFinite(1)}`);\n}\n".to_string()
            }
            ("global", "parseInt(value)") => {
                "export function main(): void {\n  print(`${parseInt(\"1\")}`);\n}\n".to_string()
            }
            ("Number", "Number(value)") => {
                "export function main(): void {\n  print(`${Number(\"1\")}`);\n}\n".to_string()
            }
            ("Number", "new Number(value)") => {
                "export function main(): void {\n  const value = new Number(1);\n}\n".to_string()
            }
            ("f32 / f64", "toLocaleString") => {
                "export function main(): void {\n  print((1.0 as f64).toLocaleString());\n}\n"
                    .to_string()
            }
            ("f32 / f64", "toString()") => {
                "export function main(): void {\n  print((1.0 as f64).toString());\n}\n"
                    .to_string()
            }
            ("f32 / f64", "toPrecision()") => {
                "export function main(): void {\n  print((1.0 as f64).toPrecision());\n}\n"
                    .to_string()
            }
            ("sized integers", "toFixed/toString/toExponential/toPrecision") => {
                "export function main(): void {\n  const value: i32 = 1;\n  print(value.toFixed(2));\n}\n"
                    .to_string()
            }
            ("Math", "max/min/hypot with more than two arguments") => {
                "export function main(): void {\n  print(`${Math.max(1, 2, 3)}`);\n}\n"
                    .to_string()
            }
            ("Math", "Math used as a value") => {
                "export function main(): void {\n  const value = Math;\n}\n".to_string()
            }
            ("Map<K, scalar V>", "get(key)") => {
                "export function main(): void {\n  const value: Map<i32, i32> = new Map<i32, i32>();\n  print(`${value.get(1)}`);\n}\n"
                    .to_string()
            }
            ("FixedArray<T, N>", "T[] methods") => {
                "export function main(): void {\n  const value: FixedArray<i32, 1> = [1];\n  print(`${value.indexOf(1)}`);\n}\n"
                    .to_string()
            }
            ("Map / Set", "new Map/Set(iterable)") => {
                "export function main(): void {\n  const value: Map<i32, i32> = new Map<i32, i32>([[1, 2]]);\n}\n"
                    .to_string()
            }
            ("Map", "groupBy") => {
                "export function main(): void {\n  Map.groupBy([1], (value: i32): i32 => value);\n}\n"
                    .to_string()
            }
            _ => panic!(
                "no checker witness for generated rejection {} {}",
                rejection.group, rejection.surface
            ),
        }
    }
}
