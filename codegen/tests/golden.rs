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
    run_c_aot_with_native_libraries, run_c_aot_with_native_libraries_and_host_hooks,
    run_jit_with_native_libraries, NativeLibrary, RunError,
};
#[cfg(not(all(windows, target_env = "msvc")))]
use subscript_codegen::{EntryArg, ReloadSession};

const HOST_OWNED_STATE_ID: &str = "a128-host-owned-state";
const HOST_OWNED_STATE_PRE_ENTRY: &str = "subHostOwnedStatePreEntry";
const HOST_OWNED_STATE_POST_RUN: &str = "subHostOwnedStatePostRun";
const HANDLE_ENTRY_PARAM_ID: &str = "a137-handle-entry-param";
const HANDLE_ENTRY_PARAM_PRE_ENTRY: &str = "subHostOwnedStateAdoptDrive";
const WIRE_ENTRY_PARAM_ID: &str = "a140-wire-entry-param";
const WIRE_ENTRY_PARAM_PRE_ENTRY: &str = "subWireEntryDrive";

fn host_hooks(id: &str) -> (Option<&'static str>, Option<&'static str>) {
    if id == HOST_OWNED_STATE_ID {
        (
            Some(HOST_OWNED_STATE_PRE_ENTRY),
            Some(HOST_OWNED_STATE_POST_RUN),
        )
    } else if id == HANDLE_ENTRY_PARAM_ID {
        (
            Some(HANDLE_ENTRY_PARAM_PRE_ENTRY),
            Some(HOST_OWNED_STATE_POST_RUN),
        )
    } else if id == WIRE_ENTRY_PARAM_ID {
        (Some(WIRE_ENTRY_PARAM_PRE_ENTRY), None)
    } else {
        (None, None)
    }
}

fn run_ship_corpus_entry(
    id: &str,
    sources: &[subscript_compiler::SourceFile],
    libraries: &[NativeLibrary],
) -> Result<Vec<u8>, RunError> {
    let (pre_entry, post_run) = host_hooks(id);
    run_c_aot_with_native_libraries_and_host_hooks(sources, libraries, pre_entry, post_run)
}

fn run_dev_corpus_entry(
    id: &str,
    sources: &[subscript_compiler::SourceFile],
    libraries: &[NativeLibrary],
) -> Result<Vec<u8>, RunError> {
    #[cfg(all(windows, target_env = "msvc"))]
    let _ = id;
    #[cfg(not(all(windows, target_env = "msvc")))]
    if id == HOST_OWNED_STATE_ID {
        let mut session = ReloadSession::new_with_native_libraries(sources, libraries)?;
        native_fixture::host_owned_state_pre_entry();
        let run = (|| {
            session.call_main()?;
            session.call_export("secondEntry")?;
            while session.async_pending() != 0 {
                session.async_step()?;
            }
            Ok(session.take_output())
        })();
        native_fixture::host_owned_state_post_run();
        return run;
    }
    #[cfg(not(all(windows, target_env = "msvc")))]
    if id == HANDLE_ENTRY_PARAM_ID {
        let mut session = ReloadSession::new_with_native_libraries(sources, libraries)?;
        native_fixture::host_owned_state_pre_entry();
        let run = (|| {
            let state = native_fixture::host_owned_state_borrow_and_advance();
            session.call_export_with("adopt", &[EntryArg::Handle(state), EntryArg::I32(7)])?;
            session.call_main()?;
            Ok(session.take_output())
        })();
        native_fixture::host_owned_state_post_run();
        return run;
    }
    #[cfg(not(all(windows, target_env = "msvc")))]
    if id == WIRE_ENTRY_PARAM_ID {
        let mut session = ReloadSession::new_with_native_libraries(sources, libraries)?;
        session.call_export_with("configure", &[EntryArg::I32(23), EntryArg::I32(5)])?;
        session.call_main()?;
        return Ok(session.take_output());
    }
    run_jit_with_native_libraries(sources, libraries)
}

#[test]
fn r27_field_initializer_entries_match_across_tiers() {
    let accept = corpus::corpus_accept();
    let mut failures = Vec::new();
    for id in ["a133-field-init-no-ctor", "a134-field-init-order"] {
        let sources = corpus::entry_sources(&accept, id);
        let libraries = native_libraries(&sources).expect("R27 entries have no native dependency");
        let jit = run_dev_corpus_entry(id, &sources, &libraries)
            .unwrap_or_else(|error| panic!("{id}: dev-JIT run failed: {error}"));
        let ship = run_ship_corpus_entry(id, &sources, &libraries)
            .unwrap_or_else(|error| panic!("{id}: ship-C-AOT run failed: {error}"));
        if jit != ship {
            failures.push(format!(
                "{id}: dev-JIT output {:?} != ship-C-AOT output {:?}",
                String::from_utf8_lossy(&jit),
                String::from_utf8_lossy(&ship)
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn r28_binary32_bit_access_matches_the_golden_across_tiers() {
    let accept = corpus::corpus_accept();
    let id = "a135-f32-bits";
    let sources = corpus::entry_sources(&accept, id);
    let libraries = native_libraries(&sources).expect("R28 has no native dependency");
    let golden = corpus::golden_bytes(&accept, id);
    let jit = run_dev_corpus_entry(id, &sources, &libraries)
        .unwrap_or_else(|error| panic!("{id}: dev-JIT run failed: {error}"));
    let ship = run_ship_corpus_entry(id, &sources, &libraries)
        .unwrap_or_else(|error| panic!("{id}: ship-C-AOT run failed: {error}"));
    assert_eq!(jit, golden, "{id}: dev-JIT output differs from the golden");
    assert_eq!(
        ship, golden,
        "{id}: ship-C-AOT output differs from the golden"
    );
}

#[test]
fn r29_class_index_signature_matches_the_golden_across_tiers() {
    let accept = corpus::corpus_accept();
    let id = "a136-index-signature";
    let sources = corpus::entry_sources(&accept, id);
    let libraries = native_libraries(&sources).expect("R29 has no native dependency");
    let golden = corpus::golden_bytes(&accept, id);
    let jit = run_dev_corpus_entry(id, &sources, &libraries)
        .unwrap_or_else(|error| panic!("{id}: dev-JIT run failed: {error}"));
    let ship = run_ship_corpus_entry(id, &sources, &libraries)
        .unwrap_or_else(|error| panic!("{id}: ship-C-AOT run failed: {error}"));
    assert_eq!(jit, golden, "{id}: dev-JIT output differs from the golden");
    assert_eq!(
        ship, golden,
        "{id}: ship-C-AOT output differs from the golden"
    );
}

#[test]
fn r30_handle_entry_parameters_match_the_golden_across_tiers() {
    let accept = corpus::corpus_accept();
    let id = HANDLE_ENTRY_PARAM_ID;
    let sources = corpus::entry_sources(&accept, id);
    let Some(libraries) = native_libraries(&sources) else {
        println!("{id}: skipped: interop fixture excluded here (compiler.md §11c)");
        return;
    };
    let golden = corpus::golden_bytes(&accept, id);
    let jit = run_dev_corpus_entry(id, &sources, &libraries)
        .unwrap_or_else(|error| panic!("{id}: dev-JIT run failed: {error}"));
    let ship = run_ship_corpus_entry(id, &sources, &libraries)
        .unwrap_or_else(|error| panic!("{id}: ship-C-AOT run failed: {error}"));
    assert_eq!(jit, golden, "{id}: dev-JIT output differs from the golden");
    assert_eq!(
        ship, golden,
        "{id}: ship-C-AOT output differs from the golden"
    );
}

#[test]
fn r32_wire_entry_parameters_match_the_golden_across_tiers() {
    let accept = corpus::corpus_accept();
    let id = WIRE_ENTRY_PARAM_ID;
    let sources = corpus::entry_sources(&accept, id);
    let Some(libraries) = native_libraries(&sources) else {
        println!("{id}: skipped: interop fixture excluded here (compiler.md §11c)");
        return;
    };
    let golden = corpus::golden_bytes(&accept, id);
    let jit = run_dev_corpus_entry(id, &sources, &libraries)
        .unwrap_or_else(|error| panic!("{id}: dev-JIT run failed: {error}"));
    let ship = run_ship_corpus_entry(id, &sources, &libraries)
        .unwrap_or_else(|error| panic!("{id}: ship-C-AOT run failed: {error}"));
    assert_eq!(jit, golden, "{id}: dev-JIT output differs from the golden");
    assert_eq!(
        ship, golden,
        "{id}: ship-C-AOT output differs from the golden"
    );
}

#[test]
fn r31_using_disposal_matches_the_goldens_across_tiers() {
    let accept = corpus::corpus_accept();
    for id in ["a138-using-dispose", "a139-using-async"] {
        let sources = corpus::entry_sources(&accept, id);
        let libraries = native_libraries(&sources).expect("R31 entries have no native dependency");
        let golden = corpus::golden_bytes(&accept, id);
        let jit = run_dev_corpus_entry(id, &sources, &libraries)
            .unwrap_or_else(|error| panic!("{id}: dev-JIT run failed: {error}"));
        let ship = run_ship_corpus_entry(id, &sources, &libraries)
            .unwrap_or_else(|error| panic!("{id}: ship-C-AOT run failed: {error}"));
        assert_eq!(jit, golden, "{id}: dev-JIT output differs from the golden");
        assert_eq!(
            ship, golden,
            "{id}: ship-C-AOT output differs from the golden"
        );
    }
}

#[cfg(not(all(windows, target_env = "msvc")))]
fn native_libraries(sources: &[subscript_compiler::SourceFile]) -> Option<Vec<NativeLibrary>> {
    if sources.iter().any(|source| {
        corpus::references_interop(&source.source)
            || source.source.contains("SubWireMode")
            || source.source.contains("SubBindTone")
    }) {
        Some(vec![native_fixture::library()])
    } else {
        Some(Vec::new())
    }
}

// On windows-msvc the interop fixture is excluded, so entries that reference
// it cannot run in this configuration.
#[cfg(all(windows, target_env = "msvc"))]
fn native_libraries(sources: &[subscript_compiler::SourceFile]) -> Option<Vec<NativeLibrary>> {
    if sources.iter().any(|source| {
        corpus::references_interop(&source.source)
            || source.source.contains("SubWireMode")
            || source.source.contains("SubBindTone")
    }) {
        None
    } else {
        Some(Vec::new())
    }
}

#[test]
fn unicode_string_entry_matches_across_tiers_before_golden_comparison() {
    let accept = corpus::corpus_accept();
    let id = "a60-string-unicode";
    let sources = corpus::entry_sources(&accept, id);
    let Some(libraries) = native_libraries(&sources) else {
        println!("{id}: skipped: interop fixture excluded here (compiler.md §11c)");
        return;
    };
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
    let Some(libraries) = native_libraries(&sources) else {
        println!("{id}: skipped: interop fixture excluded here (compiler.md §11c)");
        return;
    };
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
fn descriptor_literal_entry_matches_across_tiers_and_golden() {
    let accept = corpus::corpus_accept();
    let id = "a92-descriptor-literals";
    let sources = corpus::entry_sources(&accept, id);
    let Some(libraries) = native_libraries(&sources) else {
        println!("{id}: skipped: interop fixture excluded here (compiler.md §11c)");
        return;
    };
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
fn q34_async_entries_match_across_tiers_and_golden() {
    let accept = corpus::corpus_accept();
    for id in [
        "a93-async-chain",
        "a94-async-two-roots",
        "a95-interop-async-await",
    ] {
        let sources = corpus::entry_sources(&accept, id);
        let Some(libraries) = native_libraries(&sources) else {
            println!("{id}: skipped: interop fixture excluded here (compiler.md §11c)");
            continue;
        };
        let jit = run_jit_with_native_libraries(&sources, &libraries)
            .unwrap_or_else(|error| panic!("{id}: dev-JIT run failed: {error}"));
        let ship = run_c_aot_with_native_libraries(&sources, &libraries)
            .unwrap_or_else(|error| panic!("{id}: ship-C-AOT run failed: {error}"));
        let golden = corpus::golden_bytes(&accept, id);
        assert_eq!(jit, ship, "{id}: tier outputs differ");
        assert_eq!(jit, golden, "{id}: captured golden differs");
        println!("{id}:\n{}", String::from_utf8_lossy(&jit));
    }
}

#[test]
fn r13_async_method_entries_match_across_tiers_and_golden() {
    let accept = corpus::corpus_accept();
    for id in [
        "a110-async-method-receiver",
        "a111-interop-async-method-poll",
    ] {
        let sources = corpus::entry_sources(&accept, id);
        let Some(libraries) = native_libraries(&sources) else {
            println!("{id}: skipped: interop fixture excluded here (compiler.md §11c)");
            continue;
        };
        let jit = run_jit_with_native_libraries(&sources, &libraries)
            .unwrap_or_else(|error| panic!("{id}: dev-JIT run failed: {error}"));
        let ship = run_c_aot_with_native_libraries(&sources, &libraries)
            .unwrap_or_else(|error| panic!("{id}: ship-C-AOT run failed: {error}"));
        let golden = corpus::golden_bytes(&accept, id);
        assert_eq!(jit, ship, "{id}: tier outputs differ");
        assert_eq!(jit, golden, "{id}: captured golden differs");
        println!("{id}:\n{}", String::from_utf8_lossy(&jit));
    }
}

#[test]
fn capturing_lambda_environment_survives_recursive_reentry() {
    let accept = corpus::corpus_accept();
    let id = "a114-lambda-env-recursion";
    let sources = corpus::entry_sources(&accept, id);
    let libraries = native_libraries(&sources).expect("a114 has no native interop dependency");
    let jit = run_jit_with_native_libraries(&sources, &libraries)
        .unwrap_or_else(|error| panic!("{id}: dev-JIT run failed: {error}"));
    let ship = run_c_aot_with_native_libraries(&sources, &libraries)
        .unwrap_or_else(|error| panic!("{id}: ship-C-AOT run failed: {error}"));
    let golden = corpus::golden_bytes(&accept, id);
    assert_eq!(jit, golden, "{id}: dev-JIT output differs from the golden");
    assert_eq!(
        ship,
        golden,
        "{id}: ship-C-AOT output differs from the golden; dev-JIT emitted {:?}",
        String::from_utf8_lossy(&jit)
    );
}

#[test]
fn scalar_parameter_pair_entry_matches_across_tiers_and_golden() {
    let accept = corpus::corpus_accept();
    let id = "a96-interop-byte-pairs";
    let sources = corpus::entry_sources(&accept, id);
    let Some(libraries) = native_libraries(&sources) else {
        println!("{id}: skipped: interop fixture excluded here (compiler.md §11c)");
        return;
    };
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
fn embedded_chain_header_box_keeps_the_complete_extension() {
    let accept = corpus::corpus_accept();
    let id = "a89-interop-chain-payload";
    let sources = corpus::entry_sources(&accept, id);
    let Some(libraries) = native_libraries(&sources) else {
        println!("{id}: skipped: interop fixture excluded here (compiler.md §11c)");
        return;
    };
    let jit = run_jit_with_native_libraries(&sources, &libraries)
        .unwrap_or_else(|error| panic!("{id}: dev-JIT run failed: {error}"));
    let ship = run_c_aot_with_native_libraries(&sources, &libraries)
        .unwrap_or_else(|error| panic!("{id}: ship-C-AOT run failed: {error}"));
    let golden = corpus::golden_bytes(&accept, id);
    println!("{id} dev-JIT:\n{}", String::from_utf8_lossy(&jit));
    println!("{id} ship-C-AOT:\n{}", String::from_utf8_lossy(&ship));
    assert_eq!(jit, golden, "{id}: embedded-header payload is wrong");
    assert_eq!(ship, jit, "{id}: tier outputs differ");
}

#[test]
fn managed_boundary_box_survives_the_building_function() {
    let accept = corpus::corpus_accept();
    let id = "a169-managed-boundary-box";
    let sources = corpus::entry_sources(&accept, id);
    let Some(libraries) = native_libraries(&sources) else {
        println!("{id}: skipped: interop fixture excluded here (compiler.md §11c)");
        return;
    };
    let jit = run_jit_with_native_libraries(&sources, &libraries)
        .unwrap_or_else(|error| panic!("{id}: dev-JIT run failed: {error}"));
    let ship = run_c_aot_with_native_libraries(&sources, &libraries)
        .unwrap_or_else(|error| panic!("{id}: ship-C-AOT run failed: {error}"));
    let golden = corpus::golden_bytes(&accept, id);
    println!("{id} dev-JIT:\n{}", String::from_utf8_lossy(&jit));
    println!("{id} ship-C-AOT:\n{}", String::from_utf8_lossy(&ship));
    assert_eq!(jit, golden, "{id}: managed-box payload is wrong");
    assert_eq!(ship, jit, "{id}: tier outputs differ");
}

#[test]
fn string_field_pointer_write_direction_matches_both_tiers_and_golden() {
    let accept = corpus::corpus_accept();
    let id = "a97-interop-string-field-write";
    let sources = corpus::entry_sources(&accept, id);
    let Some(libraries) = native_libraries(&sources) else {
        println!("{id}: skipped: interop fixture excluded here (compiler.md §11c)");
        return;
    };
    let jit = run_jit_with_native_libraries(&sources, &libraries)
        .unwrap_or_else(|error| panic!("{id}: dev-JIT run failed: {error}"));
    let ship = run_c_aot_with_native_libraries(&sources, &libraries)
        .unwrap_or_else(|error| panic!("{id}: ship-C-AOT run failed: {error}"));
    let golden = corpus::golden_bytes(&accept, id);
    println!("dev-JIT:\n{}", String::from_utf8_lossy(&jit));
    println!("ship-C-AOT:\n{}", String::from_utf8_lossy(&ship));
    assert_eq!(jit, golden, "{id}: dev-JIT C observations are wrong");
    assert_eq!(ship, jit, "{id}: tier outputs differ");
}

#[test]
fn string_field_pointer_read_direction_matches_both_tiers_and_golden() {
    let accept = corpus::corpus_accept();
    let id = "a98-interop-string-field-read";
    let sources = corpus::entry_sources(&accept, id);
    let Some(libraries) = native_libraries(&sources) else {
        println!("{id}: skipped: interop fixture excluded here (compiler.md §11c)");
        return;
    };
    let jit = run_jit_with_native_libraries(&sources, &libraries)
        .unwrap_or_else(|error| panic!("{id}: dev-JIT run failed: {error}"));
    let ship = run_c_aot_with_native_libraries(&sources, &libraries)
        .unwrap_or_else(|error| panic!("{id}: ship-C-AOT run failed: {error}"));
    let golden = corpus::golden_bytes(&accept, id);
    println!("dev-JIT:\n{}", String::from_utf8_lossy(&jit));
    println!("ship-C-AOT:\n{}", String::from_utf8_lossy(&ship));
    assert_eq!(jit, golden, "{id}: C-filled view was not materialized");
    assert_eq!(ship, jit, "{id}: tier outputs differ");
}

#[test]
fn texture_descriptor_write_direction_matches_both_tiers_and_golden() {
    let accept = corpus::corpus_accept();
    let id = "a99-interop-texture-descriptor-write";
    let sources = corpus::entry_sources(&accept, id);
    let Some(libraries) = native_libraries(&sources) else {
        println!("{id}: skipped: interop fixture excluded here (compiler.md §11c)");
        return;
    };
    let jit = run_jit_with_native_libraries(&sources, &libraries)
        .unwrap_or_else(|error| panic!("{id}: dev-JIT run failed: {error}"));
    let ship = run_c_aot_with_native_libraries(&sources, &libraries)
        .unwrap_or_else(|error| panic!("{id}: ship-C-AOT run failed: {error}"));
    let golden = corpus::golden_bytes(&accept, id);
    println!("dev-JIT:\n{}", String::from_utf8_lossy(&jit));
    println!("ship-C-AOT:\n{}", String::from_utf8_lossy(&ship));
    assert_eq!(jit, golden, "{id}: C descriptor observations are wrong");
    assert_eq!(ship, jit, "{id}: tier outputs differ");
}

#[test]
fn texture_descriptor_read_direction_matches_both_tiers_and_golden() {
    let accept = corpus::corpus_accept();
    let id = "a100-interop-texture-descriptor-read";
    let sources = corpus::entry_sources(&accept, id);
    let Some(libraries) = native_libraries(&sources) else {
        println!("{id}: skipped: interop fixture excluded here (compiler.md §11c)");
        return;
    };
    let jit = run_jit_with_native_libraries(&sources, &libraries)
        .unwrap_or_else(|error| panic!("{id}: dev-JIT run failed: {error}"));
    let ship = run_c_aot_with_native_libraries(&sources, &libraries)
        .unwrap_or_else(|error| panic!("{id}: ship-C-AOT run failed: {error}"));
    let golden = corpus::golden_bytes(&accept, id);
    println!("dev-JIT:\n{}", String::from_utf8_lossy(&jit));
    println!("ship-C-AOT:\n{}", String::from_utf8_lossy(&ship));
    assert_eq!(jit, golden, "{id}: C-filled aggregate copy-back is wrong");
    assert_eq!(ship, jit, "{id}: tier outputs differ");
}

#[test]
fn recursive_boundary_pipeline_entries_match_both_tiers_and_goldens() {
    let accept = corpus::corpus_accept();
    for id in [
        "a103-interop-recursive-compute-pipeline",
        "a104-interop-recursive-render-pipeline",
        "a105-interop-recursive-string-pair-elements",
    ] {
        let sources = corpus::entry_sources(&accept, id);
        let Some(libraries) = native_libraries(&sources) else {
            println!("{id}: skipped: interop fixture excluded here (compiler.md §11c)");
            continue;
        };
        let jit = run_jit_with_native_libraries(&sources, &libraries)
            .unwrap_or_else(|error| panic!("{id}: dev-JIT run failed: {error}"));
        let ship = run_c_aot_with_native_libraries(&sources, &libraries)
            .unwrap_or_else(|error| panic!("{id}: ship-C-AOT run failed: {error}"));
        let golden = corpus::golden_bytes(&accept, id);
        println!("{id} dev-JIT:\n{}", String::from_utf8_lossy(&jit));
        println!("{id} ship-C-AOT:\n{}", String::from_utf8_lossy(&ship));
        assert_eq!(jit, golden, "{id}: recursive C observations are wrong");
        assert_eq!(ship, jit, "{id}: tier outputs differ");
    }
}

#[test]
fn struct_pointer_recursive_boundary_pipeline_matches_both_tiers_and_golden() {
    let accept = corpus::corpus_accept();
    let id = "a106-interop-recursive-struct-pointer-members";
    let sources = corpus::entry_sources(&accept, id);
    let Some(libraries) = native_libraries(&sources) else {
        println!("{id}: skipped: interop fixture excluded here (compiler.md §11c)");
        return;
    };
    let jit = run_jit_with_native_libraries(&sources, &libraries)
        .unwrap_or_else(|error| panic!("{id}: dev-JIT run failed: {error}"));
    let ship = run_c_aot_with_native_libraries(&sources, &libraries)
        .unwrap_or_else(|error| panic!("{id}: ship-C-AOT run failed: {error}"));
    let golden = corpus::golden_bytes(&accept, id);
    println!("{id} dev-JIT:\n{}", String::from_utf8_lossy(&jit));
    println!("{id} ship-C-AOT:\n{}", String::from_utf8_lossy(&ship));
    assert_eq!(
        jit, golden,
        "{id}: pointer-reachable C observations are wrong"
    );
    assert_eq!(ship, jit, "{id}: tier outputs differ");
}

#[test]
fn handle_beside_arrays_through_nullable_member_matches_both_tiers_and_golden() {
    let accept = corpus::corpus_accept();
    let id = "a119-interop-handle-beside-arrays";
    let sources = corpus::entry_sources(&accept, id);
    let Some(libraries) = native_libraries(&sources) else {
        println!("{id}: skipped: interop fixture excluded here (compiler.md §11c)");
        return;
    };
    let jit = run_jit_with_native_libraries(&sources, &libraries)
        .unwrap_or_else(|error| panic!("{id}: dev-JIT run failed: {error}"));
    let ship = run_c_aot_with_native_libraries(&sources, &libraries)
        .unwrap_or_else(|error| panic!("{id}: ship-C-AOT run failed: {error}"));
    let golden = corpus::golden_bytes(&accept, id);
    println!("{id} dev-JIT:\n{}", String::from_utf8_lossy(&jit));
    println!("{id} ship-C-AOT:\n{}", String::from_utf8_lossy(&ship));
    assert_eq!(jit, golden, "{id}: handle/array C observations are wrong");
    assert_eq!(ship, jit, "{id}: tier outputs differ");
}

#[test]
fn nested_structs_behind_array_element_pointer_match_both_tiers_and_golden() {
    let accept = corpus::corpus_accept();
    let id = "a120-interop-nested-behind-element-pointer";
    let sources = corpus::entry_sources(&accept, id);
    let Some(libraries) = native_libraries(&sources) else {
        println!("{id}: skipped: interop fixture excluded here (compiler.md §11c)");
        return;
    };
    let jit = run_jit_with_native_libraries(&sources, &libraries)
        .unwrap_or_else(|error| panic!("{id}: dev-JIT run failed: {error}"));
    let ship = run_c_aot_with_native_libraries(&sources, &libraries)
        .unwrap_or_else(|error| panic!("{id}: ship-C-AOT run failed: {error}"));
    let golden = corpus::golden_bytes(&accept, id);
    println!("{id} dev-JIT:\n{}", String::from_utf8_lossy(&jit));
    println!("{id} ship-C-AOT:\n{}", String::from_utf8_lossy(&ship));
    assert_eq!(jit, golden, "{id}: nested-pointer C observations are wrong");
    assert_eq!(ship, jit, "{id}: tier outputs differ");
}

#[test]
fn unmarked_struct_pointer_in_array_element_matches_both_tiers_and_golden() {
    let accept = corpus::corpus_accept();
    let id = "a121-interop-unmarked-reach-through";
    let sources = corpus::entry_sources(&accept, id);
    let Some(libraries) = native_libraries(&sources) else {
        println!("{id}: skipped: interop fixture excluded here (compiler.md §11c)");
        return;
    };
    let jit = run_jit_with_native_libraries(&sources, &libraries)
        .unwrap_or_else(|error| panic!("{id}: dev-JIT run failed: {error}"));
    let ship = run_c_aot_with_native_libraries(&sources, &libraries)
        .unwrap_or_else(|error| panic!("{id}: ship-C-AOT run failed: {error}"));
    let golden = corpus::golden_bytes(&accept, id);
    println!("{id} dev-JIT:\n{}", String::from_utf8_lossy(&jit));
    println!("{id} ship-C-AOT:\n{}", String::from_utf8_lossy(&ship));
    assert_eq!(
        jit, golden,
        "{id}: unmarked-pointer C observations are wrong"
    );
    assert_eq!(ship, jit, "{id}: tier outputs differ");
}

#[test]
fn two_reach_through_pointer_members_match_both_tiers_and_golden() {
    let accept = corpus::corpus_accept();
    let id = "a122-interop-two-pointer-members";
    let sources = corpus::entry_sources(&accept, id);
    let Some(libraries) = native_libraries(&sources) else {
        println!("{id}: skipped: interop fixture excluded here (compiler.md §11c)");
        return;
    };
    let jit = run_jit_with_native_libraries(&sources, &libraries)
        .unwrap_or_else(|error| panic!("{id}: dev-JIT run failed: {error}"));
    let ship = run_c_aot_with_native_libraries(&sources, &libraries)
        .unwrap_or_else(|error| panic!("{id}: ship-C-AOT run failed: {error}"));
    let golden = corpus::golden_bytes(&accept, id);
    println!("{id} dev-JIT:\n{}", String::from_utf8_lossy(&jit));
    println!("{id} ship-C-AOT:\n{}", String::from_utf8_lossy(&ship));
    assert_eq!(jit, golden, "{id}: breadth-axis C observations are wrong");
    assert_eq!(ship, jit, "{id}: tier outputs differ");
}

#[test]
fn wide_descriptor_breadth_and_depth_match_both_tiers_and_golden() {
    let accept = corpus::corpus_accept();
    let id = "a123-interop-wide-descriptor";
    let sources = corpus::entry_sources(&accept, id);
    let Some(libraries) = native_libraries(&sources) else {
        println!("{id}: skipped: interop fixture excluded here (compiler.md §11c)");
        return;
    };
    let jit = run_jit_with_native_libraries(&sources, &libraries)
        .unwrap_or_else(|error| panic!("{id}: dev-JIT run failed: {error}"));
    let ship = run_c_aot_with_native_libraries(&sources, &libraries)
        .unwrap_or_else(|error| panic!("{id}: ship-C-AOT run failed: {error}"));
    let golden = corpus::golden_bytes(&accept, id);
    println!("{id} dev-JIT:\n{}", String::from_utf8_lossy(&jit));
    println!("{id} ship-C-AOT:\n{}", String::from_utf8_lossy(&ship));
    assert_eq!(
        jit, golden,
        "{id}: wide-descriptor C observations are wrong"
    );
    assert_eq!(ship, jit, "{id}: tier outputs differ");
}

#[test]
fn contextual_conditionals_match_both_tiers_and_golden() {
    let accept = corpus::corpus_accept();
    let id = "a124-contextual-conditional";
    let sources = corpus::entry_sources(&accept, id);
    let Some(libraries) = native_libraries(&sources) else {
        println!("{id}: skipped: interop fixture excluded here (compiler.md §11c)");
        return;
    };
    let jit = run_jit_with_native_libraries(&sources, &libraries)
        .unwrap_or_else(|error| panic!("{id}: dev-JIT run failed: {error}"));
    let ship = run_c_aot_with_native_libraries(&sources, &libraries)
        .unwrap_or_else(|error| panic!("{id}: ship-C-AOT run failed: {error}"));
    let golden = corpus::golden_bytes(&accept, id);
    println!("{id} dev-JIT:\n{}", String::from_utf8_lossy(&jit));
    println!("{id} ship-C-AOT:\n{}", String::from_utf8_lossy(&ship));
    assert_eq!(
        jit, golden,
        "{id}: contextual conditional observations are wrong"
    );
    assert_eq!(ship, jit, "{id}: tier outputs differ");
}

#[test]
fn conditional_arm_narrowing_matches_both_tiers_and_golden() {
    let accept = corpus::corpus_accept();
    let id = "a125-conditional-arm-narrowing";
    let sources = corpus::entry_sources(&accept, id);
    let Some(libraries) = native_libraries(&sources) else {
        println!("{id}: skipped: interop fixture excluded here (compiler.md §11c)");
        return;
    };
    let jit = run_jit_with_native_libraries(&sources, &libraries)
        .unwrap_or_else(|error| panic!("{id}: dev-JIT run failed: {error}"));
    let ship = run_c_aot_with_native_libraries(&sources, &libraries)
        .unwrap_or_else(|error| panic!("{id}: ship-C-AOT run failed: {error}"));
    let golden = corpus::golden_bytes(&accept, id);
    println!("{id} dev-JIT:\n{}", String::from_utf8_lossy(&jit));
    println!("{id} ship-C-AOT:\n{}", String::from_utf8_lossy(&ship));
    assert_eq!(jit, golden, "{id}: conditional-arm observations are wrong");
    assert_eq!(ship, jit, "{id}: tier outputs differ");
}

#[test]
fn suspension_state_matches_both_tiers_and_golden() {
    let accept = corpus::corpus_accept();
    let id = "a149-suspension-state";
    let sources = corpus::entry_sources(&accept, id);
    let Some(libraries) = native_libraries(&sources) else {
        println!("{id}: skipped: interop fixture excluded here (compiler.md §11c)");
        return;
    };
    let jit = run_jit_with_native_libraries(&sources, &libraries).unwrap_or_else(|error| {
        if let RunError::AbnormalTermination(termination) = &error {
            eprintln!(
                "{id}: dev-JIT output before termination:\n{}",
                String::from_utf8_lossy(&termination.stdout)
            );
        }
        panic!("{id}: dev-JIT run failed: {error}")
    });
    let ship = run_c_aot_with_native_libraries(&sources, &libraries)
        .unwrap_or_else(|error| panic!("{id}: ship-C-AOT run failed: {error}"));
    let golden = corpus::golden_bytes(&accept, id);
    assert_eq!(jit, golden, "{id}: dev-JIT suspension output is wrong");
    assert_eq!(ship, jit, "{id}: tier outputs differ");
}

#[test]
fn by_value_packing_matches_both_tiers_and_golden() {
    let accept = corpus::corpus_accept();
    let id = "a126-interop-by-value-packing";
    let sources = corpus::entry_sources(&accept, id);
    let Some(libraries) = native_libraries(&sources) else {
        println!("{id}: skipped: interop fixture excluded here (compiler.md §11c)");
        return;
    };
    let jit = run_jit_with_native_libraries(&sources, &libraries)
        .unwrap_or_else(|error| panic!("{id}: dev-JIT run failed: {error}"));
    let ship = run_c_aot_with_native_libraries(&sources, &libraries)
        .unwrap_or_else(|error| panic!("{id}: ship-C-AOT run failed: {error}"));
    let golden = corpus::golden_bytes(&accept, id);
    println!("{id} dev-JIT:\n{}", String::from_utf8_lossy(&jit));
    println!("{id} ship-C-AOT:\n{}", String::from_utf8_lossy(&ship));
    assert_eq!(jit, golden, "{id}: by-value packing observations are wrong");
    assert_eq!(ship, jit, "{id}: tier outputs differ");
}

#[test]
fn handle_parameter_pair_matches_both_tiers_and_golden() {
    let accept = corpus::corpus_accept();
    let id = "a107-interop-handle-parameter-pair";
    let sources = corpus::entry_sources(&accept, id);
    let Some(libraries) = native_libraries(&sources) else {
        println!("{id}: skipped: interop fixture excluded here (compiler.md §11c)");
        return;
    };
    let jit = run_jit_with_native_libraries(&sources, &libraries)
        .unwrap_or_else(|error| panic!("{id}: dev-JIT run failed: {error}"));
    let ship = run_c_aot_with_native_libraries(&sources, &libraries)
        .unwrap_or_else(|error| panic!("{id}: ship-C-AOT run failed: {error}"));
    let golden = corpus::golden_bytes(&accept, id);
    println!("{id} dev-JIT:\n{}", String::from_utf8_lossy(&jit));
    println!("{id} ship-C-AOT:\n{}", String::from_utf8_lossy(&ship));
    assert_eq!(
        jit, golden,
        "{id}: C handle identity observations are wrong"
    );
    assert_eq!(ship, jit, "{id}: tier outputs differ");
}

#[test]
fn nullable_handle_parameter_matches_both_tiers_and_golden() {
    let accept = corpus::corpus_accept();
    let id = "a108-interop-nullable-handle-parameter";
    let sources = corpus::entry_sources(&accept, id);
    let Some(libraries) = native_libraries(&sources) else {
        println!("{id}: skipped: interop fixture excluded here (compiler.md §11c)");
        return;
    };
    let jit = run_jit_with_native_libraries(&sources, &libraries)
        .unwrap_or_else(|error| panic!("{id}: dev-JIT run failed: {error}"));
    let ship = run_c_aot_with_native_libraries(&sources, &libraries)
        .unwrap_or_else(|error| panic!("{id}: ship-C-AOT run failed: {error}"));
    let golden = corpus::golden_bytes(&accept, id);
    println!("{id} dev-JIT:\n{}", String::from_utf8_lossy(&jit));
    println!("{id} ship-C-AOT:\n{}", String::from_utf8_lossy(&ship));
    assert_eq!(jit, golden, "{id}: C handle/null observations are wrong");
    assert_eq!(ship, jit, "{id}: tier outputs differ");
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
        let Some(libraries) = native_libraries(&sources) else {
            println!("{id}: skipped: interop fixture excluded here (compiler.md §11c)");
            continue;
        };
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
    let mut failures = Vec::new();
    let mut compared = 0usize;
    let mut skipped = 0usize;
    for id in &golden_ids {
        let golden = corpus::golden_bytes(&accept, id);
        let sources = corpus::entry_sources(&accept, id);
        let Some(libraries) = native_libraries(&sources) else {
            println!("{id}: skipped: interop fixture excluded here (compiler.md §11c)");
            skipped += 1;
            continue;
        };
        let jit = match run_dev_corpus_entry(id, &sources, &libraries) {
            Ok(bytes) => bytes,
            Err(e) => {
                failures.push(format!("{id}: dev-JIT run failed: {e}"));
                continue;
            }
        };
        // The ship tier: emit C, compile at -O2 -ffp-contract=off, link
        // with the runtime, run, capture stdout.
        let ship = match run_ship_corpus_entry(id, &sources, &libraries) {
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
    println!("golden sweep: compared {compared} entries, skipped {skipped} entries");
    assert!(
        failures.is_empty(),
        "{} differential failure(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert_eq!(
        compared + skipped,
        golden_ids.len(),
        "every committed golden must be compared on both tiers or explicitly skipped"
    );
    #[cfg(not(all(windows, target_env = "msvc")))]
    assert_eq!(
        skipped, 0,
        "the reference configuration compares every committed golden and skips none"
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
        assert_eq!(
            golden.last(),
            Some(&b'\n'),
            "{id}: golden has no final newline"
        );
    }
}
