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
// The fixture is excluded on windows-msvc (compiler.md §11c), and no interop
// corpus entry is compiled or run there, so this module and its symbols are
// gated out under the same predicate.
#[cfg(not(all(windows, target_env = "msvc")))]
#[path = "support/native_fixture.rs"]
mod native_fixture;

use subscript_codegen::{
    run_aot_with_native_libraries, run_c_aot_with_native_libraries,
    run_jit_with_native_libraries, NativeLibrary,
};

#[cfg(not(all(windows, target_env = "msvc")))]
fn native_libraries(sources: &[subscript_compiler::SourceFile]) -> Vec<NativeLibrary> {
    if sources
        .iter()
        .any(|source| corpus::references_interop(&source.source))
    {
        vec![native_fixture::library()]
    } else {
        Vec::new()
    }
}

// On windows-msvc the interop fixture is excluded and every interop corpus
// entry is filtered out before it is run, so no entry ever needs a native
// library.
#[cfg(all(windows, target_env = "msvc"))]
fn native_libraries(_sources: &[subscript_compiler::SourceFile]) -> Vec<NativeLibrary> {
    Vec::new()
}

#[test]
fn unicode_string_entry_matches_across_tiers_before_golden_comparison() {
    let accept = corpus::corpus_accept();
    let id = "a60-string-unicode";
    let sources = corpus::entry_sources(&accept, id);
    let libraries = native_libraries(&sources);
    let jit = run_jit_with_native_libraries(&sources, &libraries)
        .unwrap_or_else(|e| panic!("{id}: dev-JIT run failed: {e}"));
    let ship = run_c_aot_with_native_libraries(&sources, &libraries)
        .unwrap_or_else(|e| panic!("{id}: ship-C-AOT run failed: {e}"));
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
fn string_literal_union_entry_matches_across_tiers_and_golden() {
    let accept = corpus::corpus_accept();
    let id = "a91-string-literal-union";
    let sources = corpus::entry_sources(&accept, id);
    let jit = run_jit_with_native_libraries(&sources, &[])
        .unwrap_or_else(|error| panic!("{id}: dev-JIT run failed: {error}"));
    let ship = run_c_aot_with_native_libraries(&sources, &[])
        .unwrap_or_else(|error| panic!("{id}: ship-C-AOT run failed: {error}"));
    let golden = corpus::golden_bytes(&accept, id);
    assert_eq!(jit, ship, "{id}: tier outputs differ");
    assert_eq!(jit, golden, "{id}: captured golden differs");
    println!("dev-JIT:\n{}", String::from_utf8_lossy(&jit));
    println!("ship-C-AOT:\n{}", String::from_utf8_lossy(&ship));
}

#[test]
fn descriptor_literal_entry_matches_across_tiers_and_golden() {
    let accept = corpus::corpus_accept();
    let id = "a92-descriptor-literals";
    let sources = corpus::entry_sources(&accept, id);
    let jit = run_jit_with_native_libraries(&sources, &[])
        .unwrap_or_else(|error| panic!("{id}: dev-JIT run failed: {error}"));
    let ship = run_c_aot_with_native_libraries(&sources, &[])
        .unwrap_or_else(|error| panic!("{id}: ship-C-AOT run failed: {error}"));
    let golden = corpus::golden_bytes(&accept, id);
    assert_eq!(jit, ship, "{id}: tier outputs differ");
    assert_eq!(jit, golden, "{id}: captured golden differs");
    println!("dev-JIT:\n{}", String::from_utf8_lossy(&jit));
    println!("ship-C-AOT:\n{}", String::from_utf8_lossy(&ship));
}

#[test]
fn q34_async_entries_match_across_tiers_and_golden() {
    let accept = corpus::corpus_accept();
    for id in [
        "a93-async-chain",
        "a94-async-two-roots",
        "a95-interop-async-await",
    ] {
        let sources = corpus::entry_sources(&accept, id);
        let libraries = native_libraries(&sources);
        let jit = run_jit_with_native_libraries(&sources, &libraries)
            .unwrap_or_else(|error| panic!("{id}: dev-JIT run failed: {error}"));
        let ship = run_c_aot_with_native_libraries(&sources, &libraries)
            .unwrap_or_else(|error| panic!("{id}: ship-C-AOT run failed: {error}"));
        let object = run_aot_with_native_libraries(&sources, &libraries)
            .unwrap_or_else(|error| panic!("{id}: object-AOT run failed: {error}"));
        let golden = corpus::golden_bytes(&accept, id);
        assert_eq!(jit, ship, "{id}: tier outputs differ");
        assert_eq!(jit, object, "{id}: generated AOT entry output differs");
        assert_eq!(jit, golden, "{id}: captured golden differs");
        println!("{id}:\n{}", String::from_utf8_lossy(&jit));
    }
}

#[test]
fn scalar_parameter_pair_entry_matches_across_tiers_and_golden() {
    let accept = corpus::corpus_accept();
    let id = "a96-interop-byte-pairs";
    let sources = corpus::entry_sources(&accept, id);
    let libraries = native_libraries(&sources);
    let jit = run_jit_with_native_libraries(&sources, &libraries)
        .unwrap_or_else(|error| panic!("{id}: dev-JIT run failed: {error}"));
    let ship = run_c_aot_with_native_libraries(&sources, &libraries)
        .unwrap_or_else(|error| panic!("{id}: ship-C-AOT run failed: {error}"));
    let golden = corpus::golden_bytes(&accept, id);
    assert_eq!(jit, ship, "{id}: tier outputs differ");
    assert_eq!(jit, golden, "{id}: captured golden differs");
    println!("dev-JIT:\n{}", String::from_utf8_lossy(&jit));
    println!("ship-C-AOT:\n{}", String::from_utf8_lossy(&ship));
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
        // On windows-msvc the interop fixture is excluded, so the interop
        // narrow entry (a48) is not compiled or run there.
        #[cfg(all(windows, target_env = "msvc"))]
        if sources
            .iter()
            .any(|source| corpus::references_interop(&source.source))
        {
            continue;
        }
        let libraries = native_libraries(&sources);
        let jit = run_jit_with_native_libraries(&sources, &libraries)
            .unwrap_or_else(|e| panic!("{id}: dev-JIT run failed: {e}"));
        let ship = run_c_aot_with_native_libraries(&sources, &libraries)
            .unwrap_or_else(|e| panic!("{id}: ship-C-AOT run failed: {e}"));
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
    // coverage (a77–a81), P23 RegExp (a82–a83), and P24 code-point
    // storage coverage (a84–a87), plus the P24 astral-intern collection
    // coverage (a88), the P25 embedded chain-payload read-back
    // coverage (a89), callback-userdata collection rooting (a90), and
    // Q32 string-literal union aliases (a91), Q33 descriptor literals
    // (a92), Q34 poll-driven async (a93–a95), and R5 scalar parameter
    // array-pairs (a96).
    assert_eq!(
        golden_ids.len(),
        96,
        "expected exactly 96 committed goldens: the 81 standing goldens (a01–a24 run set + a25–a39 interop \
         + a40–a45 stdlib + a46–a50 narrow numerics + a51–a56 Map/Set \
         + a57–a59 Number + a60 Unicode String + a61 SameValueZero \
         + a62 Q26 Number formatting/clz32 + a63–a68 Q27 stages 1–6 \
         + a69 P13 JSON.stringify + a70–a72 P13 JSON.parse \
         + a73 P19 divisor single-evaluation + a74–a76 P20 review fixes \
         + a77–a81 P22 for-of/container iteration/array spread), plus the \
         a82–a83 P23 regex, a84–a87 P24 code-point, and a88 P24 \
         astral-intern collection, a89 P25 embedded chain-payload read-back, \
         a90 callback-userdata rooting, and a91 Q32 string-literal-union \
         a92 Q33 descriptor-literal, a93–a95 Q34 async goldens, and a96 R5 \
         scalar parameter-pair interop, found {}",
        golden_ids.len()
    );

    // The count guard above checks the full committed set on every host
    // (goldens are never deleted). On windows-msvc the interop fixture is
    // excluded, so interop entries are filtered out of the run set here — no
    // interop program is compiled or run there — while every other golden is
    // still compared on both tiers.
    #[cfg(all(windows, target_env = "msvc"))]
    let golden_ids: Vec<String> = golden_ids
        .into_iter()
        .filter(|id| {
            !corpus::entry_sources(&accept, id)
                .iter()
                .any(|source| corpus::references_interop(&source.source))
        })
        .collect();

    let mut failures = Vec::new();
    let mut compared = 0usize;
    for id in &golden_ids {
        let golden = corpus::golden_bytes(&accept, id);
        let sources = corpus::entry_sources(&accept, id);
        let libraries = native_libraries(&sources);
        let jit = match run_jit_with_native_libraries(&sources, &libraries) {
            Ok(bytes) => bytes,
            Err(e) => {
                failures.push(format!("{id}: dev-JIT run failed: {e}"));
                continue;
            }
        };
        // The ship tier: emit C, compile at -O2 -ffp-contract=off, link
        // with the runtime, run, capture stdout.
        let ship = match run_c_aot_with_native_libraries(&sources, &libraries) {
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
        // On windows-msvc the interop fixture is excluded, so interop entries
        // are not compiled or run in this cross-check either.
        #[cfg(all(windows, target_env = "msvc"))]
        if sources
            .iter()
            .any(|source| corpus::references_interop(&source.source))
        {
            continue;
        }
        let libraries = native_libraries(&sources);
        match run_aot_with_native_libraries(&sources, &libraries) {
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
