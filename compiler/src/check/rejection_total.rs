//! Total witness check for compiler.md §153.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

use super::rejection::RejectionSite;
use crate::diag::Pos;
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
    unreachable: &'a str,
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
                unreachable: fields[5],
                message,
                source,
            }
        })
        .collect()
}

fn checker_failures(witness: &Witness<'_>, site: RejectionSite) -> Vec<String> {
    if !witness.unreachable.is_empty() {
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
    if diagnostic.divergence != expected {
        failures.push(format!(
            "{}: {}: variant {:?}, expected {:?}",
            witness.target, witness.file, diagnostic.divergence, expected
        ));
    }
    if witness.codes.is_empty() && expected.is_none() {
        failures.push(format!(
            "{}: accepted witness has no variant",
            witness.target
        ));
    }
    let rendered = crate::render_diagnostics(&files, std::slice::from_ref(diagnostic));
    if rendered.contains("= TypeScript accepts:") != expected.is_some() {
        failures.push(format!("{}: divergence block absent", witness.target));
    }
    failures
}

/// One tsc process measures all witnesses; the checker runs once per program.
/// This test costs 0.44 seconds: tsc 0.33 seconds and checker 0.07 seconds.
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
    let mut identities = BTreeSet::new();
    let mut paths = vec![root.join("prelude/lang.d.ts")];
    for site in all_sites() {
        let (key, variant) = index::witness_key(site);
        let primary = witnesses
            .iter()
            .find(|w| w.file == key)
            .expect("each target has a witness");
        let target_witnesses: Vec<_> = witnesses
            .iter()
            .filter(|w| w.target == primary.target)
            .collect();
        let reachable = primary.unreachable.is_empty();
        let accepted = target_witnesses.iter().any(|w| w.codes.is_empty());
        if !identities.insert(key) {
            failures.push(format!("{site:?}: duplicate witness identity {key}"));
        }
        if variant.is_some() != (reachable && accepted) {
            failures.push(format!("{site:?}: witness class disagrees with variant"));
        }
        if site.divergence() != variant {
            failures.push(format!(
                "{site:?}: production variant disagrees with witness table"
            ));
        }
    }
    // The rewritten reject entries join this same tsc process.
    for file in [
        "r27-string-match",
        "r78-call-spread-variadic",
        "r201-new-class-spread-variadic",
    ] {
        let path = temporary.join(format!("{file}-rewritten.ts"));
        fs::copy(root.join(format!("corpus/reject/{file}.ts")), &path).unwrap();
        paths.push(path);
    }
    for variant in index::NEW_VARIANTS {
        let path = temporary.join(format!("fragment-{variant:?}.ts"));
        let source = format!("export {{}};\n{}", variant.entry().ts);
        fs::write(&path, source).unwrap();
        paths.push(path);
    }
    for witness in &witnesses {
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
        if codes != witness.codes {
            failures.push(format!(
                "{}: tsc {:?}, expected {:?}",
                witness.file, codes, witness.codes
            ));
        }
    }
    assert!(measured.is_empty(), "unowned tsc errors: {measured:?}");
    assert!(
        result.status.code() == Some(0) || result.status.code() == Some(2),
        "tsc failed: {output}"
    );
    let checker_started = Instant::now();
    let targets: BTreeMap<_, _> = all_sites()
        .into_iter()
        .map(|site| {
            let key = index::witness_key(site).0;
            let target = witnesses.iter().find(|w| w.file == key).unwrap().target;
            (target, site)
        })
        .collect();
    for witness in &witnesses {
        failures.extend(checker_failures(witness, targets[witness.target]));
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
    let mut witness = witnesses().remove(0);
    witness.message = "deliberately wrong target message";
    assert!(checker_failures(&witness, all_sites()[0])
        .iter()
        .any(|failure| failure.contains("target message absent")));
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
