//! Discovery for the runtime-trap corpus category.

use crate::corpus::interop;

use std::fs;
use std::path::{Path, PathBuf};

use subscript_codegen::{
    run_c_aot_with_native_libraries_and_host_hooks, EntryArg, NativeLibrary, ReloadSession,
    RunError,
};
use subscript_compiler::SourceFile;
use subscript_runtime::TrapKind;

/// The corpus trap directory.
pub fn corpus_trap() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../corpus/trap")
}

/// Every trap entry id, sorted.
///
/// The trap category contains paired single-file `.ts` and `.expected`
/// entries. Rejecting every other directory member and incomplete pair
/// makes the returned count an exact enumeration of the category,
/// rather than a count that silently ignores unrecognized files.
pub fn trap_ids(trap: &Path) -> Vec<String> {
    let mut source_ids = Vec::new();
    let mut expected_ids = Vec::new();
    for entry in fs::read_dir(trap).expect("read corpus/trap") {
        let entry = entry.expect("read corpus/trap entry");
        assert!(
            entry.path().is_file(),
            "{}: trap corpus entries must be files",
            entry.path().display()
        );
        let name = entry.file_name().to_string_lossy().into_owned();
        let (id, ids) = if let Some(id) = name.strip_suffix(".ts") {
            (id, &mut source_ids)
        } else if let Some(id) = name.strip_suffix(".expected") {
            (id, &mut expected_ids)
        } else {
            panic!("{name}: trap corpus contains only paired .ts and .expected entries");
        };
        assert!(
            id.starts_with('t'),
            "{name}: trap corpus ids must start with `t`"
        );
        ids.push(id.to_string());
    }
    source_ids.sort();
    expected_ids.sort();
    assert_eq!(
        source_ids, expected_ids,
        "trap corpus .ts and .expected ids must be paired exactly"
    );
    source_ids
}

/// Loads one single-file trap corpus entry.
pub fn trap_sources(trap: &Path, id: &str) -> Vec<SourceFile> {
    let path = trap.join(format!("{id}.ts"));
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read trap entry {}: {e}", path.display()));
    let mut sources = interop::mirrors_for(&text, SourceFile::ambient);
    sources.push(SourceFile::new(format!("{id}.ts"), text));
    sources
}

/// Loads the exact dev-tier stdout expected before a trap.
pub fn trap_expected(trap: &Path, id: &str) -> Vec<u8> {
    let path = trap.join(format!("{id}.expected"));
    fs::read(&path).unwrap_or_else(|e| panic!("read trap golden {}: {e}", path.display()))
}

/// Drives the unknown wire value through the dev host-entry surface.
pub fn run_wire_entry_unknown_dev(
    sources: &[SourceFile],
    libraries: &[NativeLibrary],
) -> Result<Vec<u8>, RunError> {
    let mut session = ReloadSession::new_with_native_libraries(sources, libraries)?;
    session.call_export_with("configure", &[EntryArg::I32(12345), EntryArg::I32(5)])?;
    session.call_main()?;
    Ok(session.take_output())
}

/// Drives the unknown wire value through the ship host-entry surface.
pub fn run_wire_entry_unknown_ship(
    sources: &[SourceFile],
    libraries: &[NativeLibrary],
) -> Result<Vec<u8>, RunError> {
    run_c_aot_with_native_libraries_and_host_hooks(
        sources,
        libraries,
        Some("subWireEntryDriveUnknown"),
        None,
    )
}

/// The trap kind, line, and column that each trap entry states.
pub fn trap_expectation(id: &str) -> (TrapKind, u32, u32) {
    match id {
        "t02-statements-after-fault" => (TrapKind::IndexOutOfBounds, 10, 24),
        "t03-loop-stops-at-fault" => (TrapKind::IndexOutOfBounds, 13, 28),
        "t04-call-after-fault" => (TrapKind::IndexOutOfBounds, 15, 24),
        "t05-foreach-callback-fault" => (TrapKind::IndexOutOfBounds, 13, 26),
        "t06-push-after-fault" => (TrapKind::IndexOutOfBounds, 11, 24),
        "t07-p19-compound-assign-ordering" => (TrapKind::IndexOutOfBounds, 10, 3),
        "t08-div-zero-expression" => (TrapKind::DivisionByZero, 10, 25),
        "t09-rem-zero-expression" => (TrapKind::DivisionByZero, 10, 26),
        "t10-div-zero-loop-condition" | "t11-rem-zero-loop-condition" => {
            (TrapKind::DivisionByZero, 10, 11)
        }
        "t12-div-zero-array-element" | "t13-rem-zero-array-element" => {
            (TrapKind::DivisionByZero, 10, 29)
        }
        "t14-div-zero-call-divisor" => (TrapKind::DivisionByZero, 14, 25),
        "t15-rem-zero-call-divisor" => (TrapKind::DivisionByZero, 14, 26),
        "t16-array-write-oob" => (TrapKind::IndexOutOfBounds, 11, 3),
        "t17-fixed-array-read-oob" => (TrapKind::IndexOutOfBounds, 8, 10),
        "t18-fixed-array-write-oob" => (TrapKind::IndexOutOfBounds, 8, 3),
        "t19-array-compound-second-write-oob" => (TrapKind::IndexOutOfBounds, 16, 3),
        "t20-narrow-null" => (TrapKind::NullNarrowing, 20, 29),
        "t21-narrow-class-mismatch" => (TrapKind::ClassMismatch, 27, 33),
        "t22-double-delete-q6" => (TrapKind::DoubleDelete, 19, 3),
        "t23-use-after-delete-q6" => (TrapKind::UseAfterDelete, 23, 12),
        "t24-stale-coroutine-reload" => (TrapKind::StaleCoroutine, 19, 3),
        "t25-allocation-sites-before-second-template-fault" => (TrapKind::DivisionByZero, 21, 47),
        "t26-allocation-failure-new" => (TrapKind::AllocationFailure, 17, 20),
        "t27-dynamic-value-field-write-oob" => (TrapKind::IndexOutOfBounds, 25, 3),
        "t28-allocation-failure-array-literal" => (TrapKind::AllocationFailure, 9, 25),
        "t29-allocation-failure-push-grow" => (TrapKind::AllocationFailure, 10, 3),
        "t30-allocation-failure-string-concat" => (TrapKind::AllocationFailure, 11, 26),
        "t31-allocation-failure-template" => (TrapKind::AllocationFailure, 10, 26),
        "t32-allocation-failure-generator-frame" => (TrapKind::AllocationFailure, 7, 11),
        "t33-allocation-failure-json-raw-new" => (TrapKind::AllocationFailure, 18, 5),
        "t35-allocation-failure-map-new" => (TrapKind::AllocationFailure, 9, 33),
        "t36-allocation-failure-set-new" => (TrapKind::AllocationFailure, 9, 28),
        "t37-allocation-failure-map-grow" | "t38-allocation-failure-set-grow" => {
            (TrapKind::AllocationFailure, 10, 3)
        }
        "t39-regex-budget" => (TrapKind::RegexBudget, 9, 12),
        "t40-regex-invalid-pattern"
        | "t41-regex-unsupported-flag"
        | "t42-regex-duplicate-flag"
        | "t43-regex-mutually-exclusive-flags"
        | "t45-regex-sticky-last-index" => (TrapKind::Regex, 8, 25),
        "t44-regex-replace-all-without-global" => (TrapKind::Regex, 8, 9),
        "t46-callback-userdata-freed" => (TrapKind::CallbackUserdataFreed, 31, 43),
        "t47-unreachable-reached" => (TrapKind::UnreachableReached, 10, 3),
        "t54-async-start-fault" => (TrapKind::UnreachableReached, 9, 3),
        "t55-async-trap-after-settled-await" => (TrapKind::UnreachableReached, 15, 3),
        "t48-wire-enum-unknown-value" => (TrapKind::WireEnumUnknownValue, 10, 30),
        "t49-wire-enum-struct-unknown-member" => (TrapKind::WireEnumUnknownValue, 12, 36),
        "t50-wire-entry-unknown-value" => (TrapKind::WireEnumUnknownValue, 8, 27),
        "t51-bytes-into-range" => (TrapKind::IndexOutOfBounds, 18, 3),
        "t52-static-map-callback-fault" | "t53-value-map-callback-fault" => {
            (TrapKind::IndexOutOfBounds, 9, 18)
        }
        "t56-pattern-short-array" => (TrapKind::IndexOutOfBounds, 10, 17),
        // §14.4b (A) through a §111 registration: the trap carries the
        // freed userdata's allocation site, as t46 does.
        "t59-registration-userdata-freed" => (TrapKind::CallbackUserdataFreed, 42, 40),
        // §111 rule 14 records position 0, because the refused fire
        // reaches no script site. Both tiers report the empty position
        // for this kind, and the entry pins their agreement.
        "t60-registration-fire-after-release" => (TrapKind::CallbackRegistrationEnded, 0, 0),
        // compiler.md §115.4 item 1: the position of the `throw`.
        "t61-uncaught-exception" => (TrapKind::UncaughtException, 9, 5),
        // compiler.md §115.5 rule 2: the position of the `using` binding
        // whose hook raised.
        "t62-hook-raises-during-exit" => (TrapKind::DisposeRaisedDuringExit, 26, 9),
        // compiler.md §115.4 items 3 and 5, §116.1 rule 5: the boundary
        // converts the exception, and the trap cites the position of the
        // `throw`.
        "t63-exception-leaves-async" => (TrapKind::UncaughtException, 16, 5),
        "t64-exception-leaves-generator" => (TrapKind::UncaughtException, 10, 7),
        "t65-exception-leaves-host-callback" => (TrapKind::UncaughtException, 13, 7),
        // compiler.md §116.1 rule 4: the release of the last holder traps,
        // and the trap cites the position of the `throw`.
        "t66-unobserved-async-exception" => (TrapKind::UncaughtException, 9, 3),
        "t67-exception-exit-releases-its-handle" => (TrapKind::UncaughtException, 8, 40),
        "t68-narrowing-accessor-compound" | "t69-narrowing-static-accessor-compound" => {
            (TrapKind::SharedNullNarrowing, 13, 34)
        }
        "t70-narrowing-optional-getter" => (TrapKind::SharedNullNarrowing, 17, 16),
        "t73-boxed-local-narrowing-call" => (TrapKind::SharedNullNarrowing, 17, 18),
        "t74-boxed-local-narrowing-alias-store" => (TrapKind::SharedNullNarrowing, 21, 18),
        "t75-generator-done-value-member" | "t76-generator-done-value-store" => {
            (TrapKind::GeneratorDoneValue, 16, 20)
        }
        "t77-generator-done-fixed-reference" => (TrapKind::GeneratorDoneValue, 15, 15),
        "t78-generator-done-string" => (TrapKind::GeneratorDoneValue, 14, 23),
        "t80-generator-done-wire-alias" => (TrapKind::GeneratorDoneValue, 21, 18),
        "t81-generator-done-fixed-wire-alias" => (TrapKind::GeneratorDoneValue, 15, 15),
        "t72-narrowing-boundary-getter" => (TrapKind::SharedNullNarrowing, 13, 44),
        "t71-narrowing-destructuring-getter" => (TrapKind::SharedNullNarrowing, 16, 16),
        "t84-dropped-unfinished-task-group" | "t85-dropped-failed-task-group" => {
            (TrapKind::TaskGroup, 11, 16)
        }
        "t86-add-to-closed-task-group" => (TrapKind::TaskGroup, 15, 3),
        "t102-zero-length-store"
        | "t87-discarded-counted-shift"
        | "t88-parameter-counted-pop"
        | "t89-counted-array-completion"
        | "t90-nested-counted-array"
        | "t91-fixed-counted-array"
        | "t92-counted-generator-holder"
        | "t93-counted-fill-release" => (TrapKind::UncaughtException, 10, 40),
        "t94-counted-map-delete" | "t95-counted-map-clear" | "t96-counted-map-replace" => {
            (TrapKind::UncaughtException, 10, 40)
        }
        // compiler.md §342 rule 15: the detection collection reports at the `throw`.
        "t97-counted-field-collect"
        | "t98-counted-field-task-order"
        | "t107-collected-task-order"
        | "t116-detected-task-failure"
        | "t117-detected-task-order" => (TrapKind::UncaughtException, 11, 55),
        "t118-unreachable-completed-task-failure" => (TrapKind::UncaughtException, 18, 65),
        "t108-host-operation-dropped-error" => (TrapKind::UncaughtException, 12, 15),
        "t101-unused-coroutine-parameter" => (TrapKind::UncaughtException, 19, 79),
        "t102-generator-parameter" => (TrapKind::UncaughtException, 24, 5),
        "t105-dropped-generator-break" => (TrapKind::UncaughtException, 12, 39),
        "t109-generator-close-throws" => (TrapKind::UncaughtException, 11, 13),
        // compiler.md §191 rule 1: the position of the padding call.
        "t110-pad-start-cut-inside-sequence" | "t111-pad-end-cut-inside-sequence" => {
            (TrapKind::StringSlice, 13, 23)
        }
        // compiler.md §200.1 rule 2: the position of the `switch`.
        "t112-enum-switch-non-member-value" => (TrapKind::UnreachableReached, 20, 3),
        // compiler.md §220.1 rule 6: the position of the subject.
        "t113-fixed-array-traversal-element-removed" => (TrapKind::IndexOutOfBounds, 12, 19),
        "t106-dropped-generator-local" => (TrapKind::UncaughtException, 12, 39),
        "t103-unstarted-generator-parameter" => (TrapKind::UncaughtException, 22, 5),
        "t104-coroutine-closure-environment" => (TrapKind::UncaughtException, 21, 61),
        "t100-conditional-await-collect" => (TrapKind::UncaughtException, 17, 5),
        "t99-loop-result-collect" => (TrapKind::UncaughtException, 13, 3),
        "t83-unobserved-aggregate-exception" => (TrapKind::UncaughtException, 10, 40),
        "t82-worker-trap-site" => (TrapKind::WorkerTrapped, 16, 14),
        "t114-worker-spawn-in-worker" => (TrapKind::WorkerTrapped, 13, 35),
        "t115-worker-spawn-in-initializer" => (TrapKind::WorkerTrapped, 13, 36),
        other => panic!("{other}: trap corpus entry has no exact expectation"),
    }
}

/// The exact message of each regular-expression trap entry.
pub fn regex_error_message(id: &str) -> Option<&'static str> {
    Some(match id {
        "t40-regex-invalid-pattern" => "invalid regular expression: Unbalanced parenthesis",
        "t41-regex-unsupported-flag" => {
            "unsupported regular-expression flag `q`; supported flags are d, g, i, m, s, u, v"
        }
        "t42-regex-duplicate-flag" => "duplicate regular-expression flag `g`",
        "t43-regex-mutually-exclusive-flags" => {
            "regular-expression flags `u` and `v` are mutually exclusive"
        }
        "t44-regex-replace-all-without-global" => {
            "string.replaceAll with a RegExp requires the `g` flag"
        }
        "t45-regex-sticky-last-index" => {
            "sticky regular-expression flag `y` requires mutable `RegExp.lastIndex`, which is not in the language"
        }
        _ => return None,
    })
}

/// The Context allocation that each allocation-failure entry rejects.
pub fn allocation_failure_count(id: &str) -> Option<u64> {
    match id {
        "t26-allocation-failure-new" | "t28-allocation-failure-array-literal" => Some(2),
        "t29-allocation-failure-push-grow" => Some(3),
        "t30-allocation-failure-string-concat" => Some(4),
        "t31-allocation-failure-template" => Some(5),
        "t32-allocation-failure-generator-frame" => Some(2),
        "t33-allocation-failure-json-raw-new" => Some(4),
        "t35-allocation-failure-map-new" | "t36-allocation-failure-set-new" => Some(2),
        "t37-allocation-failure-map-grow" | "t38-allocation-failure-set-grow" => Some(3),
        _ => None,
    }
}

/// Q6/§8.1b: the double-delete and use-after-delete entries run the dev
/// tier with freed-handle diagnostics and post-run accounting.
pub fn freed_handle_diagnostic(id: &str) -> bool {
    matches!(id, "t22-double-delete-q6" | "t23-use-after-delete-q6")
}

/// §14.4b (A) pins its trap with freed-handle diagnostics on, for the
/// Context-lifetime binding and for the §111 registration alike.
pub fn callback_userdata_diagnostic(id: &str) -> bool {
    matches!(
        id,
        "t46-callback-userdata-freed" | "t59-registration-userdata-freed"
    )
}

/// The exact message that a trap entry states, beside the regular-expression
/// messages of [`regex_error_message`].
pub fn trap_message(id: &str) -> Option<&'static str> {
    match id {
        "t68-narrowing-accessor-compound"
        | "t69-narrowing-static-accessor-compound"
        | "t70-narrowing-optional-getter"
        | "t71-narrowing-destructuring-getter"
        | "t73-boxed-local-narrowing-call"
        | "t74-boxed-local-narrowing-alias-store"
        | "t72-narrowing-boundary-getter" => Some("a narrowed shared location is null"),
        "t22-double-delete-q6" => Some("Context.free of an already-deleted allocation"),
        "t23-use-after-delete-q6" => Some("use of a deleted allocation"),
        "t46-callback-userdata-freed" | "t59-registration-userdata-freed" => {
            Some("callback userdata points to a freed allocation")
        }
        "t60-registration-fire-after-release" => {
            Some("a callback fired through a registration the host released")
        }
        "t48-wire-enum-unknown-value" => {
            Some("unknown wire value 12345 for CEnum alias `SubWireMode`")
        }
        "t49-wire-enum-struct-unknown-member" => {
            Some("unknown wire value 12345 for CEnum alias `SubWireMode`")
        }
        "t50-wire-entry-unknown-value" => {
            Some("unknown wire value 12345 for CEnum alias `SubWireMode`")
        }
        "t51-bytes-into-range" => {
            Some("byte range at offset 5 with size 16 exceeds array length 20")
        }
        "t105-dropped-generator-break"
        | "t106-dropped-generator-local"
        | "t94-counted-map-delete"
        | "t95-counted-map-clear"
        | "t96-counted-map-replace"
        | "t99-loop-result-collect"
        | "t100-conditional-await-collect"
        | "t101-unused-coroutine-parameter"
        | "t102-generator-parameter"
        | "t103-unstarted-generator-parameter"
        | "t104-coroutine-closure-environment"
        | "t116-detected-task-failure" => Some("Error: lost"),
        "t97-counted-field-collect"
        | "t98-counted-field-task-order"
        | "t107-collected-task-order"
        | "t117-detected-task-order" => Some("Error: first"),
        "t118-unreachable-completed-task-failure" => Some("Error: cycle"),
        "t108-host-operation-dropped-error" => Some("Error: host failure"),
        "t109-generator-close-throws" => Some("Error: close"),
        "t110-pad-start-cut-inside-sequence" => {
            Some("padStart(2): the cut is at pad byte 1, inside a UTF-8 sequence")
        }
        "t111-pad-end-cut-inside-sequence" => {
            Some("padEnd(2): the cut is at pad byte 1, inside a UTF-8 sequence")
        }
        "t83-unobserved-aggregate-exception" => Some("Error: aggregate dropped"),
        "t114-worker-spawn-in-worker" | "t115-worker-spawn-in-initializer" => {
            Some("worker trapped with worker-spawn-in-worker: a Worker cannot spawn a Worker")
        }
        "t82-worker-trap-site" => Some(
            "worker trapped with index-out-of-bounds: index 5 out of bounds for array length 1",
        ),
        _ => None,
    }
}

/// The native libraries of a trap entry, or `None` when the entry needs the
/// interop fixture and the fixture is not available here.
pub fn trap_native_libraries(
    files: &[SourceFile],
) -> Option<Vec<subscript_codegen::NativeLibrary>> {
    match crate::native_fixture::fixture() {
        Some(fixture) => Some(vec![fixture.library()]),
        None if files.iter().any(|source| {
            source.source.contains("subDevice")
                || source.source.contains("subWireMode")
                || source.source.contains("subBindTone")
        }) =>
        {
            None
        }
        None => Some(Vec::new()),
    }
}

/// The reason that the two-tier trap sweeps do not run an entry, if any.
///
/// A stale coroutine exists only after two runs and a hot reload.
/// `reload.rs` drives this paired corpus source through that dev-tier-only
/// mode; a shipped C binary has no body-swap mode.
pub fn sweep_exclusion(id: &str) -> Option<&'static str> {
    match id {
        "t24-stale-coroutine-reload" => Some("a stale coroutine exists only after a hot reload"),
        "t34-allocation-failure-unrepresentable-policy" => {
            Some("a policy record with no runnable program")
        }
        _ => None,
    }
}
