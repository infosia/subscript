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
    run_c_aot_with_native_libraries_and_host_hooks, run_jit_with_native_libraries,
    NativeLibrary, RunError,
};
#[cfg(not(all(windows, target_env = "msvc")))]
use subscript_codegen::ReloadSession;

const HOST_OWNED_STATE_ID: &str = "a128-host-owned-state";
const HOST_OWNED_STATE_PRE_ENTRY: &str = "subHostOwnedStatePreEntry";
const HOST_OWNED_STATE_POST_RUN: &str = "subHostOwnedStatePostRun";

fn host_hooks(id: &str) -> (Option<&'static str>, Option<&'static str>) {
    if id == HOST_OWNED_STATE_ID {
        (
            Some(HOST_OWNED_STATE_PRE_ENTRY),
            Some(HOST_OWNED_STATE_POST_RUN),
        )
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
    run_c_aot_with_native_libraries_and_host_hooks(
        sources,
        libraries,
        pre_entry,
        post_run,
    )
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
    run_jit_with_native_libraries(sources, libraries)
}

#[cfg(not(all(windows, target_env = "msvc")))]
fn native_libraries(sources: &[subscript_compiler::SourceFile]) -> Option<Vec<NativeLibrary>> {
    if sources
        .iter()
        .any(|source| corpus::references_interop(&source.source))
    {
        Some(vec![native_fixture::library()])
    } else {
        Some(Vec::new())
    }
}

// On windows-msvc the interop fixture is excluded, so entries that reference
// it cannot run in this configuration.
#[cfg(all(windows, target_env = "msvc"))]
fn native_libraries(sources: &[subscript_compiler::SourceFile]) -> Option<Vec<NativeLibrary>> {
    if sources
        .iter()
        .any(|source| corpus::references_interop(&source.source))
    {
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
    assert_eq!(
        jit,
        golden,
        "{id}: C-filled view was not materialized"
    );
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
    assert_eq!(jit, golden, "{id}: pointer-reachable C observations are wrong");
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
    assert_eq!(jit, golden, "{id}: unmarked-pointer C observations are wrong");
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
    assert_eq!(jit, golden, "{id}: wide-descriptor C observations are wrong");
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
    assert_eq!(jit, golden, "{id}: contextual conditional observations are wrong");
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
    assert_eq!(jit, golden, "{id}: C handle identity observations are wrong");
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
    // array-pairs (a96), and R6 string-view fields in pointer-passed
    // boundary structs in both directions (a97–a98), and R7 texture
    // descriptors with nested aggregates + enum pairs (a99–a100), and R8
    // opaque-handle aggregate positions (a101–a102), and R9 recursive
    // embedded/pair-element lowering (a103–a105), R10 recursive
    // struct-pointer-member lowering (a106), and R11 handle parameter
    // pairs (a107), R12 nullable handle parameters (a108), and OBS-1
    // null-only boundary type reachability (a109), and R13 async instance
    // methods (a110–a111), Q35 workers (a112–a113), and the capturing-lambda
    // recursion review pin (a114), R14 Q32 alias switches (a115), R15
    // divergence flow (a116), R17 nullable descriptor literals (a117),
    // R16 absence-capable descriptor members (a118), and OBS-3 scalar
    // handle fields beside arrays behind nullable members (a119), and
    // nested structs behind array-element nullable pointers (a120), and
    // unmarked reach-through pointers in array elements (a121), two
    // simultaneous reach-through pointer members (a122), and the wide
    // breadth-and-depth composition (a123), and R18 contextual conditional
    // typing (a124), R19 conditional-arm narrowing (a125), and OBS-4
    // by-value register-image packing (a126), and R20 external types shared
    // across two generated mirrors (a127), and R21 host-owned state across
    // two host-driven entry calls (a128), and R23 wire-mapped literal-union
    // boundary crossings (a129), and R24 bind-generated enum-typedef CEnum
    // crossings (a130).
    assert_eq!(
        golden_ids.len(),
        130,
        "expected exactly 130 committed goldens: the 81 standing goldens (a01–a24 run set + a25–a39 interop \
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
         scalar parameter-pair interop, a97–a98 R6 pointer-passed boundary \
         string-field interop in both directions, and a99–a100 R7 texture \
         descriptor interop in both directions, a101–a102 R8 opaque-handle \
         aggregate interop, a103–a105 R9 recursive lowering, and a106 R10 \
         struct-pointer-member lowering, and a107 R11 handle parameter-pair \
         interop, a108 R12 nullable handle parameter interop, a109 OBS-1 \
         null-only boundary type reachability, a110–a111 R13 async \
         instance-method goldens, a112–a113 Q35 worker goldens, and the a114 \
         capturing-lambda recursion review golden, the a115 R14 Q32 \
         alias-switch golden, the a116 R15 divergence-flow golden, and the \
         a117 R17 nullable-descriptor-literal golden, and the a118 R16 \
         absence-capable descriptor-member golden, the a119 OBS-3 \
         handle-beside-arrays golden, and the a120 OBS-3 nested-behind-element-pointer \
         golden, the a121 OBS-3 unmarked-reach-through golden, the a122 \
         OBS-3 two-pointer-members golden, and the a123 OBS-3 wide-descriptor \
         golden, the a124 R18 contextual-conditional golden, and the a125 \
         R19 conditional-arm-narrowing golden, and the a126 OBS-4 by-value \
         register-image packing golden, and the a127 R20 external-type \
         two-mirror golden, the a128 R21 host-owned-state golden, and the a129 \
         R23 wire-enum interop golden, and the a130 R24 bind-generated \
         enum-typedef wire-enum golden, found {}",
        golden_ids.len()
    );

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

/// Optional cross-check: the retired `cranelift-object` AOT path still
/// reproduces the goldens byte-for-byte (§11 keeps it as an extra
/// column, not a requirement). It shares the dev tier's lowering, so it
/// is a cheap independent confirmation that the goldens are stable.
#[test]
fn cranelift_object_aot_still_matches_the_goldens_cross_check() {
    let accept = corpus::corpus_accept();
    let mut failures = Vec::new();
    for id in corpus::golden_ids(&accept) {
        if id == HOST_OWNED_STATE_ID {
            // R21's new host-driven form is a ship-C surface. The retired
            // Cranelift-object cross-check has no hook-enabled runner.
            continue;
        }
        let golden = corpus::golden_bytes(&accept, &id);
        let sources = corpus::entry_sources(&accept, &id);
        let Some(libraries) = native_libraries(&sources) else {
            println!("{id}: skipped: interop fixture excluded here (compiler.md §11c)");
            continue;
        };
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
