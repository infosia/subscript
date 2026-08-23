//! Generated AI-facing language reference and corpus index.
//!
//! The prose in this module is curated, while diagnostics, warning excerpts,
//! and corpus metadata are read from their checker and harness sources.

use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::{RuleCode, WarnCode};

const GENERATED_BY: &str = "cargo run --offline -p subscript-compiler --bin generate-api-reference";

const SURFACE_SUMMARY: &str = "subscript is a deliberately closed, TypeScript-shaped language for deterministic embedded programs. Exported functions are host entry points. Types are explicit and nominal; exceptions, dynamic evaluation, `any`, general unions, ordinary `undefined` values, and an implicit scheduler are outside the language. Standard-library acceptance is narrower than the stock ES2022 declarations; consult `api-reference.md` for the checker-owned surface and replacements.";

const SIZED_NUMERICS: &str = "Numeric types are `i8`, `u8`, `i16`, `u16`, `i32`, `u32`, `i64`, `u64`, `f16`, `f32`, and `f64`; bare `number` is rejected. Literals are checked against their contextual sized type. `f16` is storage-only: convert to `f32` or `f64` before arithmetic.";

const VALUE_REFERENCE_CLASSES: &str = "`@CStruct class` declares a nominal C-layout value class, copied on assignment and argument passing. The `@CStruct({ align: N })` form raises alignment to 2, 4, 8, or 16 bytes and rounds size without changing field offsets. A plain `class` declares a nominal heap reference class: `new` allocates it in the active `Context`, and assignments copy the reference. Value classes do not inherit, and same-shaped nominal types do not substitute for one another.";

const Q33_DESCRIPTORS: &str = "`@Descriptor class` declares a closed, data-only reference class for literal construction. A required member is written `name!: T`; a defaulted member is written `name?: T = default`. When `A` is a Q32 string-literal union alias, `name?: A` without an initializer is absence-capable: omission is a distinct state, explicit `undefined` is rejected, and reads are legal only in the present arm established by `member !== undefined` or the inverse arm of `member === undefined`. No other member type admits that spelling. Literals may be nested, may omit defaulted and absence-capable members, and remain constructible through a `Descriptor | null` contextual type. Construction uses an object literal in a descriptor context; `new Descriptor(...)`, literals against plain nominal classes (including through `| null`), methods, missing required members, and excess members are rejected.";

const Q32_LITERAL_UNIONS: &str = "Q32 admits declared aliases whose members are string literals, such as `type Mode = \"fast\" | \"safe\"`. The member set is closed and the alias is nominal: a non-member, an inline literal union, or a value from a distinct same-shaped alias is rejected. A switch over an alias uses member string literals as case labels. Without `default` it must name every member exactly once; with `default` any subset of distinct members is accepted. A default-less exhaustive alias switch is a diverging statement when every arm diverges, so it satisfies non-void function return flow without a trailing return.";

const DIVERGENCE_FLOW: &str = "`unreachable()` is legal only as a call statement. It marks that path as diverging for return-flow analysis and traps with `unreachable-reached` if execution reaches the call. Returns, `unreachable()`, and recursively exhaustive Q32 switches compose as diverging statements; a switch arm that falls through or breaks does not diverge.";

const Q34_ASYNC: &str = "Q34/R13/R36 has exactly three awaitable forms: `await Context.suspend()`, `await` applied directly to a named async-function call, and `await receiver.method(...)` for an async instance method on a plain reference class. The named async function can be generic when the call supplies explicit type arguments. The reference class can be generic. `Promise<T>` appears only in TypeScript-compatible return annotations; Promise objects, storage, constructors, statics, and combinators do not exist. A direct async call must be immediately awaited. Suspension resumes only when the embedding host steps pending computations; there is no event loop or microtask queue.";

const Q35_WORKERS: &str = "`Worker<In, Out>` runs one directly named, module-level synchronous function on an OS thread with a fresh `Context`. Its exact entry shape is `(inbox: Inbox<In>, outbox: Outbox<Out>): void`; entries cannot capture state. Message classes are monomorphized by the `In`/`Out` pair and may contain sized numerics, booleans, enums, string-literal union aliases, value classes, and `FixedArray` values composed recursively from those types. Worker handles and endpoints are context-affine: they cannot be module globals, class fields, array elements, any `Map`/`Set` type argument, or lambda captures, and they can only be created by `Worker.spawn`. Post all independent work before joining when parallel execution matters; `wait` and `poll` return `null` when their channel is empty or closed, and `close` plus `join` provide explicit shutdown.";

const MODULES: &str = "A program may import named exports from sibling source files with a relative `./name` specifier. The entry file and its imported siblings are checked as one program; exports are the host-visible entry surface.";

const COROUTINES: &str = "A `function*` coroutine yields typed values and is driven explicitly through `Generator<T>.next()` or the accepted `for...of` generator path. Suspension is caller- or host-driven; the language does not schedule coroutine steps implicitly.";

const MEMORY_MODEL: &str = "Reference allocations belong to a `Context`. `Context.free(value)` releases one allocation explicitly; `Context.collect()` performs an explicitly requested reachability collection. No collection runs implicitly. `Context.bytesOf<T>` returns zero-padded storage bytes for eligible `@CStruct` and `FixedArray` values. `bytesInto` writes that form, and `fromBytes` reconstructs storage without initialization. W001 flags unreleased loop allocations, W002 flags straight-line use after `Context.free`, and W003 flags fresh rooted callback userdata registered in a loop.";

struct Feature {
    title: &'static str,
    prose: &'static str,
    corpus: &'static [&'static str],
}

const FEATURES: &[Feature] = &[
    Feature {
        title: "Sized numerics",
        prose: SIZED_NUMERICS,
        corpus: &[
            "corpus/accept/a02-integer-types.ts",
            "corpus/accept/a03-integer-literals.ts",
            "corpus/accept/a46-narrow-numerics.ts",
            "corpus/accept/a49-f16-conversions.ts",
            "corpus/reject/r08-bare-number.ts",
            "corpus/reject/r09-int-literal-overflow.ts",
            "corpus/reject/r36-f16-arithmetic.ts",
        ],
    },
    Feature {
        title: "Value and reference classes",
        prose: VALUE_REFERENCE_CLASSES,
        corpus: &[
            "corpus/accept/a04-value-struct.ts",
            "corpus/accept/a05-nominal-identity.ts",
            "corpus/accept/a15-manual-lifetime.ts",
            "corpus/accept/a21-methods.ts",
            "corpus/accept/a141-cstruct-align.ts",
            "corpus/reject/r06-structural-substitution.ts",
            "corpus/reject/r07-value-class-extends.ts",
            "corpus/reject/r135-cstruct-align-below-natural.ts",
            "corpus/reject/r136-cstruct-align-not-in-set.ts",
        ],
    },
    Feature {
        title: "Q33 descriptors",
        prose: Q33_DESCRIPTORS,
        corpus: &[
            "corpus/accept/a92-descriptor-literals.ts",
            "corpus/accept/a117-descriptor-literal-nullable-member.ts",
            "corpus/accept/a118-absence-capable-member.ts",
            "corpus/reject/r90-descriptor-missing-required.ts",
            "corpus/reject/r91-descriptor-excess-member.ts",
            "corpus/reject/r92-literal-for-unmarked-class.ts",
            "corpus/reject/r93-descriptor-optional-without-default.ts",
            "corpus/reject/r94-descriptor-method.ts",
            "corpus/reject/r95-descriptor-new.ts",
            "corpus/reject/r137-descriptor-align.ts",
            "corpus/reject/r116-object-literal-nullable-class.ts",
            "corpus/reject/r117-explicit-undefined-member.ts",
            "corpus/reject/r118-unnarrowed-absence-read.ts",
        ],
    },
    Feature {
        title: "Q32 string-literal unions",
        prose: Q32_LITERAL_UNIONS,
        corpus: &[
            "corpus/accept/a91-string-literal-union.ts",
            "corpus/reject/r87-literal-union-nonmember.ts",
            "corpus/reject/r88-literal-union-inline.ts",
            "corpus/reject/r89-literal-union-cross-alias.ts",
            "corpus/accept/a115-switch-literal-union.ts",
            "corpus/accept/a116-exhaustive-switch-returns.ts",
            "corpus/reject/r112-switch-alias-missing-member.ts",
            "corpus/reject/r113-switch-alias-non-member.ts",
            "corpus/reject/r114-switch-alias-duplicate-member.ts",
        ],
    },
    Feature {
        title: "Divergence flow",
        prose: DIVERGENCE_FLOW,
        corpus: &[
            "corpus/accept/a116-exhaustive-switch-returns.ts",
            "corpus/reject/r115-unreachable-as-value.ts",
            "corpus/trap/t47-unreachable-reached.ts",
        ],
    },
    Feature {
        title: "Q34 async",
        prose: Q34_ASYNC,
        corpus: &[
            "corpus/accept/a93-async-chain.ts",
            "corpus/accept/a94-async-two-roots.ts",
            "corpus/accept/a95-interop-async-await.ts",
            "corpus/accept/a110-async-method-receiver.ts",
            "corpus/accept/a111-interop-async-method-poll.ts",
            "corpus/accept/a143-async-generic.ts",
            "corpus/reject/r96-new-promise.ts",
            "corpus/reject/r97-promise-combinator.ts",
            "corpus/reject/r98-promise-static.ts",
            "corpus/reject/r99-await-outside-async.ts",
            "corpus/reject/r100-floating-async-call.ts",
            "corpus/reject/r101-async-static-method.ts",
            "corpus/reject/r102-async-generator-method.ts",
            "corpus/reject/r103-async-cstruct-method.ts",
            "corpus/reject/r105-floating-async-method-call.ts",
            "corpus/reject/r140-async-lambda.ts",
        ],
    },
    Feature {
        title: "Q35 workers",
        prose: Q35_WORKERS,
        corpus: &[
            "corpus/accept/a112-worker-echo.ts",
            "corpus/accept/a113-worker-parallel.ts",
            "corpus/reject/r106-capturing-lambda-worker-entry.ts",
            "corpus/reject/r107-async-worker-entry.ts",
            "corpus/reject/r108-string-field-worker-message.ts",
            "corpus/reject/r109-worker-module-global.ts",
            "corpus/reject/r110-new-worker.ts",
            "corpus/reject/r111-worker-in-map-value.ts",
        ],
    },
    Feature {
        title: "Modules",
        prose: MODULES,
        corpus: &[
            "corpus/accept/a19-modules/main.ts",
            "corpus/accept/a19-modules/math.ts",
        ],
    },
    Feature {
        title: "Coroutines",
        prose: COROUTINES,
        corpus: &[
            "corpus/accept/a20-coroutine-generator.ts",
            "corpus/accept/a79-for-of-generator.ts",
        ],
    },
    Feature {
        title: "Memory model",
        prose: MEMORY_MODEL,
        corpus: &[
            "corpus/accept/a15-manual-lifetime.ts",
            "corpus/accept/a16-explicit-collect.ts",
            "corpus/accept/a90-callback-userdata-rooted.ts",
            "corpus/accept/a142-bytes-of.ts",
            "corpus/reject/r138-bytes-of-reference-class.ts",
            "corpus/reject/r139-bytes-of-string-element.ts",
            "corpus/warn/w01-loop-allocation-unreleased.ts",
            "corpus/warn/w02-use-after-free.ts",
            "corpus/warn/w03-fresh-callback-userdata-loop.ts",
            "corpus/trap/t22-double-delete-q6.ts",
            "corpus/trap/t23-use-after-delete-q6.ts",
            "corpus/trap/t46-callback-userdata-freed.ts",
            "corpus/trap/t51-bytes-into-range.ts",
        ],
    },
];

#[derive(Debug)]
struct Pin {
    file: String,
    code: String,
    line: usize,
}

#[derive(Debug)]
struct HeaderField {
    key: String,
    value: String,
}

#[derive(Debug)]
struct CorpusHeader {
    corpus: String,
    purpose: String,
    exercises: String,
    questions: String,
    fields: Vec<HeaderField>,
}

impl CorpusHeader {
    fn guidance(&self) -> impl Iterator<Item = &HeaderField> {
        self.fields.iter().filter(|field| {
            !matches!(
                field.key.as_str(),
                "corpus" | "purpose" | "exercises" | "questions" | "warning"
            )
        })
    }
}

/// Renders the generated current-state language reference.
///
/// # Errors
///
/// Fails when a harness pin cannot be parsed, a stable code has no pin, a
/// pinned corpus file is invalid, or a curated corpus link does not exist.
pub fn render_language_reference(repository_root: &Path) -> io::Result<String> {
    let reject_pins = reject_pins(repository_root)?;
    let warn_pins = warn_pins(repository_root)?;
    let mut out = String::new();

    writeln!(out, "<!-- DO NOT EDIT. Generated by `{GENERATED_BY}`. -->").expect("write to String");
    writeln!(out, "\n# subscript language reference").expect("write to String");
    writeln!(out, "\n## Current-state surface summary").expect("write to String");
    writeln!(out, "\n{SURFACE_SUMMARY}").expect("write to String");

    writeln!(out, "\n## Rejection rules").expect("write to String");
    writeln!(
        out,
        "\nEach excerpt comes from the first `(file, code, line)` pin for that code in `compiler/tests/corpus_reject.rs`; S100 therefore uses its first table entry. Excerpts include two source lines before and after the pinned line when available."
    )
    .expect("write to String");
    for code in RuleCode::ALL {
        let pin = reject_pins
            .iter()
            .find(|pin| pin.code == code.as_str())
            .ok_or_else(|| invalid(format!("{} has no reject-corpus pin", code)))?;
        render_code_entry(
            &mut out,
            repository_root,
            "reject",
            code.as_str(),
            code.explanation(),
            pin,
        )?;
    }

    writeln!(out, "\n## Warning rules").expect("write to String");
    writeln!(
        out,
        "\nWarnings do not change acceptance unless the CLI is run with `--deny-warnings`. Each excerpt comes from the first `(file, code, line)` pin in `compiler/tests/corpus_warn.rs`."
    )
    .expect("write to String");
    for code in WarnCode::ALL {
        let pin = warn_pins
            .iter()
            .find(|pin| pin.code == code.as_str())
            .ok_or_else(|| invalid(format!("{} has no warning-corpus pin", code)))?;
        render_code_entry(
            &mut out,
            repository_root,
            "warn",
            code.as_str(),
            code.explanation(),
            pin,
        )?;
    }

    writeln!(out, "\n## Feature guide").expect("write to String");
    for feature in FEATURES {
        writeln!(out, "\n### {}", feature.title).expect("write to String");
        writeln!(out, "\n{}", feature.prose).expect("write to String");
        write!(out, "\nCorpus: ").expect("write to String");
        for (index, relative) in feature.corpus.iter().enumerate() {
            let path = repository_root.join(relative);
            if !path.is_file() {
                return Err(invalid(format!(
                    "feature {} links missing corpus file {}",
                    feature.title,
                    path.display()
                )));
            }
            if index > 0 {
                write!(out, ", ").expect("write to String");
            }
            write!(out, "[`{relative}`](../{relative})").expect("write to String");
        }
        writeln!(out, ".").expect("write to String");
    }

    Ok(out)
}

/// Renders the generated four-arm corpus index.
///
/// # Errors
///
/// Fails when a corpus entry lacks a structured header, names the wrong
/// source path, or lacks its required accept/trap expected-output file.
pub fn render_corpus_index(repository_root: &Path) -> io::Result<String> {
    let mut out = String::new();
    writeln!(out, "<!-- DO NOT EDIT. Generated by `{GENERATED_BY}`. -->").expect("write to String");
    writeln!(out, "\n# subscript corpus index").expect("write to String");
    writeln!(
        out,
        "\nThis index is derived from the structured header comments on every TypeScript corpus source."
    )
    .expect("write to String");

    for arm in ["accept", "reject", "warn", "trap"] {
        render_corpus_table(&mut out, repository_root, arm)?;
    }
    Ok(out)
}

fn render_code_entry(
    out: &mut String,
    repository_root: &Path,
    arm: &str,
    code: &str,
    explanation: &str,
    pin: &Pin,
) -> io::Result<()> {
    let relative = format!("corpus/{arm}/{}", pin.file);
    let path = repository_root.join(&relative);
    let source = read(&path)?;
    let header = parse_header(&path, &source)?;
    let expected_name = format!("{arm}/{}", pin.file.trim_end_matches(".ts"));
    if header.corpus != expected_name {
        return Err(invalid(format!(
            "{}: corpus header `{}` does not match `{expected_name}`",
            path.display(),
            header.corpus
        )));
    }
    let lines = source.lines().collect::<Vec<_>>();
    if pin.line == 0 || pin.line > lines.len() {
        return Err(invalid(format!(
            "{}:{}: harness pin is outside the {}-line file",
            path.display(),
            pin.line,
            lines.len()
        )));
    }
    let start = pin.line.saturating_sub(3);
    let end = usize::min(pin.line + 2, lines.len());

    writeln!(out, "\n### {code}").expect("write to String");
    writeln!(out, "\n{explanation}").expect("write to String");
    writeln!(
        out,
        "\nPinned corpus: [`{relative}`](../{relative}), line {}.",
        pin.line
    )
    .expect("write to String");

    let guidance = header.guidance().collect::<Vec<_>>();
    if !guidance.is_empty() {
        writeln!(out, "\nHeader guidance:\n\n```text").expect("write to String");
        for field in guidance {
            writeln!(out, "// {}: {}", field.key, field.value).expect("write to String");
        }
        writeln!(out, "```").expect("write to String");
    }

    writeln!(out, "\n```ts").expect("write to String");
    for line in &lines[start..end] {
        writeln!(out, "{line}").expect("write to String");
    }
    writeln!(out, "```").expect("write to String");
    Ok(())
}

fn render_corpus_table(out: &mut String, repository_root: &Path, arm: &str) -> io::Result<()> {
    let arm_dir = repository_root.join("corpus").join(arm);
    let mut paths = Vec::new();
    collect_typescript_files(&arm_dir, &mut paths)?;
    paths.sort_by_key(|path| normalized_relative(repository_root, path));
    if paths.is_empty() {
        return Err(invalid(format!(
            "{} has no TypeScript corpus entries",
            arm_dir.display()
        )));
    }

    let title = match arm {
        "accept" => "Accept",
        "reject" => "Reject",
        "warn" => "Warn",
        "trap" => "Trap",
        _ => return Err(invalid(format!("unknown corpus arm {arm}"))),
    };
    writeln!(out, "\n## {title}").expect("write to String");
    let has_expected = matches!(arm, "accept" | "trap");
    if has_expected {
        writeln!(
            out,
            "\n| Entry | Purpose | Exercises | Questions | Expected output |\n|---|---|---|---|---|"
        )
        .expect("write to String");
    } else {
        writeln!(
            out,
            "\n| Entry | Purpose | Exercises | Questions |\n|---|---|---|---|"
        )
        .expect("write to String");
    }

    for path in paths {
        let relative = normalized_relative(repository_root, &path);
        let source = read(&path)?;
        let header = parse_header(&path, &source)?;
        let expected_corpus_name = relative
            .strip_prefix("corpus/")
            .and_then(|value| value.strip_suffix(".ts"))
            .ok_or_else(|| invalid(format!("invalid corpus source path {relative}")))?;
        if header.corpus != expected_corpus_name {
            return Err(invalid(format!(
                "{}: corpus header `{}` does not match `{expected_corpus_name}`",
                path.display(),
                header.corpus
            )));
        }
        write!(
            out,
            "| [`{}`](../{}) | {} | {} | {} |",
            escape_table(&header.corpus),
            relative,
            escape_table(&header.purpose),
            escape_table(&header.exercises),
            escape_table(&header.questions),
        )
        .expect("write to String");
        if has_expected {
            let expected = expected_path(&arm_dir, &path)?;
            let expected_relative = normalized_relative(repository_root, &expected);
            write!(
                out,
                " [`{}`](../{}) |",
                escape_table(&expected_relative),
                expected_relative
            )
            .expect("write to String");
        }
        writeln!(out).expect("write to String");
    }
    Ok(())
}

fn reject_pins(repository_root: &Path) -> io::Result<Vec<Pin>> {
    let path = repository_root.join("compiler/tests/corpus_reject.rs");
    let source = read(&path)?;
    let mut pins = parse_pin_table(&path, &source, "EXPECTED", "RuleCode::")?;
    pins.extend(parse_pin_table(
        &path,
        &source,
        "REGEX_EXPECTED",
        "RuleCode::",
    )?);
    Ok(pins)
}

fn warn_pins(repository_root: &Path) -> io::Result<Vec<Pin>> {
    let path = repository_root.join("compiler/tests/corpus_warn.rs");
    let source = read(&path)?;
    parse_pin_table(&path, &source, "EXPECTED", "WarnCode::")
}

fn parse_pin_table(
    path: &Path,
    source: &str,
    table_name: &str,
    code_prefix: &str,
) -> io::Result<Vec<Pin>> {
    let declaration = format!("const {table_name}:");
    let declaration_start = source.find(&declaration).ok_or_else(|| {
        invalid(format!(
            "{}: missing pin table declaration `{declaration}`",
            path.display()
        ))
    })?;
    let after_declaration = &source[declaration_start..];
    let array_start = after_declaration.find("= &[").ok_or_else(|| {
        invalid(format!(
            "{}: malformed pin table `{table_name}`",
            path.display()
        ))
    })? + 4;
    let after_start = &after_declaration[array_start..];
    let array_end = after_start.find("]; ").or_else(|| after_start.find("];\n"));
    let array_end = array_end
        .or_else(|| after_start.find("];"))
        .ok_or_else(|| {
            invalid(format!(
                "{}: unterminated pin table `{table_name}`",
                path.display()
            ))
        })?;
    let table = &after_start[..array_end];

    let mut pins = Vec::new();
    let mut cursor = 0;
    while let Some(quote_offset) = table[cursor..].find('"') {
        let file_start = cursor + quote_offset + 1;
        let file_end_offset = table[file_start..].find('"').ok_or_else(|| {
            invalid(format!(
                "{}: unterminated filename in `{table_name}`",
                path.display()
            ))
        })?;
        let file_end = file_start + file_end_offset;
        let file = &table[file_start..file_end];
        cursor = file_end + 1;
        if !file.ends_with(".ts") {
            continue;
        }

        let code_start_offset = table[cursor..].find(code_prefix).ok_or_else(|| {
            invalid(format!(
                "{}: `{table_name}` entry for {file} has no {code_prefix} code",
                path.display()
            ))
        })?;
        let code_start = cursor + code_start_offset + code_prefix.len();
        let code_len = table[code_start..]
            .chars()
            .take_while(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit())
            .map(char::len_utf8)
            .sum::<usize>();
        if code_len == 0 {
            return Err(invalid(format!(
                "{}: `{table_name}` entry for {file} has an empty code",
                path.display()
            )));
        }
        let code_end = code_start + code_len;
        let after_code = &table[code_end..];
        let comma = after_code.find(',').ok_or_else(|| {
            invalid(format!(
                "{}: `{table_name}` entry for {file} has no pinned line",
                path.display()
            ))
        })?;
        let after_comma = &after_code[comma + 1..];
        let line_text = after_comma.trim_start();
        let line_len = line_text
            .chars()
            .take_while(|ch| ch.is_ascii_digit())
            .map(char::len_utf8)
            .sum::<usize>();
        let line = line_text[..line_len].parse::<usize>().map_err(|_| {
            invalid(format!(
                "{}: `{table_name}` entry for {file} has an invalid pinned line",
                path.display()
            ))
        })?;
        pins.push(Pin {
            file: file.to_string(),
            code: table[code_start..code_end].to_string(),
            line,
        });
        cursor = code_end + comma + 1 + (after_comma.len() - line_text.len()) + line_len;
    }

    if pins.is_empty() {
        return Err(invalid(format!(
            "{}: pin table `{table_name}` is empty",
            path.display()
        )));
    }
    Ok(pins)
}

fn parse_header(path: &Path, source: &str) -> io::Result<CorpusHeader> {
    let mut fields: Vec<HeaderField> = Vec::new();
    let mut current: Option<usize> = None;

    for line in source.lines() {
        let Some(comment) = line.strip_prefix("//") else {
            break;
        };
        let content = comment.strip_prefix(' ').unwrap_or(comment);
        if content.starts_with(char::is_whitespace) {
            append_continuation(&mut fields, current, content.trim());
            continue;
        }
        let mut parsed_field = false;
        for segment in content.split(" // ") {
            if let Some((key, value)) = segment.split_once(':') {
                let key = key.trim();
                if !key.is_empty() {
                    fields.push(HeaderField {
                        key: key.to_string(),
                        value: value.trim().to_string(),
                    });
                    current = Some(fields.len() - 1);
                    parsed_field = true;
                }
            }
        }
        if parsed_field {
            continue;
        }
        append_continuation(&mut fields, current, content.trim());
    }

    let required = |name: &str| -> io::Result<String> {
        let matches = fields
            .iter()
            .filter(|field| field.key == name)
            .collect::<Vec<_>>();
        if matches.len() != 1 || matches[0].value.trim().is_empty() {
            return Err(invalid(format!(
                "{}: expected exactly one non-empty `// {name}:` header field",
                path.display()
            )));
        }
        Ok(matches[0].value.clone())
    };

    Ok(CorpusHeader {
        corpus: required("corpus")?,
        purpose: required("purpose")?,
        exercises: required("exercises")?,
        questions: required("questions")?,
        fields,
    })
}

fn append_continuation(fields: &mut [HeaderField], current: Option<usize>, value: &str) {
    if value.is_empty() {
        return;
    }
    if let Some(field) = current.and_then(|index| fields.get_mut(index)) {
        if !field.value.is_empty() {
            field.value.push(' ');
        }
        field.value.push_str(value);
    }
}

fn collect_typescript_files(directory: &Path, output: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(directory).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!("read {}: {error}", directory.display()),
        )
    })? {
        let entry = entry.map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("read {} entry: {error}", directory.display()),
            )
        })?;
        let path = entry.path();
        if path.is_dir() {
            collect_typescript_files(&path, output)?;
        } else if path.extension().and_then(|value| value.to_str()) == Some("ts") {
            output.push(path);
        }
    }
    Ok(())
}

fn expected_path(arm_dir: &Path, source_path: &Path) -> io::Result<PathBuf> {
    let beside_source = source_path.with_extension("expected");
    if beside_source.is_file() {
        return Ok(beside_source);
    }
    let relative = source_path.strip_prefix(arm_dir).map_err(|_| {
        invalid(format!(
            "{} is not beneath {}",
            source_path.display(),
            arm_dir.display()
        ))
    })?;
    let entry_name = relative
        .components()
        .next()
        .ok_or_else(|| invalid(format!("invalid corpus path {}", source_path.display())))?
        .as_os_str();
    let shared = arm_dir.join(entry_name).with_extension("expected");
    if shared.is_file() {
        return Ok(shared);
    }
    Err(invalid(format!(
        "{}: no expected-output file beside the source or entry directory",
        source_path.display()
    )))
}

fn normalized_relative(repository_root: &Path, path: &Path) -> String {
    path.strip_prefix(repository_root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn escape_table(value: &str) -> String {
    value.replace('|', "\\|").replace('\n', " ")
}

fn read(path: &Path) -> io::Result<String> {
    fs::read_to_string(path)
        .map_err(|error| io::Error::new(error.kind(), format!("read {}: {error}", path.display())))
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_ai_references_are_byte_identical() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        let generated = root.join("generated-docs");
        let language = render_language_reference(&root).expect("render language reference");
        let corpus = render_corpus_index(&root).expect("render corpus index");
        let committed_language =
            fs::read(generated.join("language-reference.md")).expect("read language reference");
        let committed_corpus =
            fs::read(generated.join("corpus-index.md")).expect("read corpus index");
        assert_eq!(language.as_bytes(), committed_language.as_slice());
        assert_eq!(corpus.as_bytes(), committed_corpus.as_slice());
    }
}
