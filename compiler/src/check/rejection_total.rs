//! Total witness check for compiler.md §153.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

use super::rejection::RejectionSite;
use crate::diag::{Diagnostic, Pos};
use crate::divergence::Divergence;
use crate::{check_program, SourceFile};

#[path = "rejection_witness_index.rs"]
mod index;

thread_local! {
    static REACHED: std::cell::RefCell<Vec<(RejectionSite, String, Pos)>> = const { std::cell::RefCell::new(Vec::new()) };
}

pub(super) fn record_site(site: RejectionSite, message: &str, pos: &Pos) {
    REACHED.with_borrow_mut(|reached| reached.push((site, message.to_owned(), pos.clone())));
}

fn all_sites() -> Vec<RejectionSite> {
    crate::ambient::rejected_api()
        .into_iter()
        .map(RejectionSite::Api)
        .chain(index::DIRECT_SITES.iter().copied())
        .collect()
}

#[path = "../../tests/support/tsc.rs"]
mod tsc;

struct Witness<'a> {
    file: &'a str,
    target: &'a str,
    codes: BTreeSet<String>,
    code: &'a str,
    mirror: &'a str,
    message: &'a str,
    source: &'a str,
}

fn witnesses() -> Vec<Witness<'static>> {
    include_str!("rejection_witnesses.txt")
        .split("===\n")
        .filter(|record| !record.is_empty())
        .map(|record| {
            let (header, body) = record.split_once('\n').unwrap();
            let fields: Vec<_> = header.split('\t').collect();
            let (message, source) = body.split_once('\n').unwrap();
            Witness {
                file: fields[0],
                target: fields[1],
                codes: fields[2]
                    .split(',')
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .collect(),
                code: fields[3],
                mirror: fields[4],
                message,
                source,
            }
        })
        .collect()
}

fn checker_failures(witness: &Witness<'_>, site: RejectionSite) -> Vec<String> {
    if matches!(
        index::witness_entry(site),
        index::WitnessEntry::Unreachable { .. }
    ) {
        return Vec::new();
    }
    let mut files = Vec::new();
    if !witness.mirror.is_empty() {
        let mirrors: Vec<_> = include_str!("rejection_mirrors.txt")
            .split("===\n")
            .collect();
        files.push(SourceFile::ambient(
            witness.mirror,
            mirrors[usize::from(witness.mirror.contains("-b"))],
        ));
    }
    files.push(SourceFile::entry(witness.file, witness.source));
    REACHED.with_borrow_mut(Vec::clear);
    let diagnostics = check_program(&files).err().unwrap_or_default();
    let reached = REACHED.with_borrow_mut(std::mem::take);
    let Some(diagnostic) = diagnostics
        .iter()
        .find(|d| d.code.as_str() == witness.code && d.message == witness.message)
    else {
        return vec![format!(
            "{}: target message absent: {}",
            witness.target, witness.message
        )];
    };
    let expected = index::witness_key(site).1;
    let mut failures = Vec::new();
    if !reached.iter().any(|(actual, message, pos)| {
        *actual == site && message == &diagnostic.message && pos == &diagnostic.pos
    }) {
        failures.push(format!(
            "{}: {}: named target absent",
            witness.target, witness.file
        ));
    }
    failures.extend(diagnostic_failures(
        witness.target,
        diagnostic,
        expected,
        &files,
    ));
    failures
}

fn diagnostic_failures(
    target: &str,
    diagnostic: &Diagnostic,
    expected: Divergence,
    files: &[SourceFile],
) -> Vec<String> {
    let mut failures = Vec::new();
    if diagnostic.divergence != Some(expected) {
        failures.push(format!(
            "{target}: variant {:?}, expected {:?}",
            diagnostic.divergence, expected
        ));
    }
    let rendered = crate::render_diagnostics(files, std::slice::from_ref(diagnostic));
    if !rendered.contains("= TypeScript accepts:") {
        failures.push(format!("{target}: divergence block absent"));
    }
    failures
}

fn tsc_class_failure(witness: &Witness<'_>, codes: &BTreeSet<String>) -> Option<String> {
    (codes != &witness.codes).then(|| {
        format!(
            "{}: tsc {:?}, expected {:?}",
            witness.file, codes, witness.codes
        )
    })
}

fn table_failures(witnesses: &[Witness<'_>], sites: &[RejectionSite]) -> Vec<String> {
    let mut failures = Vec::new();
    let mut identities = BTreeSet::new();
    for witness in witnesses {
        if !identities.insert(witness.file) {
            failures.push(format!(
                "{}: duplicate witness identity {}",
                witness.target, witness.file
            ));
        }
    }
    let listed: BTreeSet<_> = sites
        .iter()
        .flat_map(|site| index::witness_entry(*site).files().iter().copied())
        .collect();
    for witness in witnesses {
        if !listed.contains(witness.file) {
            failures.push(format!(
                "{}: witness has no named target {}",
                witness.target, witness.file
            ));
        }
    }
    for site in sites {
        let entry = index::witness_entry(*site);
        let (_, variant) = entry.key();
        for key in entry.files() {
            if !witnesses.iter().any(|w| w.file == *key) {
                failures.push(format!("{site:?}: named target has no witness {key}"));
            }
        }
        if site.divergence() != variant {
            failures.push(format!(
                "{site:?}: production variant disagrees with witness table"
            ));
        }
    }
    failures
}

fn class_controls() -> [Witness<'static>; 2] {
    [
        Witness {
            file: "control-accepted.ts",
            target: "control",
            codes: BTreeSet::new(),
            code: "",
            mirror: "",
            message: "",
            source: "export {}; const value: string = 1;",
        },
        Witness {
            file: "control-rejected.ts",
            target: "control",
            codes: BTreeSet::from(["TS2322".to_owned()]),
            code: "",
            mirror: "",
            message: "",
            source: "export {}; const value: string = \"x\";",
        },
    ]
}

/// One tsc process measures all witnesses; the checker runs once per program.
/// This test costs 0.38 seconds: tsc 0.28 seconds and checker 0.05 seconds.
/// The batch proves both TypeScript classes without a second corpus or tier run.
#[test]
fn every_subset_rejection_carries_its_divergence() {
    let started = Instant::now();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let temporary = std::env::temp_dir().join(format!("subscript-s153-{}", std::process::id()));
    fs::create_dir_all(&temporary).unwrap();
    let witnesses = witnesses();
    let mut failures = Vec::new();
    failures.extend(table_failures(&witnesses, &all_sites()));
    let mut paths = vec![root.join("prelude/lang.d.ts")];
    for variant in index::NEW_VARIANTS {
        let path = temporary.join(format!("fragment-{variant:?}.ts"));
        let source = format!("export {{}};\n{}", variant.entry().ts);
        fs::write(&path, source).unwrap();
        paths.push(path);
    }
    let controls = class_controls();
    for witness in witnesses.iter().chain(&controls) {
        let path = temporary.join(witness.file);
        fs::write(&path, witness.source).unwrap();
        paths.push(path);
    }
    let mirrors: Vec<_> = include_str!("rejection_mirrors.txt")
        .split("===\n")
        .collect();
    for (file, source) in ["mirror-pattern.d.ts", "mirror-pattern-b.d.ts"]
        .into_iter()
        .zip(mirrors)
    {
        let path = temporary.join(file);
        fs::write(&path, source).unwrap();
        paths.push(path);
    }
    let config = temporary.join("tsconfig.json");
    fs::write(&config, tsc::tsconfig(&paths)).unwrap();
    let tsc_started = Instant::now();
    let result = Command::new(tsc::tsc_binary(&root))
        .args(["--project", config.to_str().unwrap(), "--pretty", "false"])
        .current_dir(&root)
        .output()
        .unwrap();
    let tsc_cost = tsc_started.elapsed();
    let output = String::from_utf8(result.stdout).unwrap();
    let mut measured = BTreeMap::<String, BTreeSet<String>>::new();
    for line in output.lines().filter(|line| line.contains("error TS")) {
        let (position, diagnostic) = line.split_once("): error ").unwrap();
        let file = position.rsplit_once('(').unwrap().0;
        let name = std::path::Path::new(file)
            .file_name()
            .unwrap()
            .to_str()
            .unwrap();
        let code = diagnostic.split_once(':').unwrap().0;
        measured
            .entry(name.to_owned())
            .or_default()
            .insert(code.to_owned());
    }
    for witness in &witnesses {
        let mut codes = measured.remove(witness.file).unwrap_or_default();
        if !witness.mirror.is_empty() {
            codes.extend(measured.remove(witness.mirror).unwrap_or_default());
        }
        if let Some(failure) = tsc_class_failure(witness, &codes) {
            failures.push(failure);
        }
    }
    // Construct both false class claims; the same tsc batch measures their sources.
    for control in &controls {
        let codes = measured.remove(control.file).unwrap_or_default();
        assert!(tsc_class_failure(control, &codes).is_some());
    }
    assert!(measured.is_empty(), "unowned tsc errors: {measured:?}");
    assert!(
        result.status.code() == Some(0) || result.status.code() == Some(2),
        "tsc failed: {output}"
    );
    let checker_started = Instant::now();
    for site in all_sites() {
        for file in index::witness_entry(site).files() {
            if let Some(witness) = witnesses.iter().find(|w| w.file == *file) {
                failures.extend(checker_failures(witness, site));
            }
        }
    }
    eprintln!(
        "s153: {} witnesses; tsc {:.3}s; checker {:.3}s; total {:.3}s",
        witnesses.len(),
        tsc_cost.as_secs_f64(),
        checker_started.elapsed().as_secs_f64(),
        started.elapsed().as_secs_f64()
    );
    let targets: BTreeSet<_> = failures
        .iter()
        .filter_map(|s| s.split_once(": ").map(|p| p.0))
        .collect();
    fs::remove_dir_all(temporary).unwrap();
    assert!(
        failures.is_empty(),
        "{} targets fail:\n{}",
        targets.len(),
        failures.join("\n")
    );
}

#[test]
fn wrong_message_expectation_fires() {
    let witness = Witness {
        source: "export function main(): void {}",
        ..witnesses().remove(0)
    };
    assert!(checker_failures(&witness, all_sites()[0])
        .iter()
        .any(|failure| failure.contains("target message absent")));
}

#[test]
fn variant_mismatch_fires() {
    let witness = witnesses().remove(0);
    let files = [SourceFile::entry(witness.file, witness.source)];
    let diagnostics = check_program(&files).unwrap_err();
    let entry = index::WitnessEntry::Reachable {
        files: &["a001.ts"],
        variant: Divergence::DateSubset,
    };
    let failures = diagnostic_failures(witness.target, &diagnostics[0], entry.key().1, &files);
    assert!(failures.iter().any(|failure| failure.contains("variant")));
}

#[test]
fn absent_block_fires() {
    let files = [SourceFile::entry(
        "control.ts",
        "export function main(): void { missing(); }",
    )];
    let diagnostics = check_program(&files).unwrap_err();
    let entry = index::WitnessEntry::Reachable {
        files: &["control.ts"],
        variant: Divergence::MathSubset,
    };
    assert!(
        diagnostic_failures("control", &diagnostics[0], entry.key().1, &files)
            .iter()
            .any(|failure| failure.contains("block absent"))
    );
}

#[test]
fn named_target_absent_fires() {
    let witness = witnesses().remove(0);
    assert!(checker_failures(&witness, all_sites()[1])
        .iter()
        .any(|failure| failure.contains("named target absent")));
}

#[test]
fn duplicate_witness_fires() {
    let witnesses = vec![witnesses().remove(0), witnesses().remove(0)];
    assert!(table_failures(&witnesses, &[])
        .iter()
        .any(|failure| failure.contains("duplicate witness")));
}

#[test]
fn named_target_without_witness_fires() {
    let witnesses = vec![witnesses().remove(1)];
    assert!(table_failures(&witnesses, &all_sites()[..1])
        .iter()
        .any(|failure| failure.contains("named target has no witness")));
}

#[test]
fn witness_without_named_target_fires() {
    let witness = Witness {
        file: "unlisted.ts",
        ..witnesses().remove(0)
    };
    assert!(table_failures(&[witness], &all_sites()[..1])
        .iter()
        .any(|failure| failure.contains("witness has no named target")));
}

#[test]
fn production_variant_mismatch_fires() {
    let row = crate::ambient::ApiRejection {
        divergence: Divergence::DateSubset,
        ..crate::ambient::rejected_api()[0]
    };
    assert!(table_failures(&witnesses(), &[RejectionSite::Api(row)])
        .iter()
        .any(|failure| failure.contains("production variant disagrees")));
}

#[test]
fn empty_unreachable_reason_fires() {
    let entry = index::WitnessEntry::Unreachable {
        files: &["control.ts"],
        variant: Divergence::JsonTypeDomain,
        reason: "",
    };
    assert!(std::panic::catch_unwind(|| entry.key()).is_err());
}

#[test]
fn new_subscript_fragments_are_accepted() {
    let mut failures = Vec::new();
    for variant in index::NEW_VARIANTS {
        let fragment = variant.entry().subscript;
        let files = if *variant == crate::divergence::Divergence::MirrorParameterPattern {
            vec![
                SourceFile::ambient(
                    "fragment.d.ts",
                    format!("// @subscript-c-header include=\"fragment.h\"\n{fragment}"),
                ),
                SourceFile::entry("main.ts", "export function main(): void {}"),
            ]
        } else {
            vec![SourceFile::entry(
                "main.ts",
                format!("export function main(): void {{ {fragment} }}"),
            )]
        };
        if let Err(diagnostics) = check_program(&files) {
            failures.push(format!(
                "{variant:?}: {}",
                crate::render_diagnostics(&files, &diagnostics)
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn s014_has_one_checker_constructor() {
    fn visit(path: &std::path::Path, failures: &mut Vec<String>) {
        for entry in fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(&path, failures);
            } else if path.extension().is_some_and(|extension| extension == "rs")
                && path.file_name().unwrap() != "rejection.rs"
                && !["rejection_total.rs", "rejection_witness_index.rs"]
                    .contains(&path.file_name().unwrap().to_str().unwrap())
            {
                let source = fs::read_to_string(&path).unwrap();
                // Checker files put inline tests after their production items.
                let production = source.split("#[cfg(test)]\nmod tests").next().unwrap();
                if production.contains("RuleCode::S014") {
                    failures.push(path.display().to_string());
                }
            }
        }
    }
    let mut failures = Vec::new();
    visit(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/check"),
        &mut failures,
    );
    assert!(
        failures.is_empty(),
        "S014 constructors outside rejection.rs: {failures:?}"
    );
    let source = include_str!("rejection.rs");
    let body = source
        .split("pub(super) enum RejectionSite {")
        .nth(1)
        .unwrap()
        .split("\n}")
        .next()
        .unwrap();
    let declared: BTreeSet<_> = body
        .lines()
        .map(str::trim)
        .filter(|line| line.ends_with(',') && !line.starts_with("Api("))
        .map(|line| line.trim_end_matches(',').to_owned())
        .collect();
    let listed: BTreeSet<_> = index::DIRECT_SITES
        .iter()
        .map(|site| format!("{site:?}"))
        .collect();
    assert_eq!(
        declared, listed,
        "every direct site must enter the total check"
    );
}
