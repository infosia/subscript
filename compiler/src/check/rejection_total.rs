//! Total witness check for compiler.md §153 and §154.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

use super::rejection::{RejectionClass, RejectionSite};
use super::rejection_programs;
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
        .map(|row| RejectionSite::Api(row.id, row.divergence))
        .chain(index::DIRECT_SITES.iter().copied())
        .chain(index::GENERAL_SITES.iter().copied())
        .collect()
}

fn carried_variants() -> Vec<Divergence> {
    let mut variants = Vec::new();
    for site in all_sites() {
        let Some(variant) = site.class().1.divergence() else {
            continue;
        };
        if !variants.contains(&variant) {
            variants.push(variant);
        }
    }
    variants
}

fn fragment_files(variant: Divergence, source: &str) -> Vec<SourceFile> {
    if source.starts_with("// file: ") {
        return source
            .split("// file: ")
            .skip(1)
            .map(|part| {
                let (name, body) = part.split_once('\n').unwrap();
                if name.ends_with(".d.ts") {
                    SourceFile::ambient(name, body)
                } else if name == "main.ts" {
                    SourceFile::entry(name, body)
                } else {
                    SourceFile::new(name, body)
                }
            })
            .collect();
    }
    if variant == Divergence::MirrorParameterPattern {
        vec![
            SourceFile::ambient(
                "fragment.d.ts",
                format!("// @subscript-c-header include=\"fragment.h\"\n{source}"),
            ),
            SourceFile::entry("main.ts", "export function main(): void {}"),
        ]
    } else {
        vec![SourceFile::entry("fragment.ts", source)]
    }
}

fn fragment_checker_failures(variant: Divergence, ts: &str, subscript: &str) -> Vec<String> {
    let mut failures = Vec::new();
    let files = fragment_files(variant, ts);
    // The discovery guard reads the absent-module option, not only source syntax.
    let options = crate::CheckOptions {
        poison_missing_modules: if matches!(
            variant,
            Divergence::PoisonedDefaultImport | Divergence::NamespaceImportTargetMissingForm
        ) {
            vec![
                "s154_fragment_poison_default".to_owned(),
                "./missing".to_owned(),
            ]
        } else {
            Vec::new()
        },
        ..crate::CheckOptions::default()
    };
    let diagnostics = match crate::check_program_with(&files, &options) {
        Ok(module) if variant == Divergence::RunnerMainMissing => {
            module.runner_main().err().into_iter().collect()
        }
        Ok(_) => Vec::new(),
        Err(diagnostics) => diagnostics,
    };
    if !diagnostics
        .iter()
        .any(|diagnostic| diagnostic.divergence == Some(variant))
    {
        failures.push(format!(
            "{variant:?}: ts fragment has no rejection with its variant: {diagnostics:?}"
        ));
    }
    if !subscript.starts_with("no equivalent; ") {
        let files = fragment_files(variant, subscript);
        if let Err(diagnostics) = check_program(&files) {
            failures.push(format!(
                "{variant:?}: subscript fragment rejected: {}",
                crate::render_diagnostics(&files, &diagnostics)
            ));
        }
    }
    failures
}

fn fragment_tsc_failure(name: &str, codes: &BTreeSet<String>) -> Option<String> {
    (!codes.is_empty()).then(|| format!("{name}: ts fragment rejected by tsc: {codes:?}"))
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
    expected: Option<Divergence>,
    files: &[SourceFile],
) -> Vec<String> {
    let mut failures = Vec::new();
    if diagnostic.divergence != expected {
        failures.push(format!(
            "{target}: variant {:?}, expected {:?}",
            diagnostic.divergence, expected
        ));
    }
    let rendered = crate::render_diagnostics(files, std::slice::from_ref(diagnostic));
    if expected.is_some() && !rendered.contains("= TypeScript accepts:") {
        failures.push(format!("{target}: divergence block absent"));
    }
    if expected.is_none() && rendered.contains("= TypeScript accepts:") {
        failures.push(format!("{target}: TscRejects diagnostic renders a block"));
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
    let programs = rejection_programs::programs();
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
            if !witnesses.iter().any(|w| w.file == *key) && !programs.iter().any(|p| p.key == *key)
            {
                failures.push(format!("{site:?}: named target has no witness {key}"));
            }
        }
        if site.class().1.divergence() != variant {
            failures.push(format!(
                "{site:?}: production variant disagrees with witness table"
            ));
        }
    }
    failures
}

fn class_controls() -> [Witness<'static>; 3] {
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
        Witness {
            file: "control-tsc-rejects.ts",
            target: "control",
            codes: BTreeSet::new(),
            code: "S011",
            mirror: "",
            message: "",
            source: "export {}; const value:i32|null=1;",
        },
    ]
}

/// One tsc process measures all witnesses and every carried TypeScript fragment.
/// Warm cargo test reports `finished in 1.54s`: tsc 0.573s and checker 0.459s.
/// The batch proves both classes and fragment truth without a second tsc run.
#[test]
fn every_subset_rejection_carries_its_divergence() {
    let started = Instant::now();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let _pinned_tsc = tsc::tsc_binary(&root);
    let temporary = std::env::temp_dir().join(format!("subscript-s153-{}", std::process::id()));
    fs::create_dir_all(&temporary).unwrap();
    let witnesses = witnesses();
    let programs = rejection_programs::programs();
    let mut failures = Vec::new();
    failures.extend(table_failures(&witnesses, &all_sites()));
    let mut paths = vec![root.join("prelude/lang.d.ts")];
    for variant in carried_variants() {
        let mut files = fragment_files(variant, variant.entry().ts);
        // Each single-file snippet is an isolated TypeScript module.
        if files.len() == 1 {
            files[0].source = format!("export {{}};\n{}", files[0].source);
        }
        let program = rejection_programs::Program {
            key: format!("fragment-{variant:?}.ts"),
            codes: BTreeSet::new(),
            files,
            poison: Vec::new(),
            runner: false,
        };
        paths.extend(rejection_programs::write_tsc(&program, &temporary));
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
    for program in &programs {
        paths.extend(rejection_programs::write_tsc(program, &temporary));
    }
    let script = temporary.join("tsc-batch.cjs");
    fs::write(&script, include_str!("rejection_tsc.cjs")).unwrap();
    let config = temporary.join("tsconfig.json");
    fs::write(&config, tsc::tsconfig(&paths)).unwrap();
    let tsc_started = Instant::now();
    let result = Command::new("node")
        .arg(&script)
        .arg(root.join("node_modules/typescript"))
        .arg(&config)
        .current_dir(&root)
        .output()
        .unwrap();
    let tsc_cost = tsc_started.elapsed();
    let output = String::from_utf8(result.stdout).unwrap();
    assert!(
        result.status.success(),
        "tsc failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let mut measured = BTreeMap::<String, BTreeSet<String>>::new();
    for line in output.lines() {
        let (file, code) = line.split_once('\t').unwrap();
        measured
            .entry(file.to_owned())
            .or_default()
            .insert(code.to_owned());
    }
    for program in &programs {
        let codes = measured
            .remove(&format!("@{}", program.key))
            .unwrap_or_default();
        if codes != program.codes {
            failures.push(format!(
                "{}: tsc {:?}, expected {:?}",
                program.key, codes, program.codes
            ));
        }
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
        if control.file == "control-tsc-rejects.ts" {
            assert!(tsc_class_failure(control, &codes).is_none());
            assert!(site_class_failure(
                "control",
                RejectionSite::ClassUndeclaredMemberRead.class().1,
                &codes
            )
            .is_some());
            continue;
        }
        assert!(tsc_class_failure(control, &codes).is_some());
        if control.file == "control-accepted.ts" {
            assert!(fragment_tsc_failure(control.file, &codes).is_some());
        }
    }
    for variant in carried_variants() {
        let name = format!("fragment-{variant:?}.ts");
        let codes = measured.remove(&format!("@{name}")).unwrap_or_default();
        if let Some(failure) = fragment_tsc_failure(&name, &codes) {
            failures.push(failure);
        }
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
    failures.extend(general_checker_failures(&programs));
    for variant in carried_variants() {
        let entry = variant.entry();
        failures.extend(fragment_checker_failures(
            variant,
            entry.ts,
            entry.subscript,
        ));
    }
    eprintln!(
        "s154: {} witnesses; {} variants; tsc {:.3}s; checker {:.3}s; total {:.3}s",
        witnesses.len() + programs.len(),
        carried_variants().len(),
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
    let mut diagnostic = diagnostics[0].clone();
    diagnostic.divergence = Some(Divergence::DateSubset);
    let failures = diagnostic_failures(
        witness.target,
        &diagnostic,
        index::witness_key(all_sites()[0]).1,
        &files,
    );
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
    assert!(
        table_failures(&witnesses(), &[RejectionSite::Api(row.id, row.divergence)])
            .iter()
            .any(|failure| failure.contains("production variant disagrees"))
    );
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
fn accepted_ts_fragment_fires() {
    let variant = Divergence::CompilerOwnedValue;
    assert!(
        fragment_checker_failures(variant, "const value: i32 = 1;", variant.entry().subscript)
            .iter()
            .any(|failure| failure.contains("ts fragment has no rejection"))
    );
}

#[test]
fn rejected_subscript_fragment_fires() {
    let variant = Divergence::CompilerOwnedValue;
    assert!(
        fragment_checker_failures(variant, variant.entry().ts, "const held = Array;")
            .iter()
            .any(|failure| failure.contains("subscript fragment rejected"))
    );
}

fn outside_site_map(source: &str) -> bool {
    let production = source.split("#[cfg(test)]\nmod tests").next().unwrap();
    production.contains("RuleCode::")
}

#[test]
fn outside_code_literal_fires() {
    assert!(outside_site_map("fn reject() { error(RuleCode::S100); }"));
    assert!(outside_site_map("fn reject() { error(RuleCode::S001); }"));
}

#[test]
fn every_code_has_one_site_map() {
    fn visit(path: &std::path::Path, failures: &mut Vec<String>) {
        for entry in fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(&path, failures);
            } else if path.extension().is_some_and(|extension| extension == "rs")
                && !["rejection.rs", "rejection_total.rs"]
                    .contains(&path.file_name().unwrap().to_str().unwrap())
                && outside_site_map(&fs::read_to_string(&path).unwrap())
            {
                failures.push(path.display().to_string());
            }
        }
    }
    let source_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut failures = Vec::new();
    visit(&source_root.join("check"), &mut failures);
    for file in [
        "parse.rs",
        "provenance.rs",
        "regex.rs",
        "lib.rs",
        "ambient.rs",
        "hir/host_entry.rs",
    ] {
        if outside_site_map(&fs::read_to_string(source_root.join(file)).unwrap()) {
            failures.push(file.to_owned());
        }
    }
    assert!(
        failures.is_empty(),
        "code literals outside rejection.rs: {failures:?}"
    );
    let body = include_str!("rejection_sites.rs")
        .split("pub(crate) enum RejectionSite {")
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
        .chain(index::GENERAL_SITES)
        .map(|site| format!("{site:?}"))
        .collect();
    assert_eq!(
        declared, listed,
        "every direct site must enter the total check"
    );
}

#[test]
fn tsc_rejects_diagnostic_with_block_fires() {
    let files = [SourceFile::entry(
        "control.ts",
        "export {}; const value:i32=1;",
    )];
    let diagnostic = Diagnostic {
        divergence: Some(Divergence::CompilerOwnedValue),
        ..super::rejection::diagnostic(
            RejectionSite::NullableMember,
            "control",
            Pos::new("control.ts", 1, 1),
        )
    };
    let failures = diagnostic_failures("control", &diagnostic, None, &files);
    assert!(failures
        .iter()
        .any(|failure| failure.contains("TscRejects diagnostic renders a block")));
}

pub(super) fn clear_reached() {
    REACHED.with_borrow_mut(Vec::clear);
}

pub(super) fn take_reached() -> Vec<(RejectionSite, String, Pos)> {
    REACHED.with_borrow_mut(std::mem::take)
}

fn general_checker_failures(programs: &[rejection_programs::Program]) -> Vec<String> {
    let targets: Vec<_> = include_str!("rejection_targets.txt")
        .lines()
        .map(|line| {
            let fields: Vec<_> = line.splitn(4, '\t').collect();
            (fields[0], fields[1], fields[2], fields[3])
        })
        .collect();
    let mut failures = target_site_failures(targets.iter().map(|row| row.0));
    for program in programs {
        clear_reached();
        let diagnostics = rejection_programs::check(program);
        let reached = take_reached();
        if let Some(failure) = accepted_first_diagnostic_failure(program, &diagnostics) {
            failures.push(failure);
        }
        for site in index::GENERAL_SITES.iter().copied().filter(|site| {
            index::witness_entry(*site)
                .files()
                .contains(&program.key.as_str())
        }) {
            let name = format!("{site:?}");
            let expected = index::witness_key(site).1;
            let Some((_, _, code, message)) = targets
                .iter()
                .find(|(target, key, _, _)| *target == name && *key == program.key)
            else {
                failures.push(format!(
                    "{name}: {}: target expectation absent",
                    program.key
                ));
                continue;
            };
            let Some(diagnostic) = diagnostics
                .iter()
                .filter(|d| d.code.as_str() == *code && d.message == *message)
                .find(|d| {
                    reached.iter().any(|(actual, message, pos)| {
                        *actual == site && message == &d.message && pos == &d.pos
                    })
                })
                .or_else(|| {
                    diagnostics
                        .iter()
                        .find(|d| d.code.as_str() == *code && d.message == *message)
                })
            else {
                failures.push(format!(
                    "{name}: {}: target message absent: {message}",
                    program.key
                ));
                continue;
            };
            if !reached.iter().any(|(actual, message, pos)| {
                *actual == site && *message == diagnostic.message && *pos == diagnostic.pos
            }) {
                failures.push(format!("{name}: {}: named target absent", program.key));
            }
            if tsc_rejects_target_is_follow_on(
                site.class().1,
                &diagnostics,
                diagnostic,
                &program.files,
            ) {
                failures.push(format!(
                    "{name}: {}: TscRejects witness first diagnostic is elsewhere: {}",
                    program.key,
                    source_first_diagnostic(&diagnostics, &program.files)
                        .unwrap()
                        .message
                ));
            }
            failures.extend(diagnostic_failures(
                &name,
                diagnostic,
                expected,
                &program.files,
            ));
            if let Some(failure) = site_class_failure(&name, site.class().1, &program.codes) {
                failures.push(failure);
            }
        }
    }
    for site in index::GENERAL_SITES {
        let entry = index::witness_entry(*site);
        if site.class().0.as_str() != "S014"
            && entry.key().1.is_some()
            && !entry.files().is_empty()
            && !entry
                .files()
                .iter()
                .any(|key| programs.iter().any(|p| p.key == *key && p.codes.is_empty()))
        {
            failures.push(format!(
                "{site:?}: Diverges site has no tsc-accepted witness"
            ));
        }
    }
    failures
}

fn site_class_failure(
    target: &str,
    class: RejectionClass,
    codes: &BTreeSet<String>,
) -> Option<String> {
    (class == RejectionClass::TscRejects && codes.is_empty())
        .then(|| format!("{target}: TscRejects witness is accepted by tsc"))
}

#[test]
fn rejection_names_state_guards_without_source_line_numbers() {
    let has_number = |name: &str| {
        name.as_bytes()
            .windows(2)
            .any(|part| part[0].is_ascii_alphabetic() && part[1].is_ascii_digit())
    };
    for site in all_sites() {
        if matches!(site, RejectionSite::Api(..)) {
            continue;
        }
        assert!(!has_number(&format!("{site:?}")), "{site:?}");
    }
    for variant in Divergence::ALL {
        assert!(!has_number(&format!("{variant:?}")), "{variant:?}");
    }
}

fn tsc_rejects_target_is_follow_on(
    class: RejectionClass,
    diagnostics: &[Diagnostic],
    target: &Diagnostic,
    files: &[SourceFile],
) -> bool {
    class == RejectionClass::TscRejects
        && source_first_diagnostic(diagnostics, files).is_some_and(|first| first != target)
}

#[test]
fn tsc_rejects_follow_on_witness_fires() {
    let first = super::rejection::diagnostic(
        RejectionSite::UnknownClassConstructor,
        "unknown class",
        Pos::new("main.ts", 1, 1),
    );
    let later = super::rejection::diagnostic(
        RejectionSite::NullableMember,
        "nullable",
        Pos::new("main.ts", 2, 1),
    );
    let diagnostics = vec![later.clone(), first.clone()];
    assert!(tsc_rejects_target_is_follow_on(
        RejectionClass::TscRejects,
        &diagnostics,
        &later,
        &[],
    ));
    assert!(!tsc_rejects_target_is_follow_on(
        RejectionClass::TscRejects,
        &diagnostics,
        &first,
        &[],
    ));
    assert!(!tsc_rejects_target_is_follow_on(
        RejectionClass::Diverges(Divergence::NullableMemberNonNullFlow),
        &diagnostics,
        &later,
        &[],
    ));
}

fn source_first_diagnostic<'a>(
    diagnostics: &'a [Diagnostic],
    files: &[SourceFile],
) -> Option<&'a Diagnostic> {
    diagnostics.iter().min_by_key(|diagnostic| {
        (
            files
                .iter()
                .position(|file| file.name == diagnostic.pos.file)
                .unwrap_or(usize::MAX),
            diagnostic.pos.line,
            diagnostic.pos.col,
        )
    })
}

fn target_site_failures<'a>(targets: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let sites: BTreeSet<_> = all_sites().iter().map(|site| format!("{site:?}")).collect();
    targets
        .into_iter()
        .filter(|target| !sites.contains(*target))
        .map(|target| format!("{target}: target row names no site"))
        .collect()
}

#[test]
fn target_row_without_site_fires() {
    assert_eq!(
        target_site_failures(["UnknownTarget"]),
        ["UnknownTarget: target row names no site"]
    );
    assert!(target_site_failures(["RestParameter"]).is_empty());
}

fn accepted_first_diagnostic_failure(
    program: &rejection_programs::Program,
    diagnostics: &[Diagnostic],
) -> Option<String> {
    if !program.codes.is_empty() {
        return None;
    }
    let first = source_first_diagnostic(diagnostics, &program.files)?;
    first.divergence.is_none().then(|| {
        format!(
            "first diagnostic: {}: tsc accepts but the first diagnostic has no block: {}",
            program.key, first.message
        )
    })
}

#[test]
fn accepted_first_diagnostic_without_block_fires_in_source_order() {
    let program = rejection_programs::Program {
        key: "control".to_owned(),
        codes: BTreeSet::new(),
        files: vec![SourceFile::entry("main.ts", "")],
        poison: vec![],
        runner: false,
    };
    let first = super::rejection::diagnostic(
        RejectionSite::NullableMember,
        "earlier nullable read",
        Pos::new("main.ts", 1, 1),
    );
    let later = super::rejection::diagnostic(
        RejectionSite::RestParameter,
        "later rest parameter",
        Pos::new("main.ts", 2, 1),
    );
    let failure = accepted_first_diagnostic_failure(&program, &[later.clone(), first]).unwrap();
    assert!(failure.ends_with("earlier nullable read"));
    assert!(accepted_first_diagnostic_failure(&program, &[later]).is_none());
}
