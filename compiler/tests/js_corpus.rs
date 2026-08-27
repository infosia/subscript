//! Stage 2 gate for compiler.md section 69: Node runs each comparable
//! accept entry and its stdout must equal the committed golden.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

const HEADER_FIELD_PREFIX: &str = "js-comparable: ";
const NODE_VERSION: &str = "v24.18.0";
const TYPESCRIPT_VERSION: &str = "5.9.2";

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
    if paths.len() != 154 {
        return Err(format!(
            "expected 154 top-level accept entries, found {}",
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

fn collision_ids(root: &Path) -> Result<BTreeSet<String>, String> {
    let path = root.join("specs/blocks/collisions.md");
    let source =
        fs::read_to_string(&path).map_err(|error| format!("read {}: {error}", path.display()))?;
    Ok(source
        .lines()
        .filter(|line| line.starts_with("### C") || line.starts_with("- **Q"))
        .flat_map(|line| {
            line.split(|character: char| !character.is_ascii_alphanumeric())
                .filter(|word| is_collision_id(word))
                .map(str::to_string)
        })
        .collect())
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
    let defined_ids = collision_ids(&root).unwrap_or_else(|error| panic!("{error}"));
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
    assert_eq!(node_version, NODE_VERSION, "the Node pin changed");
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
