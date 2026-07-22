//! The standing differential gate (`specs/blocks/compiler.md` §2, §8.3).
//!
//! For every run-set entry with a committed golden:
//! **dev-JIT bytes ≡ AOT bytes ≡ golden bytes**, byte-exact, with no
//! normalization. The entry set is derived from `corpus/accept/`, so a
//! new entry or golden is picked up with no edit here.
//!
//! There are no skips. A missing host C compiler, a missing runtime
//! static library, or a failing link fails this test: the gate machine
//! is the development machine (§8.3).

mod corpus;

use subscript_codegen::{run_aot, run_jit};

#[test]
fn jit_aot_and_golden_agree_byte_for_byte() {
    let accept = corpus::corpus_accept();
    let golden_ids = corpus::golden_ids(&accept);
    // The set is derived, never pinned: new entries are compared with
    // no edit here. The floor is the committed count, so deleting a
    // golden fails this test instead of silently shrinking the gate
    // (compiler.md §2 — goldens are never deleted).
    assert!(
        golden_ids.len() >= 24,
        "expected at least the 24 committed goldens, found {}",
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
        let aot = match run_aot(&sources) {
            Ok(bytes) => bytes,
            Err(e) => {
                failures.push(format!("{id}: AOT run failed: {e}"));
                continue;
            }
        };
        compared += 1;
        if jit != aot {
            failures.push(format!(
                "{id}: dev-JIT output {:?} != AOT output {:?}",
                String::from_utf8_lossy(&jit),
                String::from_utf8_lossy(&aot)
            ));
        }
        if jit != golden {
            failures.push(format!(
                "{id}: dev-JIT output {:?} != golden {:?}",
                String::from_utf8_lossy(&jit),
                String::from_utf8_lossy(&golden)
            ));
        }
        if aot != golden {
            failures.push(format!(
                "{id}: AOT output {:?} != golden {:?}",
                String::from_utf8_lossy(&aot),
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
