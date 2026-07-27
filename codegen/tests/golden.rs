//! The standing differential gate (`specs/blocks/compiler.md` §2, §11).
//!
//! For every run-set entry with a committed golden:
//! **dev-JIT bytes ≡ ship-C-AOT bytes ≡ golden bytes**, byte-exact, with
//! no normalization. The ship tier is C emission compiled by the
//! platform C compiler and linked with the runtime static library
//! (§11, plan §8 Rev 2); this is where dev/ship agreement is now
//! established — by verification, since the two tiers are separate
//! lowerings. The entry set is derived from `corpus/accept/`, so a new
//! entry or golden is picked up with no edit here.
//!
//! The `cranelift-object` AOT path is retained only as an **optional
//! extra cross-check** column (its ship role has ended, §11): it is
//! compared when it is available, but the gate does not require it.
//!
//! There are no skips of the ship tier. A missing host C compiler, a
//! missing runtime static library, or a failing compile/link fails this
//! test: the gate machine is the development machine (§8.3).

mod corpus;

use subscript_codegen::{run_aot, run_c_aot, run_jit};

#[test]
fn unicode_string_entry_matches_across_tiers_before_golden_comparison() {
    let accept = corpus::corpus_accept();
    let id = "a60-string-unicode";
    let sources = corpus::entry_sources(&accept, id);
    let jit = run_jit(&sources).unwrap_or_else(|e| panic!("{id}: dev-JIT run failed: {e}"));
    let ship = run_c_aot(&sources).unwrap_or_else(|e| panic!("{id}: ship-C-AOT run failed: {e}"));
    assert_eq!(
        jit,
        ship,
        "{id}: dev-JIT output {:?} != ship-C-AOT output {:?}",
        String::from_utf8_lossy(&jit),
        String::from_utf8_lossy(&ship)
    );
    println!("{id}: {:?}", String::from_utf8_lossy(&jit));
}

#[test]
fn narrow_corpus_entries_match_across_tiers_before_golden_comparison() {
    let accept = corpus::corpus_accept();
    for id in [
        "a46-narrow-numerics",
        "a47-narrow-layout",
        "a48-interop-narrow-slices",
        "a49-f16-conversions",
        "a50-narrow-callbacks-shifts",
    ] {
        let sources = corpus::entry_sources(&accept, id);
        let jit = run_jit(&sources).unwrap_or_else(|e| panic!("{id}: dev-JIT run failed: {e}"));
        let ship =
            run_c_aot(&sources).unwrap_or_else(|e| panic!("{id}: ship-C-AOT run failed: {e}"));
        assert_eq!(
            jit,
            ship,
            "{id}: dev-JIT output {:?} != ship-C-AOT output {:?}",
            String::from_utf8_lossy(&jit),
            String::from_utf8_lossy(&ship)
        );
        println!("{id}: {:?}", String::from_utf8_lossy(&jit));
    }
}

#[test]
fn jit_ship_c_aot_and_golden_agree_byte_for_byte() {
    let accept = corpus::corpus_accept();
    let golden_ids = corpus::golden_ids(&accept);
    // The set is derived, never pinned: new entries are compared with
    // no edit here. The floor is the committed count, so deleting a
    // golden fails this test instead of silently shrinking the gate
    // (compiler.md §2 — goldens are never deleted). The floor is the
    // run set (a01–a24) plus the interop entries (a25–a34 P5/P6.2, a35
    // P6.3 async, a36–a38 P7.1 async/Future shapes, a39 P7.2 composed
    // async capstone) plus the stdlib entries (a40 Math battery, a41
    // Math.random sequence, a42 Date battery, a43 P10 String battery,
    // a44/a45 P11 Array batteries), P14/review narrow numerics
    // (a46–a50), and P15 Map/Set plus aggregate-callback coverage
    // (a51–a56), P12 Number/parsing/toFixed (a57–a59), and Unicode
    // String case/trim coverage (a60), Q22/Q24 SameValueZero (a61),
    // Q26 Number formatting/clz32 (a62), and all six Q27 stages:
    // Math/Number, String, Array, Map/Set, and callback indices
    // (a63–a67), plus the FixedArray callback family (a68), and P13
    // JSON.stringify (a69), JSON.parse (a70–a72), the P19 review
    // divisor single-evaluation probe (a73), and the P20 compound-array
    // fixes (a74–a76), and P22 for-of/container iteration/array spread
    // coverage (a77–a81).
    assert_eq!(
        golden_ids.len(),
        if cfg!(feature = "regex") { 82 } else { 81 },
        "expected exactly the 81 standing committed goldens (a01–a24 run set + a25–a39 interop \
         + a40–a45 stdlib + a46–a50 narrow numerics + a51–a56 Map/Set \
         + a57–a59 Number + a60 Unicode String + a61 SameValueZero \
         + a62 Q26 Number formatting/clz32 + a63–a68 Q27 stages 1–6 \
         + a69 P13 JSON.stringify + a70–a72 P13 JSON.parse \
         + a73 P19 divisor single-evaluation + a74–a76 P20 review fixes \
         + a77–a81 P22 for-of/container iteration/array spread), plus the \
         feature-on a82 P23 regex golden, found {}",
        golden_ids.len()
    );

    let mut failures = Vec::new();
    let mut compared = 0usize;
    for id in &golden_ids {
        let golden = corpus::golden_bytes(&accept, id);
        let sources = corpus::entry_sources(&accept, id);
        let jit = match run_jit(&sources) {
            Ok(bytes) => bytes,
            Err(e) => {
                failures.push(format!("{id}: dev-JIT run failed: {e}"));
                continue;
            }
        };
        // The ship tier: emit C, compile at -O2 -ffp-contract=off, link
        // with the runtime, run, capture stdout.
        let ship = match run_c_aot(&sources) {
            Ok(bytes) => bytes,
            Err(e) => {
                failures.push(format!("{id}: ship-C-AOT run failed: {e}"));
                continue;
            }
        };
        compared += 1;
        if jit != ship {
            failures.push(format!(
                "{id}: dev-JIT output {:?} != ship-C-AOT output {:?}",
                String::from_utf8_lossy(&jit),
                String::from_utf8_lossy(&ship)
            ));
        }
        if jit != golden {
            failures.push(format!(
                "{id}: dev-JIT output {:?} != golden {:?}",
                String::from_utf8_lossy(&jit),
                String::from_utf8_lossy(&golden)
            ));
        }
        if ship != golden {
            failures.push(format!(
                "{id}: ship-C-AOT output {:?} != golden {:?}",
                String::from_utf8_lossy(&ship),
                String::from_utf8_lossy(&golden)
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} differential failure(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert_eq!(
        compared,
        golden_ids.len(),
        "every committed golden must be compared on both tiers (no silent skips)"
    );
}

/// Optional cross-check: the retired `cranelift-object` AOT path still
/// reproduces the goldens byte-for-byte (§11 keeps it as an extra
/// column, not a requirement). It shares the dev tier's lowering, so it
/// is a cheap independent confirmation that the goldens are stable.
#[test]
fn cranelift_object_aot_still_matches_the_goldens_cross_check() {
    let accept = corpus::corpus_accept();
    let mut failures = Vec::new();
    for id in corpus::golden_ids(&accept) {
        let golden = corpus::golden_bytes(&accept, &id);
        let sources = corpus::entry_sources(&accept, &id);
        match run_aot(&sources) {
            Ok(bytes) if bytes == golden => {}
            Ok(bytes) => failures.push(format!(
                "{id}: cranelift-AOT output {:?} != golden {:?}",
                String::from_utf8_lossy(&bytes),
                String::from_utf8_lossy(&golden)
            )),
            Err(e) => failures.push(format!("{id}: cranelift-AOT run failed: {e}")),
        }
    }
    assert!(
        failures.is_empty(),
        "{} cranelift cross-check failure(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn every_corpus_entry_with_a_golden_ends_in_a_newline() {
    // Output shape is part of the corpus convention: every run-set
    // program prints at least one line.
    let accept = corpus::corpus_accept();
    for id in corpus::golden_ids(&accept) {
        let golden = corpus::golden_bytes(&accept, &id);
        assert!(!golden.is_empty(), "{id}: golden is empty");
        assert_eq!(golden.last(), Some(&b'\n'), "{id}: golden has no final newline");
    }
}
