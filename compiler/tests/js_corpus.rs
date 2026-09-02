//! Stages 2 and 3 gate for compiler.md section 69: Node checks comparable
//! output, and the collision table indexes its corpus evidence.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

const HEADER_FIELD_PREFIX: &str = "js-comparable: ";
/// The `node` major line this gate runs on (compiler.md §69.3 rule 4).
/// The repository does not install `node`, so the pin is the major line: a
/// major release brings a new V8, and that is when a person re-measures the
/// record. A patch equality would report the host, not a divergence.
const NODE_MAJOR: &str = "v24";
/// The `node` version §69.2's record was measured on (§69.3 rule 6). A
/// failure on the major line names it, so a reader can tell a host mismatch
/// from a divergence. The summary prints the version that actually ran.
const NODE_RECORDED: &str = "v24.18.0";
/// The TypeScript version `package.json` and its lockfile install. This one
/// is exact, because the repository controls it: a mismatch is a stale
/// `node_modules` (§69.3 rule 4).
const TYPESCRIPT_VERSION: &str = "5.9.2";
// These entries retire in §82.1. The pinned table records them in prose.
const SECTION_82_1_RETIRED: &[&str] = &["r130", "r143", "r144"];

/// The major line of a `node` version string, `v24.18.0` to `v24`.
fn node_major(version: &str) -> &str {
    version.split_once('.').map_or(version, |(major, _)| major)
}

#[derive(Debug, Eq, PartialEq)]
enum JsClaim {
    Yes,
    No {
        ids: BTreeSet<String>,
        reason: String,
    },
}

impl JsClaim {
    fn parse(text: &str) -> Result<Self, String> {
        if text == "yes" {
            return Ok(Self::Yes);
        }
        let Some(text) = text.strip_prefix("no ") else {
            return Err("expected `yes` or `no <id>[ <id> ...]: <reason>`".to_string());
        };
        let Some((ids, reason)) = text.split_once(": ") else {
            return Err("expected `no <id>[ <id> ...]: <reason>`".to_string());
        };
        let ids: BTreeSet<String> = ids.split_whitespace().map(str::to_string).collect();
        if ids.is_empty() || ids.iter().any(|id| !is_collision_id(id)) || reason.trim().is_empty() {
            return Err("expected `no <id>[ <id> ...]: <reason>`".to_string());
        }
        Ok(Self::No {
            ids,
            reason: reason.to_string(),
        })
    }
}

fn is_collision_id(text: &str) -> bool {
    text.strip_prefix('C')
        .or_else(|| text.strip_prefix('Q'))
        .is_some_and(|digits| {
            !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
        })
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CorpusReference {
    name: String,
    line: usize,
}

#[derive(Debug, Default, Eq, PartialEq)]
struct CollisionRule {
    references: Vec<CorpusReference>,
    pins: BTreeSet<String>,
}

#[derive(Debug, Default, Eq, PartialEq)]
struct CollisionIndex {
    defined_ids: BTreeSet<String>,
    rules: BTreeMap<String, CollisionRule>,
    retired: BTreeSet<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ScannedCorpusReference {
    name: String,
    start: usize,
    end: usize,
    retired: bool,
}

fn collision_ids_in(text: &str) -> impl Iterator<Item = &str> {
    text.split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|word| is_collision_id(word))
}

fn scan_corpus_references(text: &str) -> Vec<ScannedCorpusReference> {
    let bytes = text.as_bytes();
    let mut references = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        let start = index;
        let (name_start, retired) = if bytes[index..].starts_with(b"retired:") {
            (index + "retired:".len(), true)
        } else {
            (index, false)
        };
        if name_start >= bytes.len() || !matches!(bytes[name_start], b'a' | b'r') {
            index += 1;
            continue;
        }
        if start > 0
            && matches!(bytes[start - 1], b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b'-')
        {
            index += 1;
            continue;
        }
        let mut end = name_start + 1;
        let digits_start = end;
        while end < bytes.len() && bytes[end].is_ascii_digit() {
            end += 1;
        }
        if end == digits_start {
            index += 1;
            continue;
        }
        if end < bytes.len() && bytes[end] == b'-' {
            end += 1;
            while end < bytes.len() && matches!(bytes[end], b'a'..=b'z' | b'0'..=b'9' | b'-') {
                end += 1;
            }
        }
        references.push(ScannedCorpusReference {
            name: text[name_start..end].to_string(),
            start,
            end,
            retired,
        });
        index = end;
    }

    let mut expanded = Vec::new();
    for pair in references.windows(2) {
        let [left, right] = pair else {
            unreachable!("a two-element window must contain two references")
        };
        let separator: String = text[left.end..right.start]
            .chars()
            .filter(|character| !character.is_whitespace() && *character != '`')
            .collect();
        if separator != "–" {
            continue;
        }
        let Some((left_kind, left_number)) = corpus_reference_number(&left.name) else {
            continue;
        };
        let Some((right_kind, right_number)) = corpus_reference_number(&right.name) else {
            continue;
        };
        if left_kind != right_kind || left_number >= right_number {
            continue;
        }
        let width = left.name.len() - 1;
        expanded.extend(
            (left_number + 1..right_number).map(|number| ScannedCorpusReference {
                name: format!("{left_kind}{number:0width$}"),
                start: left.start,
                end: right.end,
                retired: false,
            }),
        );
    }
    references.extend(expanded);
    references
}

fn corpus_reference_number(name: &str) -> Option<(char, u32)> {
    let mut characters = name.chars();
    let kind = characters.next()?;
    if !matches!(kind, 'a' | 'r') {
        return None;
    }
    let digits = characters.as_str();
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok().map(|number| (kind, number))
}

impl CollisionIndex {
    fn parse(source: &str) -> Result<Self, String> {
        let mut index = Self::default();
        for reference in scan_corpus_references(source) {
            if reference.retired {
                index.retired.insert(reference.name);
            }
        }

        let mut current_rule = None::<String>;
        let mut in_collision_rules = false;
        let mut in_q_register = false;
        let mut pin_paragraph = false;
        for (line_index, line) in source.lines().enumerate() {
            let line_number = line_index + 1;
            if line == "## 1. Collision rules" {
                in_collision_rules = true;
                in_q_register = false;
                continue;
            }
            if line.starts_with("## 2.") {
                in_collision_rules = false;
                in_q_register = true;
                current_rule = None;
            }
            if line.starts_with("## 3.") {
                in_q_register = false;
            }
            if let Some(heading) = line.strip_prefix("### ").filter(|_| in_collision_rules) {
                let Some(id) = heading.split('.').next().filter(|id| {
                    id.strip_prefix('C').is_some_and(|digits| {
                        !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
                    })
                }) else {
                    continue;
                };
                if index
                    .rules
                    .insert(id.to_string(), CollisionRule::default())
                    .is_some()
                {
                    return Err(format!(
                        "duplicate collision rule `{id}` at line {line_number}"
                    ));
                }
                index
                    .defined_ids
                    .extend(collision_ids_in(line).map(str::to_string));
                current_rule = Some(id.to_string());
                pin_paragraph = false;
                continue;
            }
            if in_q_register && line.starts_with("- **Q") {
                index
                    .defined_ids
                    .extend(collision_ids_in(line).map(str::to_string));
            }
            if line.trim().is_empty() {
                pin_paragraph = false;
                continue;
            }
            let Some(rule_id) = &current_rule else {
                continue;
            };
            if line.contains("Accept:") || line.contains("Reject:") {
                pin_paragraph = true;
            }
            let rule = index
                .rules
                .get_mut(rule_id)
                .expect("the active collision rule must exist");
            for reference in scan_corpus_references(line) {
                if pin_paragraph {
                    rule.pins.insert(reference.name.clone());
                }
                rule.references.push(CorpusReference {
                    name: reference.name,
                    line: line_number,
                });
            }
        }
        if index.rules.is_empty() {
            return Err("no `### C<id>.` collision rules found in section 1".to_string());
        }
        Ok(index)
    }

    fn consistency_errors(&self, corpus: &BTreeSet<String>) -> Vec<String> {
        let mut errors = Vec::new();
        for (id, rule) in &self.rules {
            for reference in &rule.references {
                if !self.retired.contains(&reference.name)
                    && !corpus_contains(corpus, &reference.name)
                {
                    errors.push(format!(
                        "{id} references absent corpus entry `{}` at collisions.md line {}",
                        reference.name, reference.line
                    ));
                }
            }
            if !rule
                .pins
                .iter()
                .any(|name| !self.retired.contains(name) && corpus_contains(corpus, name))
            {
                errors.push(format!(
                    "{id} pins no corpus entry through its `Accept:`/`Reject:` paragraph"
                ));
            }
        }
        for retired in &self.retired {
            if corpus_contains(corpus, retired) {
                errors.push(format!(
                    "collision table marks `{retired}` retired, but that corpus entry exists"
                ));
            }
        }
        errors
    }
}

fn corpus_contains(corpus: &BTreeSet<String>, reference: &str) -> bool {
    if corpus_reference_number(reference).is_some() {
        corpus
            .iter()
            .any(|name| name == reference || name.starts_with(&format!("{reference}-")))
    } else {
        corpus.contains(reference)
    }
}

#[derive(Debug)]
struct Entry {
    relative: String,
    absolute: PathBuf,
    golden: PathBuf,
    claim: JsClaim,
}

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler crate must have a workspace parent")
        .to_path_buf()
}

fn entries(root: &Path) -> Result<Vec<Entry>, String> {
    let directory = root.join("corpus/accept");
    let mut paths: Vec<PathBuf> = fs::read_dir(&directory)
        .map_err(|error| format!("read {}: {error}", directory.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && path.extension().is_some_and(|ext| ext == "ts"))
        .collect();
    paths.sort();
    if paths.len() != 175 {
        return Err(format!(
            "expected 175 top-level accept entries, found {}",
            paths.len()
        ));
    }

    paths
        .into_iter()
        .map(|absolute| {
            let relative = absolute
                .strip_prefix(root)
                .expect("corpus path must be below the workspace root")
                .to_string_lossy()
                .into_owned();
            let source = fs::read_to_string(&absolute)
                .map_err(|error| format!("read {relative}: {error}"))?;
            let headers: Vec<&str> = source
                .lines()
                .filter_map(|line| line.strip_prefix("// "))
                .flat_map(|line| line.split("; "))
                .filter_map(|field| field.strip_prefix(HEADER_FIELD_PREFIX))
                .collect();
            if headers.len() != 1 {
                return Err(format!(
                    "{relative}: expected one `{HEADER_FIELD_PREFIX}<claim>` header field, found {}",
                    headers.len()
                ));
            }
            let claim = JsClaim::parse(headers[0])
                .map_err(|error| format!("{relative}: invalid js-comparable header: {error}"))?;
            let golden = absolute.with_extension("expected");
            if !golden.is_file() {
                return Err(format!("{relative}: missing golden {}", golden.display()));
            }
            Ok(Entry {
                relative,
                absolute,
                golden,
                claim,
            })
        })
        .collect()
}

fn corpus_entry_names(root: &Path) -> Result<BTreeSet<String>, String> {
    let mut names = BTreeSet::new();
    for kind in ["accept", "reject"] {
        let directory = root.join("corpus").join(kind);
        for entry in fs::read_dir(&directory)
            .map_err(|error| format!("read {}: {error}", directory.display()))?
        {
            let path = entry
                .map_err(|error| format!("read {} entry: {error}", directory.display()))?
                .path();
            if !path.is_file() || !path.extension().is_some_and(|extension| extension == "ts") {
                continue;
            }
            let name = path
                .file_stem()
                .expect("a TypeScript corpus entry must have a file stem")
                .to_string_lossy()
                .into_owned();
            names.insert(name);
        }
    }
    Ok(names)
}

fn collision_index(root: &Path) -> Result<CollisionIndex, String> {
    let path = root.join("specs/blocks/collisions.md");
    let source =
        fs::read_to_string(&path).map_err(|error| format!("read {}: {error}", path.display()))?;
    let mut index = CollisionIndex::parse(&source)
        .map_err(|error| format!("parse {}: {error}", path.display()))?;
    index
        .retired
        .extend(SECTION_82_1_RETIRED.iter().map(|name| (*name).to_string()));
    Ok(index)
}

fn decode_hex(text: &str) -> Result<Vec<u8>, String> {
    if text.len() % 2 != 0 {
        return Err("hex field has an odd length".to_string());
    }
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let digits = std::str::from_utf8(pair).expect("hex digits are ASCII");
            u8::from_str_radix(digits, 16)
                .map_err(|error| format!("invalid hex `{digits}`: {error}"))
        })
        .collect()
}

fn prelude_has_global(prelude: &str, name: &str) -> bool {
    ["function", "namespace", "class", "interface"]
        .iter()
        .any(|kind| {
            let prefix = format!("declare {kind} {name}");
            prelude.lines().any(|line| {
                line.trim_start().strip_prefix(&prefix).is_some_and(|rest| {
                    rest.starts_with('(')
                        || rest.starts_with('<')
                        || rest.starts_with('{')
                        || rest.starts_with(char::is_whitespace)
                })
            })
        })
}

#[test]
fn every_accept_entry_has_a_total_js_claim_and_comparable_output_matches() {
    let root = project_root();
    let entries = entries(&root).unwrap_or_else(|error| panic!("{error}"));
    let defined_ids = collision_index(&root)
        .unwrap_or_else(|error| panic!("{error}"))
        .defined_ids;
    let mut comparable = Vec::new();
    let mut claim_errors = Vec::new();
    for entry in &entries {
        match &entry.claim {
            JsClaim::Yes => comparable.push(entry),
            JsClaim::No { ids, .. } => {
                for id in ids.difference(&defined_ids) {
                    claim_errors.push(format!(
                        "{}: collision id `{id}` is absent from specs/blocks/collisions.md",
                        entry.relative
                    ));
                }
            }
        }
    }
    assert!(
        claim_errors.is_empty(),
        "invalid js-comparable claim(s):\n{}",
        claim_errors.join("\n")
    );

    let runner = root.join("corpus/node/run-js-corpus.cjs");
    let started = Instant::now();
    let output = Command::new("node")
        .arg(&runner)
        .args(comparable.iter().map(|entry| &entry.absolute))
        .current_dir(&root)
        .output()
        .unwrap_or_else(|error| panic!("run node: {error}"));
    let elapsed = started.elapsed();
    assert!(
        output.status.success(),
        "node exited with {}:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("the runner protocol must be UTF-8");
    let mut lines = stdout.lines();
    let meta = lines.next().expect("the runner must emit metadata");
    let mut meta = meta.splitn(4, '\t');
    assert_eq!(meta.next(), Some("meta"), "invalid runner metadata");
    let node_version = meta.next().expect("runner metadata must name Node");
    assert_eq!(
        node_major(node_version),
        NODE_MAJOR,
        "the Node major line changed: this host runs {node_version} and the \
         §69.2 record was measured on {NODE_RECORDED}. A new major brings a \
         new V8, so re-measure the record rather than read past it \
         (compiler.md §69.3 rule 4)"
    );
    let typescript_version = meta.next().expect("runner metadata must name TypeScript");
    assert_eq!(
        typescript_version, TYPESCRIPT_VERSION,
        "the TypeScript pin changed"
    );
    let shim_names: Vec<&str> = meta
        .next()
        .expect("runner metadata must name the shim globals")
        .split(',')
        .filter(|name| !name.is_empty())
        .collect();
    let prelude =
        fs::read_to_string(root.join("prelude/lang.d.ts")).expect("read prelude/lang.d.ts");
    for name in &shim_names {
        assert!(
            prelude_has_global(&prelude, name),
            "the shim defines `{name}`, but prelude/lang.d.ts does not"
        );
    }

    let mut disagreements = Vec::new();
    let mut measured = 0;
    for (expected_index, line) in lines.enumerate() {
        measured += 1;
        let fields: Vec<&str> = line.splitn(4, '\t').collect();
        assert_eq!(fields.len(), 4, "invalid runner record `{line}`");
        assert_eq!(
            fields[0],
            expected_index.to_string(),
            "runner records are out of order"
        );
        let entry = comparable[expected_index];
        let actual = decode_hex(fields[2]).unwrap_or_else(|error| panic!("{line}: {error}"));
        let error = decode_hex(fields[3]).unwrap_or_else(|error| panic!("{line}: {error}"));
        if fields[1] != "ok" {
            disagreements.push(format!(
                "{}: node failed after stdout {:?}:\n{}",
                entry.relative,
                String::from_utf8_lossy(&actual),
                String::from_utf8_lossy(&error)
            ));
            continue;
        }
        let golden = fs::read(&entry.golden)
            .unwrap_or_else(|error| panic!("read {}: {error}", entry.golden.display()));
        if actual != golden {
            disagreements.push(format!(
                "{}: node output {:?} != golden {:?}",
                entry.relative,
                String::from_utf8_lossy(&actual),
                String::from_utf8_lossy(&golden)
            ));
        }
    }
    assert_eq!(measured, comparable.len(), "the runner omitted an entry");

    eprintln!(
        "js corpus gate: {} comparable, {} non-comparable, {} shim name(s), node {}, tsc {}, {:.3}s",
        comparable.len(),
        entries.len() - comparable.len(),
        shim_names.len(),
        node_version,
        typescript_version,
        elapsed.as_secs_f64()
    );
    assert!(
        disagreements.is_empty(),
        "node corpus disagreement(s):\n{}",
        disagreements.join("\n")
    );
}

#[test]
fn collision_table_is_a_consistent_corpus_index() {
    let started = Instant::now();
    let root = project_root();
    let index = collision_index(&root).unwrap_or_else(|error| panic!("{error}"));
    let corpus = corpus_entry_names(&root).unwrap_or_else(|error| panic!("{error}"));
    let errors = index.consistency_errors(&corpus);
    eprintln!(
        "collision index gate: {} C rules, {} corpus entries, {:.3}s",
        index.rules.len(),
        corpus.len(),
        started.elapsed().as_secs_f64()
    );
    assert!(
        errors.is_empty(),
        "invalid collision table index:\n{}",
        errors.join("\n")
    );
}

/// §69.3 rule 4 pins `node` to its major line, so this pins what "major
/// line" reads. A patch or minor release inside the line passes, and a new
/// major does not — which is the one case that asks a person to re-measure.
/// The recorded version stays exact, because §69.3 rule 6 names it in the
/// failure.
#[test]
fn the_node_pin_holds_the_major_line_and_not_the_patch() {
    assert_eq!(node_major("v24.18.0"), NODE_MAJOR);
    assert_eq!(node_major("v24.16.0"), NODE_MAJOR);
    assert_eq!(node_major("v24.0.0"), NODE_MAJOR);
    assert_ne!(node_major("v26.1.0"), NODE_MAJOR);
    assert_ne!(node_major("v22.20.0"), NODE_MAJOR);
    assert_eq!(node_major(NODE_RECORDED), NODE_MAJOR);
    // A malformed string keeps its own text, so it cannot match the line.
    assert_ne!(node_major("unknown"), NODE_MAJOR);
}

/// `package.json` and this gate state one `node` pin, so they must agree.
/// Two records that disagree are worse than one (§69.3 rule 4).
#[test]
fn the_package_manifest_states_the_same_node_line() {
    let manifest =
        fs::read_to_string(project_root().join("package.json")).expect("read package.json");
    let expected = format!("\"node\": \"{}.x\"", NODE_MAJOR.trim_start_matches('v'));
    assert!(
        manifest.contains(&expected),
        "package.json must declare engines.node as {expected}:\n{manifest}"
    );
    assert!(
        manifest.contains(&format!("\"typescript\": \"{TYPESCRIPT_VERSION}\"")),
        "package.json must install TypeScript {TYPESCRIPT_VERSION}:\n{manifest}"
    );
}

#[test]
fn js_claim_parser_has_only_two_states() {
    assert_eq!(JsClaim::parse("yes"), Ok(JsClaim::Yes));
    assert_eq!(
        JsClaim::parse("no C2 Q13: value and foreign semantics differ"),
        Ok(JsClaim::No {
            ids: BTreeSet::from(["C2".to_string(), "Q13".to_string()]),
            reason: "value and foreign semantics differ".to_string(),
        })
    );
    for invalid in ["", "unknown", "no", "no R1: reason", "no C2", "no C2: "] {
        assert!(JsClaim::parse(invalid).is_err(), "accepted `{invalid}`");
    }
}

#[test]
fn shim_names_must_exist_in_the_prelude() {
    assert!(prelude_has_global(
        "declare function print(value: string): void;",
        "print"
    ));
    assert!(!prelude_has_global(
        "declare function other(): void;",
        "print"
    ));
    assert!(!prelude_has_global(
        "declare function printExtra(): void;",
        "print"
    ));
}

#[test]
fn collision_index_expands_ranges_and_reads_q_definitions() {
    let source = "\
# Collision table

## 1. Collision rules

### C1. First rule (Q1)

Accept: `a01`–`a03`. Reject: `retired:r04-old`.

### C2. Second rule

Accept: `a05`.

## 2. Q-register resolutions not covered above

- **Q3/Q4 (forms)** — defined together.
";
    let index = CollisionIndex::parse(source).expect("parse collision index");
    assert_eq!(
        index.defined_ids,
        BTreeSet::from([
            "C1".to_string(),
            "C2".to_string(),
            "Q1".to_string(),
            "Q3".to_string(),
            "Q4".to_string(),
        ])
    );
    assert_eq!(
        index.rules["C1"].pins,
        BTreeSet::from([
            "a01".to_string(),
            "a02".to_string(),
            "a03".to_string(),
            "r04-old".to_string(),
        ])
    );
    assert_eq!(index.retired, BTreeSet::from(["r04-old".to_string()]));
}

#[test]
fn collision_index_reports_all_missing_unpinned_and_stale_records() {
    let source = "\
## 1. Collision rules

### C1. First rule

Accept: `a01`. Reject: `r02`, `retired:r03-old`.

### C2. Empty rule

This paragraph cites absent `r04`, but it is not an evidence paragraph.

## 2. Q-register resolutions not covered above
";
    let index = CollisionIndex::parse(source).expect("parse collision index");
    let corpus = BTreeSet::from(["a01-present".to_string(), "r03-old".to_string()]);
    assert_eq!(
        index.consistency_errors(&corpus),
        vec![
            "C1 references absent corpus entry `r02` at collisions.md line 5".to_string(),
            "C2 references absent corpus entry `r04` at collisions.md line 9".to_string(),
            "C2 pins no corpus entry through its `Accept:`/`Reject:` paragraph".to_string(),
            "collision table marks `r03-old` retired, but that corpus entry exists".to_string(),
        ]
    );
}
