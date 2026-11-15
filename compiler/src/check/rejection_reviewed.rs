//! Accepted witness sets that require a form review (§312 rule 3).
use super::*;

type Accepted = BTreeMap<String, BTreeSet<String>>;
type Indexed = Vec<(String, Vec<String>)>;

fn recorded_codes() -> BTreeMap<String, BTreeSet<String>> {
    witnesses()
        .into_iter()
        .map(|witness| (witness.file.to_owned(), witness.codes))
        .chain(
            rejection_programs::programs()
                .into_iter()
                .map(|program| (program.key, program.codes)),
        )
        .collect()
}

fn indexed_witnesses() -> Indexed {
    all_sites()
        .into_iter()
        .filter(|site| site.class().1.divergence().is_some())
        .map(|site| {
            (
                format!("{site:?}"),
                index::witness_entry(site)
                    .files()
                    .iter()
                    .map(|file| (*file).to_owned())
                    .collect(),
            )
        })
        .collect()
}

fn accepted_witnesses(indexed: Indexed, codes: &BTreeMap<String, BTreeSet<String>>) -> Accepted {
    indexed
        .into_iter()
        .map(|(site, files)| {
            let accepted = files
                .into_iter()
                .filter(|file| {
                    codes
                        .get(file)
                        .unwrap_or_else(|| panic!("{site}: no recorded tsc codes for {file}"))
                        .is_empty()
                })
                .collect();
            (site, accepted)
        })
        .collect()
}

fn reviewed_witnesses() -> Accepted {
    let mut reviewed = Accepted::new();
    for line in include_str!("rejection_reviewed.txt").lines() {
        let (site, files) = line.split_once('\t').unwrap();
        let files: BTreeSet<_> = files.split(',').map(str::to_owned).collect();
        assert!(
            files.len() >= 2,
            "{site}: review needs two accepted witnesses"
        );
        assert!(
            reviewed.insert(site.to_owned(), files).is_none(),
            "{site}: duplicate review"
        );
    }
    reviewed
}

fn review_failures(derived: &Accepted, reviewed: &Accepted) -> Vec<String> {
    let sites: BTreeSet<_> = derived
        .iter()
        .filter(|(_, files)| files.len() >= 2)
        .map(|(site, _)| site)
        .chain(reviewed.keys())
        .collect();
    sites
        .into_iter()
        .filter_map(|site| {
            let actual = derived.get(site).filter(|files| files.len() >= 2);
            let expected = reviewed.get(site);
            (actual != expected).then(|| {
                format!(
                    "{site}: accepted witnesses changed; derived {actual:?}; reviewed {expected:?}"
                )
            })
        })
        .collect()
}

/// Read recorded codes once; this check does not run tsc or the checker.
/// Measured cost with --test-threads=1: 0.007711s.
#[test]
fn every_multi_witness_site_has_the_reviewed_accepted_set() {
    let started = Instant::now();
    let codes = recorded_codes();
    let derived = accepted_witnesses(indexed_witnesses(), &codes);
    let failures = review_failures(&derived, &reviewed_witnesses());
    eprintln!(
        "s312: reviewed sets {:.6}s",
        started.elapsed().as_secs_f64()
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Change only test-local witness indexes; the review record stays fixed.
/// Measured cost with --test-threads=1: 0.008766s.
#[test]
fn extra_accepted_witness_and_second_accepted_witness_require_review() {
    let started = Instant::now();
    let codes = recorded_codes();
    let reviewed = reviewed_witnesses();
    let indexed = indexed_witnesses();
    for listed in [true, false] {
        let mut changed = indexed.clone();
        let (site, files) = changed
            .iter_mut()
            .find(|(site, files)| {
                let accepted = files.iter().filter(|file| codes[*file].is_empty()).count();
                if listed {
                    reviewed.contains_key(site)
                } else {
                    accepted == 1
                }
            })
            .unwrap();
        let extra = codes
            .iter()
            .find(|(file, codes)| codes.is_empty() && !files.contains(file))
            .map(|(file, _)| file.clone())
            .unwrap();
        files.push(extra);
        let expected_site = site.clone();
        let failures = review_failures(&accepted_witnesses(changed, &codes), &reviewed);
        assert_eq!(failures.len(), 1, "{failures:?}");
        assert!(
            failures[0].starts_with(&format!("{expected_site}:")),
            "{failures:?}"
        );
    }
    eprintln!(
        "s312: review controls {:.6}s",
        started.elapsed().as_secs_f64()
    );
}
