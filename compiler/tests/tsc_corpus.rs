//! Stage 1 gate for compiler.md §69: each top-level corpus entry states
//! the result from stock TypeScript, and one batched process measures it.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use subscript_compiler::repository_relative;

const HEADER_PREFIX: &str = "// tsc: ";

#[derive(Clone, Debug, Eq, PartialEq)]
enum TscClaim {
    Accepts,
    Rejects(BTreeSet<String>),
}

impl TscClaim {
    fn parse(text: &str) -> Result<Self, String> {
        if text == "accepts" {
            return Ok(Self::Accepts);
        }
        let Some(codes) = text.strip_prefix("rejects ") else {
            return Err("expected `accepts` or `rejects TS<code>[, TS<code> ...]`".to_string());
        };
        let codes: BTreeSet<String> = codes.split(", ").map(str::to_string).collect();
        if codes.is_empty() || codes.iter().any(|code| !is_diagnostic_code(code.as_str())) {
            return Err("expected `rejects TS<code>[, TS<code> ...]`".to_string());
        }
        Ok(Self::Rejects(codes))
    }

    fn measured(codes: BTreeSet<String>) -> Self {
        if codes.is_empty() {
            Self::Accepts
        } else {
            Self::Rejects(codes)
        }
    }

    fn display(&self) -> String {
        match self {
            Self::Accepts => "accepts".to_string(),
            Self::Rejects(codes) => format!(
                "rejects {}",
                codes.iter().cloned().collect::<Vec<_>>().join(", ")
            ),
        }
    }
}

fn is_diagnostic_code(code: &str) -> bool {
    code.strip_prefix("TS").is_some_and(|digits| {
        !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
    })
}

#[derive(Debug)]
struct Entry {
    relative: String,
    absolute: PathBuf,
    accept: bool,
    external_module: bool,
    claim: TscClaim,
}

struct TempProjectDirectory(PathBuf);

impl TempProjectDirectory {
    fn create() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock must follow the Unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "subscript-tsc-corpus-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap_or_else(|error| panic!("create {}: {error}", path.display()));
        Self(path)
    }
}

impl Drop for TempProjectDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler crate must have a workspace parent")
        .to_path_buf()
}

fn corpus_entries(root: &Path) -> Result<Vec<Entry>, String> {
    let mut entries = Vec::new();
    for (kind, accept) in [("accept", true), ("reject", false)] {
        let directory = root.join("corpus").join(kind);
        let mut paths: Vec<PathBuf> = fs::read_dir(&directory)
            .map_err(|error| format!("read {}: {error}", directory.display()))?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.is_file() && path.extension().is_some_and(|ext| ext == "ts"))
            .collect();
        paths.sort();
        for absolute in paths {
            let relative = repository_relative(root, &absolute)
                .expect("corpus path must be below the workspace root");
            let source = fs::read_to_string(&absolute)
                .map_err(|error| format!("read {relative}: {error}"))?;
            for obsolete in [
                "// tsc-clean-standalone:",
                "// tsc-status:",
                "// tsc-clean:",
            ] {
                if source.lines().any(|line| line.starts_with(obsolete)) {
                    return Err(format!(
                        "{relative}: obsolete `{obsolete}` header; use `{HEADER_PREFIX}`"
                    ));
                }
            }
            let headers: Vec<&str> = source
                .lines()
                .filter_map(|line| line.strip_prefix(HEADER_PREFIX))
                .collect();
            if headers.len() != 1 {
                return Err(format!(
                    "{relative}: expected one `{HEADER_PREFIX}<claim>` header, found {}",
                    headers.len()
                ));
            }
            let claim_text = headers[0]
                .split_once("; js-comparable: ")
                .map_or(headers[0], |(claim, _)| claim);
            let claim = TscClaim::parse(claim_text)
                .map_err(|error| format!("{relative}: invalid tsc header: {error}"))?;
            if !accept {
                let expected_errors: Vec<&str> = source
                    .lines()
                    .filter_map(|line| line.strip_prefix("// expected-error: "))
                    .collect();
                if expected_errors.len() != 1 || expected_errors[0].trim().is_empty() {
                    return Err(format!(
                        "{relative}: expected one nonempty `// expected-error:` header, found {}",
                        expected_errors.len()
                    ));
                }
            }
            let external_module = source.lines().any(|line| {
                let line = line.trim_start();
                line.starts_with("import ") || line.starts_with("export ")
            });
            entries.push(Entry {
                relative,
                absolute,
                accept,
                external_module,
                claim,
            });
        }
    }
    Ok(entries)
}

fn json_string(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len() + 2);
    escaped.push('"');
    for character in text.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            character => escaped.push(character),
        }
    }
    escaped.push('"');
    escaped
}

/// Builds a project that measures `files` with `tsconfig.json`'s
/// options. One text, so every gate in this file measures the
/// configuration the repository ships.
fn tsconfig(files: &[PathBuf]) -> String {
    let listed = files
        .iter()
        .map(|path| json_string(path.to_string_lossy().as_ref()))
        .collect::<Vec<_>>()
        .join(",\n    ");
    format!(
        "{{\n  \"compilerOptions\": {{\n    \"strict\": true,\n    \"noEmit\": true,\n    \"target\": \"ES2022\",\n    \"module\": \"ESNext\",\n    \"moduleResolution\": \"Bundler\",\n    \"lib\": [\"ES2022\", \"ESNext.Disposable\"],\n    \"types\": [],\n    \"forceConsistentCasingInFileNames\": true\n  }},\n  \"files\": [\n    {listed}\n  ]\n}}\n"
    )
}

/// Answers the pinned TypeScript compiler.
///
/// `node_modules/.bin/tsc` is a POSIX shell script. Windows cannot
/// execute it (`os error 193`); npm writes `tsc.cmd` beside it for that
/// host.
fn tsc_binary(root: &Path) -> PathBuf {
    let tsc = root.join(if cfg!(windows) {
        "node_modules/.bin/tsc.cmd"
    } else {
        "node_modules/.bin/tsc"
    });
    assert!(
        tsc.is_file(),
        "the pinned TypeScript compiler is absent at {}",
        tsc.display()
    );
    tsc
}

fn write_projects(
    root: &Path,
    entries: &[Entry],
    temporary: &Path,
) -> Result<Vec<PathBuf>, String> {
    let mut ambient: Vec<PathBuf> = fs::read_dir(root.join("corpus/interop"))
        .map_err(|error| format!("read corpus/interop: {error}"))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().ends_with(".d.ts"))
        })
        .collect();
    ambient.sort();
    ambient.push(root.join("prelude/lang.d.ts"));

    let modules: Vec<&PathBuf> = entries
        .iter()
        .filter(|entry| entry.external_module)
        .map(|entry| &entry.absolute)
        .collect();
    let scripts = entries
        .iter()
        .filter(|entry| !entry.external_module)
        .map(|entry| vec![&entry.absolute]);
    std::iter::once(modules)
        .chain(scripts)
        .enumerate()
        .map(|(index, entry_files)| {
            let config_path = temporary.join(format!("entry-batch-{index:03}.json"));
            let files: Vec<PathBuf> = entry_files
                .into_iter()
                .chain(ambient.iter())
                .cloned()
                .collect();
            fs::write(&config_path, tsconfig(&files))
                .map_err(|error| format!("write {}: {error}", config_path.display()))?;
            Ok(config_path)
        })
        .collect()
}

fn diagnostic_codes(
    root: &Path,
    entries: &[Entry],
    output: &str,
) -> Result<BTreeMap<String, BTreeSet<String>>, Vec<String>> {
    let entry_names: BTreeSet<&str> = entries
        .iter()
        .map(|entry| entry.relative.as_str())
        .collect();
    let mut codes = BTreeMap::<String, BTreeSet<String>>::new();
    let mut unowned = Vec::new();
    for line in output.lines().filter(|line| line.contains("error TS")) {
        let Some(marker) = line.find("): error TS") else {
            unowned.push(line.to_string());
            continue;
        };
        let Some(position_start) = line[..marker].rfind('(') else {
            unowned.push(line.to_string());
            continue;
        };
        let source_path = Path::new(&line[..position_start]);
        let absolute = if source_path.is_absolute() {
            source_path.to_path_buf()
        } else {
            root.join(source_path)
        };
        let Some(relative) = repository_relative(root, &absolute) else {
            unowned.push(line.to_string());
            continue;
        };
        if !entry_names.contains(relative.as_str()) {
            unowned.push(line.to_string());
            continue;
        }
        let code_start = marker + "): error ".len();
        let Some(code) = line[code_start..].split(':').next() else {
            unowned.push(line.to_string());
            continue;
        };
        if !is_diagnostic_code(code) {
            unowned.push(line.to_string());
            continue;
        }
        codes.entry(relative).or_default().insert(code.to_string());
    }
    if unowned.is_empty() {
        Ok(codes)
    } else {
        Err(unowned)
    }
}

#[test]
fn every_corpus_tsc_header_matches_measured_tsc() {
    let root = project_root();
    let entries = corpus_entries(&root).unwrap_or_else(|error| panic!("{error}"));
    let temporary = TempProjectDirectory::create();
    let projects =
        write_projects(&root, &entries, &temporary.0).unwrap_or_else(|error| panic!("{error}"));
    let tsc = tsc_binary(&root);

    let started = Instant::now();
    let output = Command::new(&tsc)
        .arg("--build")
        .arg("--pretty")
        .arg("false")
        .args(&projects)
        .current_dir(&root)
        .output()
        .unwrap_or_else(|error| panic!("run {}: {error}", tsc.display()));
    let elapsed = started.elapsed();
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let codes = diagnostic_codes(&root, &entries, &combined).unwrap_or_else(|unowned| {
        panic!(
            "tsc emitted diagnostics that belong to no corpus entry:\n{}",
            unowned.join("\n")
        )
    });
    assert!(
        output.status.success() || !codes.is_empty(),
        "tsc exited with {} and emitted no attributed diagnostic:\n{combined}",
        output.status
    );

    let mut disagreements = Vec::new();
    for entry in &entries {
        let actual = TscClaim::measured(codes.get(&entry.relative).cloned().unwrap_or_default());
        if entry.claim != actual {
            disagreements.push(format!(
                "{}: header says `{}`; tsc said `{}`",
                entry.relative,
                entry.claim.display(),
                actual.display()
            ));
        }
        if entry.accept && actual != TscClaim::Accepts {
            disagreements.push(format!(
                "{}: an accept entry must type-check; tsc said `{}`",
                entry.relative,
                actual.display()
            ));
        }
    }
    eprintln!(
        "tsc corpus gate: {} entries measured in {:.3}s",
        entries.len(),
        elapsed.as_secs_f64()
    );
    assert!(
        disagreements.is_empty(),
        "tsc corpus header disagreement(s):\n{}",
        disagreements.join("\n")
    );
}

/// One form that §79 rule 6 keeps out of the reject corpus, and the
/// `tsc` class this repository records for it.
struct RecordedForm {
    stem: &'static str,
    body: &'static str,
    claim: &'static str,
}

/// Runs the pinned TypeScript compiler over every form in one batch and
/// answers each disagreement between the recorded class and the measured
/// one, with the compiler's output. `source` turns a form's body into
/// the file the batch holds.
fn recorded_form_disagreements(
    batch: &str,
    forms: &[RecordedForm],
    source: impl Fn(&str) -> String,
) -> Result<(), String> {
    let root = project_root();
    let temporary = TempProjectDirectory::create();
    let mut files: Vec<PathBuf> = Vec::new();
    for form in forms {
        let path = temporary.0.join(format!("{}.ts", form.stem));
        fs::write(&path, source(form.body))
            .unwrap_or_else(|error| panic!("write {}: {error}", path.display()));
        files.push(path);
    }
    files.push(root.join("prelude/lang.d.ts"));
    let config_path = temporary.0.join(format!("{batch}.json"));
    fs::write(&config_path, tsconfig(&files))
        .unwrap_or_else(|error| panic!("write {}: {error}", config_path.display()));

    let tsc = tsc_binary(&root);
    let output = Command::new(&tsc)
        .arg("--build")
        .arg("--pretty")
        .arg("false")
        .arg(&config_path)
        .current_dir(&root)
        .output()
        .unwrap_or_else(|error| panic!("run {}: {error}", tsc.display()));
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let mut measured: BTreeMap<&str, BTreeSet<String>> = forms
        .iter()
        .map(|form| (form.stem, BTreeSet::new()))
        .collect();
    let mut unowned = Vec::new();
    for line in combined.lines().filter(|line| line.contains("error TS")) {
        let owner = line.find("): error TS").and_then(|marker| {
            let file = &line[..line[..marker].rfind('(')?];
            let code = line[marker + "): error ".len()..].split(':').next()?;
            let form = forms
                .iter()
                .find(|form| file.ends_with(&format!("{}.ts", form.stem)))?;
            is_diagnostic_code(code).then(|| (form.stem, code.to_owned()))
        });
        match owner {
            Some((stem, code)) => {
                measured
                    .get_mut(stem)
                    .expect("every stem has an entry")
                    .insert(code);
            }
            None => unowned.push(line.to_owned()),
        }
    }
    if !unowned.is_empty() {
        return Err(format!(
            "tsc emitted diagnostics that belong to no measured form:\n{}",
            unowned.join("\n")
        ));
    }

    let mut disagreements = Vec::new();
    for form in forms {
        let recorded = TscClaim::parse(form.claim)
            .unwrap_or_else(|error| panic!("{}: invalid recorded claim: {error}", form.stem));
        let actual = TscClaim::measured(measured[form.stem].clone());
        if recorded != actual {
            disagreements.push(format!(
                "{}: this repository records `{}`; tsc said `{}`",
                form.stem,
                recorded.display(),
                actual.display()
            ));
        }
    }
    eprintln!(
        "{batch} tsc pin: {} form(s) measured under the pinned TypeScript compiler",
        forms.len()
    );
    if disagreements.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "recorded tsc class disagreement(s):\n{}\ntsc said:\n{combined}",
            disagreements.join("\n")
        ))
    }
}

/// compiler.md §104.6 and §105.5: §79 rule 6 keeps a `tsc: rejects`
/// entry off a checker site that carries a divergence variant. The
/// annotated bare-`Map` forms therefore reach no corpus entry, and
/// this test is their pin. It runs the pinned TypeScript compiler over
/// them and compares the codes it measures against the recorded class.
///
/// The unannotated forms run in the same batch. Each one differs from
/// the annotated form beside it by the annotation alone, and each one
/// measures clean. A batch that reported one code everywhere fails
/// here.
#[test]
fn the_annotated_bare_map_forms_measure_their_recorded_tsc_class() {
    let forms = [
        RecordedForm {
            stem: "annotated-spread",
            body: "  const keys: i32[] = [...map];\n  print(`${keys.length}`);",
            claim: "rejects TS2322",
        },
        RecordedForm {
            stem: "unannotated-spread",
            body: "  const keys = [...map];\n  print(`${keys.length}`);",
            claim: "accepts",
        },
        RecordedForm {
            stem: "annotated-for-of",
            body: "  for (const key of map) {\n    const n: i32 = key;\n    print(`${n}`);\n  }",
            claim: "rejects TS2322",
        },
        RecordedForm {
            stem: "unannotated-for-of",
            body: "  for (const key of map) {\n    print(`${key}`);\n  }",
            claim: "accepts",
        },
        RecordedForm {
            stem: "annotated-array-from",
            body: "  const keys: i32[] = Array.from(map);\n  print(`${keys.length}`);",
            claim: "rejects TS2322",
        },
        RecordedForm {
            stem: "unannotated-array-from",
            body: "  const keys = Array.from(map);\n  print(`${keys.length}`);",
            claim: "accepts",
        },
    ];
    let result = recorded_form_disagreements("bare-map-forms", &forms, |body| {
        format!(
            "export function main(): void {{\n\
             \x20 const map: Map<i32, string> = new Map<i32, string>();\n\
             \x20 map.set(1, \"one\");\n\
             {body}\n\
             }}\n"
        )
    });
    assert!(result.is_ok(), "{}", result.unwrap_err());
}

/// compiler.md §107.3 and §79 rule 6: the pattern rejection sites that
/// carry a divergence variant each also reject a program `tsc`
/// rejects, so that class reaches no corpus entry. This test is its
/// pin: every form below is rejected here at the named variant, and the
/// pinned TypeScript compiler measures the class recorded beside it.
///
/// The source-shape site serves both classes by the source alone: an
/// array pattern over a `string` is `tsc`-clean, and so is a pattern
/// parameter over a `string`, because a `string` iterates. A field
/// pattern over a `string` and an array pattern over an `i32` are not.
/// The rest, default, nested, and field-rest sites each serve both
/// classes through one source of the wrong shape.
#[test]
fn the_rejected_pattern_forms_measure_their_recorded_tsc_class() {
    use subscript_compiler::divergence::Divergence;

    let forms = [
        RecordedForm {
            stem: "array-pattern-over-string",
            body: "export function main(): void {\n  const text: string = \"ab\";\n  const [a, b] = text;\n  print(`${a}${b}`);\n}\n",
            claim: "accepts",
        },
        RecordedForm {
            stem: "field-pattern-over-string",
            body: "export function main(): void {\n  const text: string = \"ab\";\n  const { x } = text;\n  print(`${x}`);\n}\n",
            claim: "rejects TS2339",
        },
        RecordedForm {
            stem: "function-parameter-over-string",
            body: "function take([a, b]: string): string { return a + b; }\nexport function main(): void {\n  print(take(\"ab\"));\n}\n",
            claim: "accepts",
        },
        RecordedForm {
            stem: "method-parameter-over-i32",
            body: "class Box {\n  m([a, b]: i32): i32 { return a + b; }\n}\nexport function main(): void {\n  print(`${new Box().m(1)}`);\n}\n",
            claim: "rejects TS2488",
        },
        RecordedForm {
            stem: "array-rest-over-i32",
            body: "export function main(): void {\n  const [a, ...b] = 1;\n  print(`${a} ${b.length}`);\n}\n",
            claim: "rejects TS2488",
        },
        RecordedForm {
            stem: "nested-pattern-over-i32",
            body: "export function main(): void {\n  const [[a]] = 1;\n  print(`${a}`);\n}\n",
            claim: "rejects TS2488",
        },
        RecordedForm {
            stem: "default-value-over-i32",
            body: "export function main(): void {\n  const [a = 2] = 1;\n  print(`${a}`);\n}\n",
            claim: "rejects TS2488",
        },
        RecordedForm {
            stem: "field-rest-over-string",
            body: "export function main(): void {\n  const text: string = \"ab\";\n  const { length, ...rest } = text;\n  print(`${length}`);\n}\n",
            claim: "rejects TS2700",
        },
    ];
    let variants = [
        Divergence::PatternSourceShape,
        Divergence::PatternSourceShape,
        Divergence::PatternSourceShape,
        Divergence::PatternSourceShape,
        Divergence::ArrayRestPattern,
        Divergence::NestedPattern,
        Divergence::PatternDefaultValue,
        Divergence::ObjectRestPattern,
    ];

    let mut wrong_site = Vec::new();
    for (form, variant) in forms.iter().zip(variants) {
        let file = format!("{}.ts", form.stem);
        let diagnostics =
            subscript_compiler::check_program(&[subscript_compiler::SourceFile::new(
                &file,
                form.body.to_string(),
            )])
            .expect_err("every recorded form is rejected here");
        let sites: Vec<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.divergence)
            .collect();
        if sites != [Some(variant)] {
            wrong_site.push(format!(
                "{}: {sites:?}, wants [Some({variant:?})]",
                form.stem
            ));
        }
    }
    assert!(
        wrong_site.is_empty(),
        "a form reaches another site than the one it pins:\n{}",
        wrong_site.join("\n")
    );

    let result = recorded_form_disagreements("rejected-pattern-forms", &forms, str::to_string);
    assert!(result.is_ok(), "{}", result.unwrap_err());
}

/// compiler.md §108.1 rule 2 and §79 rule 6: the nested-assignment site
/// carries `Divergence::NestedFieldAssignment`, because a constructor
/// that assigns a field in both arms of a conditional is `tsc`-accepted
/// (`r227`). The same site rejects a constructor that assigns in one arm,
/// which `tsc` rejects, so that form reaches no corpus entry. This test is
/// its pin: the form is rejected here at the named variant, and the pinned
/// TypeScript compiler measures the class recorded beside it. The
/// both-arm form runs in the same batch as the firing control; the two
/// differ by the `else` arm alone.
#[test]
fn the_one_branch_field_assignment_form_measures_its_recorded_tsc_class() {
    use subscript_compiler::divergence::Divergence;

    let forms = [
        RecordedForm {
            stem: "one-branch-field-assignment",
            body: "class Holder {\n  value: i32;\n  constructor(flag: boolean) {\n    if (flag) {\n      this.value = 1;\n    }\n  }\n}\nexport function main(): void {\n  print(`${new Holder(true).value}`);\n}\n",
            claim: "rejects TS2564",
        },
        RecordedForm {
            stem: "both-branch-field-assignment",
            body: "class Holder {\n  value: i32;\n  constructor(flag: boolean) {\n    if (flag) {\n      this.value = 1;\n    } else {\n      this.value = 2;\n    }\n  }\n}\nexport function main(): void {\n  print(`${new Holder(true).value}`);\n}\n",
            claim: "accepts",
        },
    ];

    let mut wrong_site = Vec::new();
    for form in &forms {
        let file = format!("{}.ts", form.stem);
        let diagnostics =
            subscript_compiler::check_program(&[subscript_compiler::SourceFile::new(
                &file,
                form.body.to_string(),
            )])
            .expect_err("both recorded forms are rejected here");
        let sites: Vec<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.divergence)
            .collect();
        if sites != [Some(Divergence::NestedFieldAssignment)] {
            wrong_site.push(format!(
                "{}: {sites:?}, wants [Some(NestedFieldAssignment)]",
                form.stem
            ));
        }
    }
    assert!(
        wrong_site.is_empty(),
        "a form reaches another site than the one it pins:\n{}",
        wrong_site.join("\n")
    );

    let result = recorded_form_disagreements("field-assignment-forms", &forms, str::to_string);
    assert!(result.is_ok(), "{}", result.unwrap_err());
}

#[test]
fn tsc_claim_parser_requires_an_outcome_and_valid_diagnostic_codes() {
    assert_eq!(TscClaim::parse("accepts"), Ok(TscClaim::Accepts));
    assert_eq!(
        TscClaim::parse("rejects TS1238, TS2322"),
        Ok(TscClaim::Rejects(BTreeSet::from([
            "TS1238".to_string(),
            "TS2322".to_string()
        ])))
    );
    for invalid in ["", "clean", "rejects", "rejects S100", "rejects TS"] {
        assert!(TscClaim::parse(invalid).is_err(), "accepted `{invalid}`");
    }
}

#[test]
fn diagnostic_parser_attributes_codes_and_reports_unowned_errors() {
    let root = Path::new("/workspace");
    let entry = Entry {
        relative: "corpus/reject/r01.ts".to_string(),
        absolute: root.join("corpus/reject/r01.ts"),
        accept: false,
        external_module: true,
        claim: TscClaim::Accepts,
    };
    let measured = diagnostic_codes(
        root,
        &[entry],
        "/workspace/corpus/reject/r01.ts(2,3): error TS1234: bad\n",
    )
    .expect("diagnostic must belong to r01");
    assert_eq!(
        measured["corpus/reject/r01.ts"],
        BTreeSet::from(["TS1234".to_string()])
    );
    assert!(diagnostic_codes(root, &[], "error TS9999: global failure\n").is_err());
}
