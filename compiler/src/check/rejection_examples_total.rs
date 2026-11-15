//! Each displayed TypeScript example reaches its rejection site (§311 rule 4).
use super::*;
use crate::ambient::ApiRejectionId;
use crate::divergence::DivergenceEntry;

// §307 rule 5a: each new site-specific form b changes this review list.
pub(super) const FORM_B_SITES: &[RejectionSite] = &[
    RejectionSite::Api(
        ApiRejectionId::FormHypotWithMoreThanTwoArguments,
        Divergence::MathSubset,
    ),
    RejectionSite::InterfaceCallSignature,
    RejectionSite::Api(
        ApiRejectionId::RegexStringMatchAll,
        Divergence::RegExpSubset,
    ),
    RejectionSite::Api(
        ApiRejectionId::DateLocalGetTimezoneOffset,
        Divergence::DateSubset,
    ),
    RejectionSite::Api(
        ApiRejectionId::JsonStringifyFunction,
        Divergence::JsonSubset,
    ),
    RejectionSite::ContextValue,
    RejectionSite::NumberValue,
    RejectionSite::JsonValue,
    RejectionSite::DateValue,
    RejectionSite::MapSetValue,
    RejectionSite::ArrayMember,
    RejectionSite::MapSetMember,
    RejectionSite::NumberMember,
    RejectionSite::MathMember,
    RejectionSite::DateMember,
    RejectionSite::JsonStaticMember,
    RejectionSite::JsonParseDomain,
    RejectionSite::RegexMatchType,
    RejectionSite::ThisInStaticMethodArrow,
    RejectionSite::ClassFieldLayoutLimit,
    RejectionSite::SuspendFrameMemberLayoutLimit,
    RejectionSite::GeneratorFrameFinalAlignmentLimit,
    RejectionSite::ClosureEnvironmentLayoutLimit,
    RejectionSite::StoredAggregateLayoutLimit,
    RejectionSite::AsyncGeneratorMethod,
    RejectionSite::WireEnumValueRange,
    RejectionSite::VoidTypeOutsideResult,
    RejectionSite::UndefinedKeywordAnnotation,
    RejectionSite::MirrorNullableView,
    RejectionSite::DynamicEvaluatorValue,
    RejectionSite::ContextByteTargetLayout,
    RejectionSite::CoroutineStepLayoutLimit,
    RejectionSite::NamespacePrototypeRead,
    RejectionSite::GlobalPrototypeRead,
    RejectionSite::AggregateAnnotationLimit,
    RejectionSite::LocalAggregateFrameLimit,
    RejectionSite::ThisStaticField,
];

// §182 record spellings remain fixed when the example moves to its site.
fn fixed_record(site: RejectionSite) -> Option<&'static str> {
    match site {
        RejectionSite::ArrayElementDomain => Some("compiler.md §176"),
        RejectionSite::TaskGroupGeneratorBody => Some("collisions.md C24"),
        RejectionSite::DateMethodValue => Some("stdlib.md §3; collisions.md C24 row 12"),
        RejectionSite::ArrayMapFunctionResult => Some("stdlib.md §9"),
        RejectionSite::ArrayMapAsyncResult => Some("compiler.md §171"),
        RejectionSite::PromiseCombinatorCall => Some("collisions.md C8; compiler.md §186"),
        RejectionSite::PromiseCatchCall => Some("collisions.md C8; compiler.md §186"),
        RejectionSite::PromiseFinallyCall => Some("collisions.md C8; compiler.md §180, §186"),
        RejectionSite::AsyncArrowReceiverCapture => Some("collisions.md C24"),
        RejectionSite::ReactionReceiverCapture => Some("collisions.md C24; compiler.md §186"),
        RejectionSite::PromiseThenNullRejection => {
            Some("collisions.md C8; compiler.md §186 rule 2")
        }
        RejectionSite::PromiseReactionSpread => Some("collisions.md C8; compiler.md §186 rule 3"),
        RejectionSite::CallbackSetRepeatedValue => Some("stdlib.md §10.4; Q27"),
        _ => None,
    }
}

fn entries() -> Vec<(RejectionSite, Divergence, DivergenceEntry)> {
    all_sites()
        .into_iter()
        .filter_map(|site| {
            let variant = site.class().1.divergence()?;
            Some((
                site,
                variant,
                site.example().copied().unwrap_or_else(|| variant.entry()),
            ))
        })
        .collect()
}

pub(super) fn programs() -> Vec<rejection_programs::Program> {
    entries()
        .into_iter()
        .filter(|(site, _, _)| site.example().is_some())
        .flat_map(|(site, variant, entry)| {
            [("fragment", entry.ts)]
                .into_iter()
                .chain(
                    (!entry.subscript.starts_with("no equivalent: "))
                        .then_some(("alternative", entry.subscript)),
                )
                .map(move |(kind, source)| {
                    let mut files = fragment_files(variant, source);
                    if files.len() == 1 {
                        files[0].source = format!("export {{}};\n{}", files[0].source);
                    }
                    rejection_programs::Program {
                        key: format!("s311-{kind}-{site:?}.ts"),
                        codes: BTreeSet::new(),
                        files,
                        poison: Vec::new(),
                        runner: false,
                        enabled_modules: Vec::new(),
                    }
                })
        })
        .collect()
}

pub(super) fn shared_sites(variant: Divergence) -> Vec<RejectionSite> {
    entries()
        .into_iter()
        .filter_map(|(site, actual, _)| {
            (actual == variant && site.example().is_none()).then_some(site)
        })
        .collect()
}

pub(super) fn all_checker_failures() -> Vec<String> {
    checker_failures(
        entries()
            .into_iter()
            .filter(|(site, _, _)| site.example().is_some()),
    )
}

fn checker_failures(
    entries: impl IntoIterator<Item = (RejectionSite, Divergence, DivergenceEntry)>,
) -> Vec<String> {
    let started = Instant::now();
    let mut failures = Vec::new();
    for site in FORM_B_SITES {
        if !site
            .example()
            .is_some_and(|entry| entry.subscript.starts_with("no equivalent: "))
        {
            failures.push(format!("{site:?}: listed form b site has no form b entry"));
        }
    }
    for (site, variant, entry) in entries {
        if site.example().is_some() {
            let base = variant.entry();
            let record = fixed_record(site).unwrap_or(base.collision);
            if entry.collision != record {
                failures.push(format!("{site:?}: site entry changed its record"));
            }
            if entry.why.is_empty() {
                failures.push(format!("{site:?}: site entry has no reason"));
            }
            for source in [entry.ts, entry.subscript] {
                let lines = source
                    .lines()
                    .filter(|line| !line.trim_start().starts_with("//") && !line.trim().is_empty())
                    .count();
                let statements = if source.starts_with("no equivalent: ") {
                    0
                } else {
                    let files = fragment_files(variant, source);
                    crate::parse::parse_program(&files).map_or(0, |program| {
                        program
                            .files
                            .iter()
                            .map(|file| file.module.body.len())
                            .sum()
                    })
                };
                if lines.max(statements) > 3 {
                    failures.push(format!("{site:?}: site fragment has {lines} program lines and {statements} statements"));
                }
            }
        }
        failures.extend(fragment_site_checker_failures(
            Some(site),
            variant,
            entry.ts,
            entry.subscript,
        ));
    }
    eprintln!(
        "s311: site examples {:.3}s",
        started.elapsed().as_secs_f64()
    );
    failures
}

/// §311: a same-variant fragment must reach the specified site.
/// Measured cost with --test-threads=1: 0.006683s.
#[test]
fn another_site_with_the_same_variant_and_unlisted_form_b_fail() {
    let started = Instant::now();
    let variant = Divergence::VariadicArguments;
    let entry = variant.entry();
    let failures = fragment_site_checker_failures(
        Some(RejectionSite::CallSpread),
        variant,
        entry.ts,
        entry.subscript,
    );
    assert!(failures
        .iter()
        .any(|failure| failure.starts_with("CallSpread | ")));
    assert!(!failures
        .iter()
        .any(|failure| failure.contains("no rejection with its variant")));

    let ts = "function take(x: i32): void {} take(...[1]);";
    let failures = fragment_site_checker_failures(
        Some(RejectionSite::CallSpread),
        variant,
        ts,
        "no equivalent: Calls cannot receive arguments.",
    );
    assert!(failures
        .iter()
        .any(|failure| failure == "CallSpread: form b field disagrees with FORM_B_SITES"));
    let failures = fragment_site_checker_failures(
        Some(RejectionSite::ContextValue),
        Divergence::CompilerOwnedValue,
        "const held = Context;",
        "Context.collect();",
    );
    assert!(failures
        .iter()
        .any(|failure| failure == "ContextValue: form b field disagrees with FORM_B_SITES"));
    eprintln!("s311: controls {:.6}s", started.elapsed().as_secs_f64());
}

/// Rule 5 counts separate statements even when they occupy one physical line.
#[test]
fn long_site_entry_fails_through_checker_failures() {
    let variant = Divergence::VariadicArguments;
    let entry = DivergenceEntry {
        ts: "function take(x: i32): void {} const a = 1; const b = 2; take(...[a, b]);",
        subscript: "function take(x: i32): void {} take(1);",
        ..variant.entry()
    };
    let failures = checker_failures([(RejectionSite::CallSpread, variant, entry)]);
    assert!(
        failures
            .iter()
            .any(|failure| failure.contains("1 program lines and 4 statements")),
        "{failures:?}"
    );
}

/// The same variant on an earlier diagnostic does not prove that it shows this example.
#[test]
fn earlier_same_variant_with_a_different_example_fails() {
    let variant = Divergence::MapKeyKind;
    let entry = DivergenceEntry {
        ts: "const source = new Map<i32[], i32>(); new Map<i32[], i32>(source);",
        subscript: "const source = new Map<i32, i32>(); new Map<i32, i32>(source);",
        ..variant.entry()
    };
    let failures = checker_failures([(RejectionSite::MapCopyKey, variant, entry)]);
    // Check the supplied fragment, rather than comparing the production selector with itself.
    assert!(
        failures
            .iter()
            .any(|failure| failure.contains("example is not on the first diagnostic")),
        "{failures:?}"
    );
    assert!(
        !failures
            .iter()
            .any(|failure| failure.contains("no rejection with its variant")),
        "{failures:?}"
    );
}
