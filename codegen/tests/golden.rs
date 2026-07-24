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
fn jit_ship_c_aot_and_golden_agree_byte_for_byte() {
    let accept = corpus::corpus_accept();
    let golden_ids = corpus::golden_ids(&accept);
    // The set is derived, never pinned: new entries are compared with
    // no edit here. The floor is the committed count, so deleting a
    // golden fails this test instead of silently shrinking the gate
    // (compiler.md §2 — goldens are never deleted). The floor is the
    // run set (a01–a24) plus the P5 interop entries (a25–a31).
    assert!(
        golden_ids.len() >= 34,
        "expected at least the 34 committed goldens (a01–a24 run set + a25–a34 interop), found {}",
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
