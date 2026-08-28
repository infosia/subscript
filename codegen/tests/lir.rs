//! §68 step 1: every accepted corpus program lowers to verified LIR.

#[allow(dead_code)]
mod corpus;
#[path = "support/lir_facts.rs"]
mod lir_facts;
#[cfg(debug_assertions)]
#[allow(dead_code)]
#[path = "support/trap_corpus.rs"]
mod trap_corpus;

use subscript_codegen::interpreter::interpret;
use subscript_codegen::lir::{lower_module, verify_module};
use subscript_compiler::lir::{
    BlockId, ForOfKind, InstructionKind, Module, Operand, Terminator, TrapKind, ValueType,
};
use subscript_compiler::lir_text::print_module;
use subscript_compiler::Type;
use subscript_compiler::{check_program, SourceFile};

fn lower_entry(accept: &std::path::Path, id: &str) -> Module {
    let sources = corpus::entry_sources(accept, id);
    let hir = check_program(&sources)
        .unwrap_or_else(|diagnostics| panic!("{id}: checker rejected: {diagnostics:?}"));
    lower_module(&hir).unwrap_or_else(|error| panic!("{id}: lower failed: {error}"))
}

#[test]
fn every_hir_execution_fact_is_carried_by_lir() {
    let accept = corpus::corpus_accept();
    let mut findings = Vec::new();
    for id in corpus::entry_ids(&accept) {
        let sources = corpus::entry_sources(&accept, &id);
        let hir = check_program(&sources)
            .unwrap_or_else(|diagnostics| panic!("{id}: checker rejected: {diagnostics:?}"));
        let lir = lower_module(&hir).unwrap_or_else(|error| panic!("{id}: lower failed: {error}"));
        findings.extend(
            lir_facts::dropped_facts(&hir, &lir)
                .into_iter()
                .map(|finding| format!("{id}: {finding}")),
        );
    }
    assert!(
        findings.is_empty(),
        "LIR dropped {} HIR execution fact(s):\n{}",
        findings.len(),
        findings.join("\n")
    );
}

#[test]
fn item_12_reports_a_missing_foreign_array_snapshot_pair() {
    let accept = corpus::corpus_accept();
    let sources = corpus::entry_sources(&accept, "a26-interop-array-pair");
    let hir = check_program(&sources).expect("a26 checks clean");
    let mut lir = lower_module(&hir).expect("a26 lowers to LIR");
    let snapshot = lir
        .functions
        .iter_mut()
        .flat_map(|function| &mut function.blocks)
        .flat_map(|block| &mut block.instructions)
        .find(|instruction| instruction.kind == InstructionKind::ForeignArrayData)
        .expect("a26 has a foreign array snapshot");
    snapshot.kind = InstructionKind::Copy;
    let findings = lir_facts::dropped_facts(&hir, &lir);
    assert!(findings
        .iter()
        .any(|finding| finding.contains("foreign array argument carries no data/count snapshot")));
}

#[test]
fn item_12_compares_host_entry_traps_in_both_directions() {
    let source = "type WireMode = CEnum<{ \"m0\": 16; \"m1\": 23 }>;\n\
                  export function configure(mode: WireMode): void {}\n\
                  export function main(): void {}\n";
    let hir = check_program(&[SourceFile::new("wire-entry.ts", source)])
        .expect("wire entry checks cleanly");
    let mut lir = lower_module(&hir).expect("wire entry lowers to LIR");
    let trap = lir
        .functions
        .iter_mut()
        .find(|function| function.source_name == "configure")
        .expect("configure LIR function")
        .host_entry_traps
        .as_mut()
        .expect("configure host entry")
        .pop()
        .expect("wire validation trap");
    let missing = lir_facts::dropped_facts(&hir, &lir);
    assert!(missing
        .iter()
        .any(|finding| finding.contains("carries 0 site(s); HIR requires 1")));

    lir.functions
        .iter_mut()
        .find(|function| function.source_name == "configure")
        .expect("configure LIR function")
        .host_entry_traps
        .as_mut()
        .expect("configure host entry")
        .extend([trap.clone(), trap]);
    let extra = lir_facts::dropped_facts(&hir, &lir);
    assert!(extra
        .iter()
        .any(|finding| finding.contains("carries 2 site(s); HIR requires 1")));
}

#[test]
fn embedded_boundary_header_constructor_keeps_enclosing_place_address() {
    let sources = [
        SourceFile::ambient(
            "embedded-header.generated.d.ts",
            r#"
// @subscript-c-header include="embedded-header.h"
declare enum HeaderKind {
  TICK = 1,
  LIMIT = 2,
}
declare class Header {
  kind: HeaderKind;
  next: Header | null;
  constructor(kind: HeaderKind, next: Header | null);
}
declare class Limit {
  header: Header;
  maximum: u32;
  constructor(header: Header, maximum: u32);
}
declare class Tick {
  header: Header;
  count: u32;
  constructor(header: Header, count: u32);
}
"#,
        ),
        SourceFile::new(
            "embedded-header.ts",
            r#"
export function main(): void {
  const limit: Limit = new Limit(new Header(HeaderKind.LIMIT, null), 3);
  const tick: Tick = new Tick(
    new Header(HeaderKind.TICK, limit.header),
    2,
  );
  print(`${tick.count}`);
}
"#,
        ),
    ];
    let hir = check_program(&sources).expect("embedded-header shape checks clean");
    let lir = lower_module(&hir).expect("embedded-header shape lowers to LIR");
    verify_module(&lir).expect("embedded-header LIR verifies");
    let findings = lir_facts::dropped_facts(&hir, &lir);
    assert!(
        findings.is_empty(),
        "embedded header lost an execution fact:\n{}",
        findings.join("\n")
    );
}

/// Corpus programs whose observable result depends on a host facility the
/// reference interpreter cannot supply. The reason is deliberately repeated
/// per entry: adding a new program can never inherit a broad silent skip.
const INTERPRETER_EXCLUSIONS: &[(&str, &str)] = &[
    (
        "a25-interop-chain",
        "calls the synthetic native interop library",
    ),
    (
        "a26-interop-array-pair",
        "calls the synthetic native interop library",
    ),
    (
        "a27-interop-string-view",
        "calls the synthetic native interop library",
    ),
    (
        "a28-interop-callback",
        "calls the synthetic native interop library",
    ),
    (
        "a29-interop-handle",
        "calls the synthetic native interop library",
    ),
    (
        "a30-interop-compose",
        "calls the synthetic native interop library",
    ),
    (
        "a31-interop-primitive-slices",
        "calls the synthetic native interop library",
    ),
    (
        "a32-interop-embedded-array",
        "calls the synthetic native interop library",
    ),
    (
        "a33-interop-flags",
        "calls the synthetic native interop library",
    ),
    (
        "a34-interop-bulk-facade",
        "calls the synthetic native interop library",
    ),
    (
        "a35-interop-async",
        "calls the synthetic native interop library",
    ),
    (
        "a36-interop-chained-flags",
        "calls the synthetic native interop library",
    ),
    (
        "a37-interop-struct-return",
        "calls the synthetic native interop library",
    ),
    (
        "a38-interop-out-field",
        "calls the synthetic native interop library",
    ),
    (
        "a39-interop-async-capstone",
        "calls the synthetic native interop library",
    ),
    (
        "a48-interop-narrow-slices",
        "calls the synthetic native interop library",
    ),
    (
        "a89-interop-chain-payload",
        "calls the synthetic native interop library",
    ),
    (
        "a90-callback-userdata-rooted",
        "registers a callback with the synthetic native interop library",
    ),
    (
        "a95-interop-async-await",
        "calls the synthetic native interop library",
    ),
    (
        "a96-interop-byte-pairs",
        "calls the synthetic native interop library",
    ),
    (
        "a97-interop-string-field-write",
        "calls the synthetic native interop library",
    ),
    (
        "a98-interop-string-field-read",
        "calls the synthetic native interop library",
    ),
    (
        "a99-interop-texture-descriptor-write",
        "calls the synthetic native interop library",
    ),
    (
        "a100-interop-texture-descriptor-read",
        "calls the synthetic native interop library",
    ),
    (
        "a101-interop-handle-array-pair",
        "calls the synthetic native interop library",
    ),
    (
        "a102-interop-nullable-handle-fields",
        "calls the synthetic native interop library",
    ),
    (
        "a103-interop-recursive-compute-pipeline",
        "calls the synthetic native interop library",
    ),
    (
        "a104-interop-recursive-render-pipeline",
        "calls the synthetic native interop library",
    ),
    (
        "a105-interop-recursive-string-pair-elements",
        "calls the synthetic native interop library",
    ),
    (
        "a106-interop-recursive-struct-pointer-members",
        "calls the synthetic native interop library",
    ),
    (
        "a159-address-keeps-base-alive",
        "calls the synthetic native interop library",
    ),
    (
        "a107-interop-handle-parameter-pair",
        "calls the synthetic native interop library",
    ),
    (
        "a108-interop-nullable-handle-parameter",
        "calls the synthetic native interop library",
    ),
    (
        "a109-interop-null-only-boundary-reference",
        "calls the synthetic native interop library",
    ),
    (
        "a111-interop-async-method-poll",
        "calls the synthetic native interop library",
    ),
    (
        "a112-worker-echo",
        "requires a runtime worker adapter and a second interpreter Context",
    ),
    (
        "a113-worker-parallel",
        "requires runtime worker adapters and child interpreter Contexts",
    ),
    (
        "a119-interop-handle-beside-arrays",
        "calls the synthetic native interop library",
    ),
    (
        "a120-interop-nested-behind-element-pointer",
        "calls the synthetic native interop library",
    ),
    (
        "a121-interop-unmarked-reach-through",
        "calls the synthetic native interop library",
    ),
    (
        "a122-interop-two-pointer-members",
        "calls the synthetic native interop library",
    ),
    (
        "a123-interop-wide-descriptor",
        "calls the synthetic native interop library",
    ),
    (
        "a124-contextual-conditional",
        "calls the synthetic native interop library for boundary-handle arms",
    ),
    (
        "a125-conditional-arm-narrowing",
        "calls the synthetic native interop library for boundary-handle arms",
    ),
    (
        "a126-interop-by-value-packing",
        "calls the synthetic native interop library",
    ),
    (
        "a127-interop-external-type",
        "calls two synthetic native interop libraries",
    ),
    (
        "a128-host-owned-state",
        "requires host pre-entry and post-run hooks",
    ),
    (
        "a129-interop-wire-enum",
        "calls the synthetic native interop library",
    ),
    (
        "a130-interop-wire-enum-bind",
        "calls the synthetic native interop library",
    ),
    (
        "a131-interop-wire-enum-struct",
        "calls the synthetic native interop library",
    ),
    (
        "a137-handle-entry-param",
        "exported main requires a host-supplied handle",
    ),
    (
        "a140-wire-entry-param",
        "exported main requires a host-supplied wire value",
    ),
    (
        "a149-suspension-state",
        "reaches the synthetic native interop library after its suspension checks",
    ),
    (
        "a153-nested-cstruct-array-roundtrip",
        "dynamic arrays of nested value aggregates are outside the reference interpreter's represented value set",
    ),
];

const RELEASE_RUNNABLE_COUNT: usize = 105;
const DEBUG_RUNNABLE_COUNT: usize = 104;
const FULL_INTERPRETER_SWEEP_ENV: &str = "SUBSCRIPT_FULL_INTERPRETER_SWEEP";
const DEBUG_COST_EXCLUSIONS: &[(&str, &str)] = &[(
    "a22-matrix-propagation",
    "its purpose is benchmark cost; one million matrix multiplications add no interpreter semantics",
)];

/// The debug profile checks semantics that Rust debug checks can affect.
/// Rust checks integer overflow in debug, but Subscript arithmetic always
/// wraps. The list also checks stateful interpreter protocols in debug.
///
/// The interpreter can run `a22-matrix-propagation`, and the release sweep
/// runs it. Debug omits it because its purpose is benchmark cost. One million
/// matrix multiplications add cost, but they do not add interpreter semantics.
/// No other runnable entry is outside this declared debug subset.
const DEBUG_INTERPRETER_SUBSET: &[(&str, &str)] = &[
    (
        "a01-hello",
        "exported entry, host output, and string literals",
    ),
    (
        "a02-integer-types",
        "baseline signed/unsigned arithmetic and explicit numeric conversions",
    ),
    (
        "a03-integer-literals",
        "contextual integer literals in bindings, calls, and arrays",
    ),
    (
        "a04-value-struct",
        "value-aggregate field access and copy-on-assignment",
    ),
    (
        "a05-nominal-identity",
        "nominal identity for same-shape value aggregates",
    ),
    (
        "a06-fixed-array",
        "fixed-array storage nested in a value aggregate",
    ),
    (
        "a07-slice-pair",
        "dynamic-array length, indexed reads, loops, and slice parameters",
    ),
    (
        "a08-string-view",
        "string length, slicing, parameters, and returned handles",
    ),
    (
        "a09-enums",
        "numeric enum values, parameters, and comparisons",
    ),
    (
        "a10-control-flow",
        "if, while, for, switch, break, and continue edges",
    ),
    (
        "a11-functions",
        "direct calls, default parameters, and return values",
    ),
    (
        "a110-async-method-receiver",
        "method receiver state and roots live across suspension and collection",
    ),
    (
        "a114-lambda-env-recursion",
        "capturing closure environments across recursive re-entry",
    ),
    (
        "a115-switch-literal-union",
        "string-alias values and integer-dispatched switch arms",
    ),
    (
        "a116-exhaustive-switch-returns",
        "exhaustive switch returns and unreachable control-flow construction",
    ),
    (
        "a117-descriptor-literal-nullable-member",
        "nullable nested descriptor aggregates, defaults, and arrays",
    ),
    (
        "a118-absence-capable-member",
        "descriptor omission, presence narrowing, and string aliases",
    ),
    (
        "a12-generics-mono",
        "generic functions and value aggregates at multiple instantiations",
    ),
    (
        "a13-closures-noncapture",
        "noncapturing function values and indirect calls",
    ),
    (
        "a132-int-literal-64bit",
        "full-range i64/u64 literals and numeric separators",
    ),
    (
        "a133-field-init-no-ctor",
        "field initializers for value and reference classes without constructors",
    ),
    (
        "a134-field-init-order",
        "constructor argument, field initializer, and body evaluation order",
    ),
    ("a135-f32-bits", "binary32 bit conversion and canonical NaN"),
    (
        "a136-index-signature",
        "generic class index reads and writes through accessors",
    ),
    (
        "a138-using-dispose",
        "scope exits and reverse-order synchronous disposal",
    ),
    (
        "a139-using-async",
        "a disposable binding carried as an SSA live-in across suspension",
    ),
    (
        "a14-closures-capture",
        "capturing closure creation, storage, and indirect calls",
    ),
    (
        "a141-cstruct-align",
        "aligned value aggregates, nested copies, and fixed-array stride",
    ),
    (
        "a142-bytes-of",
        "aligned value aggregates, fixed arrays, padding, and byte-copy intrinsics",
    ),
    (
        "a143-async-generic",
        "generic async functions and methods at multiple instantiations",
    ),
    (
        "a144-accessor",
        "read and write accessor calls on reference, value, and generic classes",
    ),
    (
        "a145-emitted-identifiers",
        "coroutine live values under dense identifier reuse",
    ),
    (
        "a146-scoped-locals",
        "scoped values across generators, async, for-of, switch, lambdas, and using",
    ),
    (
        "a147-switch-body-scope",
        "one switch-body scope, distinct declarations, and fallthrough",
    ),
    (
        "a148-switch-using-scope",
        "resource disposal across switch fallthrough and scope exit",
    ),
    (
        "a150-receiver-address-invalidation",
        "receiver address recomputation after dynamic-array storage invalidation",
    ),
    (
        "a151-lambda-env-outlives-block",
        "capturing closure environment storage beyond its source block",
    ),
    (
        "a152-lambda-env-per-iteration",
        "distinct loop-iteration closure environments across suspension",
    ),
    (
        "a154-held-async-handle",
        "held async-handle creation, delayed polling, and deterministic completion order",
    ),
    (
        "a155-async-handle-array",
        "async-handle array storage, indexed held awaits, and scope release",
    ),
    (
        "a156-cstruct-this-by-value",
        "value-class receiver loads for by-value returns and arguments",
    ),
    (
        "a157-await-loop-liveness",
        "await-result liveness through a loop and a later suspension",
    ),
    (
        "a160-module-initializer-order",
        "module initializers and entry calls read initialized data bindings",
    ),
    (
        "a15-manual-lifetime",
        "reference allocation, field access, and explicit free",
    ),
    (
        "a16-explicit-collect",
        "dropped references and explicit collection",
    ),
    (
        "a17-null-story",
        "nullable parameters, fields, branches, and narrowed references",
    ),
    (
        "a18-error-handling",
        "result aggregates and guarded integer division",
    ),
    (
        "a19-modules",
        "multi-file imports, exported calls, and module initialization",
    ),
    (
        "a20-coroutine-generator",
        "generator creation, yield, resume, and completion",
    ),
    (
        "a21-methods",
        "value and reference method receivers and calls",
    ),
    (
        "a23-game-loop",
        "bounded loops over arrays of value aggregates",
    ),
    (
        "a24-particle-system",
        "array-of-struct and struct-of-arrays aggregate updates",
    ),
    (
        "a40-math",
        "Math intrinsics, constants, float edges, and formatting",
    ),
    (
        "a41-math-random",
        "deterministic Context random-number state",
    ),
    (
        "a42-date",
        "Date construction, accessors, formatting, arrays, and reference fields",
    ),
    (
        "a43-string",
        "string search, split, trim, padding, case, and replacement intrinsics",
    ),
    (
        "a44-array",
        "array equality, search, slice, fill, reverse, and concatenation",
    ),
    (
        "a45-array-fn",
        "array callbacks, changed element types, folds, and short-circuit traversal",
    ),
    (
        "a46-narrow-numerics",
        "i8/u8/i16/u16/f16 conversion, wrapping arithmetic, and bitwise operations",
    ),
    (
        "a47-narrow-layout",
        "mixed-width fields, aggregate layout, and copy-on-assignment",
    ),
    (
        "a49-f16-conversions",
        "binary16 rounding, overflow, subnormal, NaN, and signed-zero conversions",
    ),
    (
        "a50-narrow-callbacks-shifts",
        "narrow callback extension and masked shifts at every integer width, including u64 wrap",
    ),
    (
        "a51-map",
        "map operations, aggregate values, nullable lookup, and collection",
    ),
    (
        "a52-map-order",
        "map insertion order across overwrite, removal, and reinsertion",
    ),
    ("a53-set", "set operations and SameValueZero float keys"),
    (
        "a54-map-reference-key",
        "reference identity for map keys across mutation",
    ),
    (
        "a55-map-set-foreach",
        "map/set callbacks and callback-owned trap sites",
    ),
    (
        "a56-map-aggregate-foreach",
        "value-class and fixed-array copy semantics across callbacks",
    ),
    (
        "a57-number",
        "Number constants and typed numeric predicates",
    ),
    (
        "a58-number-parse",
        "integer and float parsing, casts, and parse-failure values",
    ),
    (
        "a59-number-to-fixed",
        "fixed decimal formatting, rounding, signs, and exponent fallback",
    ),
    (
        "a60-string-unicode",
        "Unicode case conversion and whitespace trimming",
    ),
    (
        "a61-same-value-zero",
        "NaN and negative-zero equality in arrays, maps, and sets",
    ),
    (
        "a62-number-formatting-clz32",
        "radix and precision formatting plus zero-defined clz32",
    ),
    (
        "a63-q27-math-number",
        "overflowing i32 Math.imul and binary32 rounding",
    ),
    (
        "a64-q27-string",
        "substring, code points, concatenation, positions, and replacement substitutions",
    ),
    (
        "a65-q27-array",
        "array callbacks and structural mutation operations",
    ),
    (
        "a66-q27-map-set",
        "Map.groupBy, Set algebra, callbacks, and insertion order",
    ),
    (
        "a67-q27-array-callback-index",
        "array callback arities and index argument order",
    ),
    (
        "a68-q27-fixed-array-callbacks",
        "fixed-array callback arities and dynamic result arrays",
    ),
    (
        "a69-json-stringify",
        "JSON serialization of scalars, arrays, dates, and aggregates",
    ),
    (
        "a70-json-roundtrip",
        "typed JSON parse and serialization round-trips for aggregates",
    ),
    (
        "a71-json-parse",
        "typed JSON parse failures, duplicate keys, and numeric ranges",
    ),
    (
        "a72-json-parse-limits",
        "JSON depth, UTF-8, and f32 representation failures",
    ),
    (
        "a73-p19-divisor-single-eval",
        "single evaluation of a call-valued integer divisor",
    ),
    (
        "a74-p20-string-array-compound",
        "string-array indexed compound assignment",
    ),
    (
        "a75-p20-array-compound-expression",
        "integer-array compound assignment in expression position",
    ),
    (
        "a76-p20-dynamic-value-field-write",
        "dynamic value-aggregate fields, index side effects, and address provenance",
    ),
    (
        "a77-for-of-containers",
        "array, fixed-array, map, set, and Unicode iteration cursors",
    ),
    (
        "a78-for-of-views",
        "array, map, and set key/value iteration views",
    ),
    (
        "a79-for-of-generator",
        "generator suspension composed with the for-of protocol",
    ),
    (
        "a80-for-of-foreach-mutation",
        "captured iteration bounds and removal/appending mutation semantics",
    ),
    (
        "a81-array-literal-spread",
        "array spread from arrays, fixed arrays, maps, sets, and strings",
    ),
    (
        "a82-regex",
        "regular-expression matches, captures, search, replacement, and split",
    ),
    (
        "a83-regex-review",
        "regular-expression source, flags, collection roots, and non-BMP behavior",
    ),
    (
        "a84-for-of-bmp",
        "BMP string iteration with static code-point handles",
    ),
    (
        "a85-for-of-repeated-astral",
        "repeated astral string iteration and handle interning",
    ),
    (
        "a86-for-of-mixed-unicode",
        "mixed BMP and astral string iteration representations",
    ),
    (
        "a87-for-of-distinct-astral",
        "distinct astral code-point handles and iteration order",
    ),
    (
        "a88-astral-intern-collect",
        "astral string roots across explicit collection",
    ),
    (
        "a91-string-literal-union",
        "string aliases in parameters, fields, returns, and arrays",
    ),
    (
        "a92-descriptor-literals",
        "descriptor defaults, nesting, arrays, and member initialization",
    ),
    (
        "a93-async-chain",
        "nested async calls, suspension propagation, and resume values",
    ),
    (
        "a94-async-two-roots",
        "standard-runner kick and pump order for multiple async roots",
    ),
];

/// Reachable trap semantics kept in the debug profile beside the accepted
/// subset. Each entry proves both the trap kind/site and trap-stop stdout.
#[cfg(debug_assertions)]
const DEBUG_INTERPRETER_TRAPS: &[(&str, &str, &str, u32)] = &[
    (
        "t08-div-zero-expression",
        "integer division-by-zero check in expression position",
        "DivisionByZero",
        10,
    ),
    (
        "t16-array-write-oob",
        "checked dynamic-array write address and stop-before-write behavior",
        "index-out-of-bounds",
        11,
    ),
    (
        "t47-unreachable-reached",
        "explicit unreachable terminator and trap-stop behavior",
        "Unreachable",
        10,
    ),
];

#[test]
fn lir_interpreter_profile_matches_corpus_goldens() {
    let started = std::time::Instant::now();
    let accept = corpus::corpus_accept();
    let entries = corpus::golden_ids(&accept);
    assert_eq!(
        INTERPRETER_EXCLUSIONS.len(),
        54,
        "the declared host-dependent exclusion count changed"
    );
    for (id, reason) in INTERPRETER_EXCLUSIONS {
        assert!(
            entries.iter().any(|entry| entry == id),
            "declared interpreter exclusion {id} has no corpus golden"
        );
        assert!(
            !reason.trim().is_empty(),
            "declared interpreter exclusion {id} has no reason"
        );
    }

    let runnable = entries
        .iter()
        .filter(|id| {
            !INTERPRETER_EXCLUSIONS
                .iter()
                .any(|(excluded, _)| excluded == *id)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        runnable.len(),
        RELEASE_RUNNABLE_COUNT,
        "release runnable corpus count changed"
    );

    if !cfg!(debug_assertions) && std::env::var_os(FULL_INTERPRETER_SWEEP_ENV).is_none() {
        eprintln!(
            "interpreter release corpus: skipped {RELEASE_RUNNABLE_COUNT} runnable entries; set {FULL_INTERPRETER_SWEEP_ENV}=1 for the full sweep"
        );
        return;
    }

    let selected = if cfg!(debug_assertions) {
        let mut selected = Vec::with_capacity(DEBUG_INTERPRETER_SUBSET.len());
        for (id, reason) in DEBUG_INTERPRETER_SUBSET {
            assert!(!reason.trim().is_empty(), "debug subset {id} has no reason");
            assert!(
                runnable.iter().any(|entry| entry.as_str() == *id),
                "debug subset {id} is not a runnable corpus entry"
            );
            assert!(
                !selected.iter().any(|selected| selected == id),
                "debug subset contains duplicate {id}"
            );
            selected.push(*id);
        }
        assert_eq!(
            selected.len(),
            DEBUG_RUNNABLE_COUNT,
            "debug runnable corpus count changed"
        );
        for (id, reason) in DEBUG_COST_EXCLUSIONS {
            assert!(
                !reason.trim().is_empty(),
                "debug cost exclusion {id} has no reason"
            );
            assert!(
                runnable.iter().any(|entry| entry.as_str() == *id),
                "debug cost exclusion {id} is not a runnable corpus entry"
            );
        }
        let outside = runnable
            .iter()
            .filter(|id| !selected.iter().any(|selected| selected == &id.as_str()))
            .map(|id| id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            outside,
            DEBUG_COST_EXCLUSIONS
                .iter()
                .map(|(id, _)| *id)
                .collect::<Vec<_>>(),
            "only declared cost-purpose entries can be outside the debug subset"
        );
        selected
    } else {
        runnable.iter().map(|id| id.as_str()).collect()
    };

    let mut ran = 0usize;
    let mut matched = 0usize;
    let mut findings = Vec::new();
    for id in selected {
        ran += 1;
        let module = lower_entry(&accept, id);
        let golden = corpus::golden_bytes(&accept, id);
        match interpret(&module) {
            Ok(output) if output == golden => matched += 1,
            Ok(output) => findings.push(format!(
                "{id}: output mismatch\n  interpreter: {:?}\n  golden:      {:?}",
                String::from_utf8_lossy(&output),
                String::from_utf8_lossy(&golden)
            )),
            Err(error) => findings.push(format!(
                "{id}: interpreter error: {error}\n  interpreter: {:?}\n  golden:      {:?}",
                String::from_utf8_lossy(error.output()),
                String::from_utf8_lossy(&golden)
            )),
        }
    }
    assert!(
        findings.is_empty(),
        "interpreter {profile} corpus: {ran} run, {matched} matched, {} findings, {} declared exclusions\n{}",
        findings.len(),
        INTERPRETER_EXCLUSIONS.len(),
        findings.join("\n"),
        profile = if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        },
    );
    eprintln!(
        "interpreter {profile} corpus: {ran} run, {matched} matched, {} declared exclusions, {:.3} s",
        INTERPRETER_EXCLUSIONS.len(),
        started.elapsed().as_secs_f64(),
        profile = if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        },
    );
}

#[cfg(debug_assertions)]
#[test]
fn lir_interpreter_debug_subset_traps_at_declared_sites() {
    let trap = trap_corpus::corpus_trap();
    let entries = trap_corpus::trap_ids(&trap);
    for (id, reason, expected_kind, expected_line) in DEBUG_INTERPRETER_TRAPS {
        assert!(!reason.trim().is_empty(), "debug trap {id} has no reason");
        assert!(
            entries.iter().any(|entry| entry == id),
            "debug trap subset {id} has no trap corpus entry"
        );
        let sources = trap_corpus::trap_sources(&trap, id);
        let hir = check_program(&sources)
            .unwrap_or_else(|diagnostics| panic!("{id}: checker rejected: {diagnostics:?}"));
        let module =
            lower_module(&hir).unwrap_or_else(|error| panic!("{id}: lower failed: {error}"));
        let error = interpret(&module).expect_err("debug trap entry must trap");
        assert_eq!(
            error.output(),
            trap_corpus::trap_expected(&trap, id),
            "{id}: pre-trap stdout differs from the golden"
        );
        let subscript_codegen::interpreter::InterpretError::Execution { source, .. } = error else {
            panic!("{id}: interpreter did not preserve pre-trap execution output");
        };
        let subscript_codegen::interpreter::InterpretError::Trap { kind, pos, .. } = *source else {
            panic!("{id}: interpreter did not report a semantic trap");
        };
        assert_eq!(kind, *expected_kind, "{id}: wrong trap kind");
        assert_eq!(pos.file, format!("{id}.ts"), "{id}: wrong trap file");
        assert_eq!(pos.line, *expected_line, "{id}: wrong trap line");
    }
}

#[test]
fn suspension_capstone_reaches_its_declared_native_boundary() {
    let accept = corpus::corpus_accept();
    let id = "a149-suspension-state";
    let module = lower_entry(&accept, id);
    let error = interpret(&module).expect_err("a149 requires the native fixture");
    assert!(error
        .to_string()
        .contains("subDeviceCreate requires a native library"));
    let golden = corpus::golden_bytes(&accept, id);
    assert!(golden.starts_with(error.output()));
    assert!(String::from_utf8_lossy(error.output()).ends_with("machinery:from-bytes=1,2\n"));
}

fn successors(terminator: &Terminator) -> Vec<BlockId> {
    match terminator {
        Terminator::Branch(target) => vec![target.block],
        Terminator::ConditionalBranch {
            then_target,
            else_target,
            ..
        } => vec![then_target.block, else_target.block],
        Terminator::Switch { arms, default, .. } => arms
            .iter()
            .map(|arm| arm.target.block)
            .chain(std::iter::once(default.block))
            .collect(),
        Terminator::Suspend { successor, .. } => vec![*successor],
        Terminator::Return { .. } | Terminator::Unreachable { .. } | Terminator::Trap(_) => {
            Vec::new()
        }
    }
}

#[test]
fn every_corpus_entry_lowers_to_verified_lir() {
    let accept = corpus::corpus_accept();
    let entries = corpus::entry_ids(&accept);
    assert!(!entries.is_empty(), "accept corpus is empty");

    let mut verified_functions = 0_usize;
    let mut instruction_count = 0_usize;
    let mut local_traffic = 0_usize;
    let mut local_count = 0_usize;
    let mut block_parameters = 0_usize;
    let mut coroutine_functions = 0_usize;
    let mut coroutine_local_after_resume = 0_usize;
    let mut checked_index_addresses = 0_usize;
    let mut coroutine_creation_allocations = 0_usize;
    for id in &entries {
        let lir = lower_entry(&accept, id);
        verify_module(&lir).unwrap_or_else(|errors| {
            panic!(
                "{id}: verifier failed:\n{}",
                errors
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        });
        verified_functions += lir.functions.len();
        for function in &lir.functions {
            local_count += function.locals.len();
            block_parameters += function
                .blocks
                .iter()
                .map(|block| block.parameters.len())
                .sum::<usize>();
            for instruction in function.blocks.iter().flat_map(|block| &block.instructions) {
                instruction_count += 1;
                if matches!(
                    instruction.kind,
                    InstructionKind::LoadLocal(_)
                        | InstructionKind::StoreLocal(_)
                        | InstructionKind::AddressOfLocal(_)
                ) {
                    local_traffic += 1;
                }
                if matches!(
                    instruction.kind,
                    InstructionKind::AddressOfIndex { checked: true }
                ) {
                    checked_index_addresses += 1;
                    assert!(
                        instruction.traps.iter().any(|trap| matches!(
                            trap.kind,
                            TrapKind::IndexRead | TrapKind::IndexWrite
                        )),
                        "{id}: checked AddressOfIndex at {} carries no index trap",
                        instruction.pos
                    );
                }
            }
            if function.is_generator || function.is_async {
                assert!(
                    matches!(
                        function.creation_traps.as_slice(),
                        [trap] if trap.kind == TrapKind::Allocation
                    ),
                    "{id}: coroutine `{}` does not carry exactly one creation Allocation trap: {:?}",
                    function.source_name,
                    function.creation_traps
                );
                coroutine_creation_allocations += 1;
            } else {
                assert!(
                    function.creation_traps.is_empty(),
                    "{id}: ordinary function `{}` carries creation traps",
                    function.source_name
                );
            }
            let resume_blocks = function
                .blocks
                .iter()
                .filter(|block| {
                    block
                        .source_name
                        .as_deref()
                        .is_some_and(|name| name.contains("resume"))
                })
                .map(|block| block.id)
                .collect::<Vec<_>>();
            if !resume_blocks.is_empty() {
                coroutine_functions += 1;
                let mut pending = resume_blocks;
                let mut seen = std::collections::BTreeSet::new();
                let mut reads_local = false;
                while let Some(block) = pending.pop() {
                    if !seen.insert(block) {
                        continue;
                    }
                    let Some(block) = function.blocks.get(block.0 as usize) else {
                        continue;
                    };
                    reads_local |= block.instructions.iter().any(|instruction| {
                        matches!(instruction.kind, InstructionKind::LoadLocal(_))
                    });
                    pending.extend(successors(&block.terminator));
                }
                coroutine_local_after_resume += usize::from(reads_local);
            }
        }
    }

    eprintln!(
        "verified {} LIR functions from {} corpus entries",
        verified_functions,
        entries.len()
    );
    eprintln!(
        "storage metrics: instructions={instruction_count}, local_traffic={local_traffic}, locals={local_count}, block_parameters={block_parameters}, coroutine_functions={coroutine_functions}, coroutine_local_after_resume={coroutine_local_after_resume}, coroutine_creation_allocations={coroutine_creation_allocations}, missing_coroutine_creation_allocations=0, checked_index_addresses={checked_index_addresses}, checked_index_addresses_without_traps=0"
    );
}

fn lower_source(name: &str, source: &str) -> Module {
    let hir = check_program(&[SourceFile::new(name, source)]).expect("source checks clean");
    lower_module(&hir).expect("source lowers to LIR")
}

fn unrolled_block_count(module: &Module) -> usize {
    module
        .functions
        .iter()
        .flat_map(|function| &function.blocks)
        .filter(|block| {
            block
                .source_name
                .as_deref()
                .is_some_and(|name| name.starts_with("for.unrolled.") && !name.ends_with("unused"))
        })
        .count()
}

#[test]
fn constant_four_trip_loop_unrolls_and_interprets() {
    let module = lower_source(
        "unroll-four.ts",
        "export function main(): void {\n  let total: i32 = 0;\n  for (let i: i32 = 0; i < 4; i += 1) {\n    total += i;\n  }\n  print(`${total}`);\n}\n",
    );
    assert_eq!(unrolled_block_count(&module), 4);
    verify_module(&module).expect("unrolled LIR verifies");
    assert_eq!(interpret(&module).expect("unrolled LIR interprets"), b"6\n");
}

#[test]
fn unroller_handles_small_counts_inclusive_bounds_and_wide_steps() {
    for (name, condition, step, expected_blocks, expected_output) in [
        ("one", "i < 1", "i += 1", 1, b"0\n".as_slice()),
        ("two", "i < 2", "i += 1", 2, b"1\n".as_slice()),
        ("inclusive", "i <= 7", "i += 1", 8, b"28\n".as_slice()),
        ("wide-step", "i < 8", "i += 2", 4, b"12\n".as_slice()),
    ] {
        let source = format!(
            "export function main(): void {{\n  let total: i32 = 0;\n  for (let i: i32 = 0; {condition}; {step}) {{ total += i; }}\n  print(`${{total}}`);\n}}\n"
        );
        let module = lower_source(&format!("unroll-{name}.ts"), &source);
        assert_eq!(unrolled_block_count(&module), expected_blocks, "{name}");
        verify_module(&module).unwrap_or_else(|errors| panic!("{name}: {errors:?}"));
        assert_eq!(
            interpret(&module).unwrap_or_else(|error| panic!("{name}: {error}")),
            expected_output,
            "{name}"
        );
    }
}

#[test]
fn unroller_declines_trip_body_and_trap_limits() {
    let nine_trips = lower_source(
        "unroll-nine.ts",
        "export function main(): void {\n  let total: i32 = 0;\n  for (let i: i32 = 0; i < 9; i += 1) { total += i; }\n  print(`${total}`);\n}\n",
    );
    assert_eq!(unrolled_block_count(&nine_trips), 0);

    let large_body = lower_source(
        "unroll-large.ts",
        "export function main(): void {\n  let total: i32 = 0;\n  for (let i: i32 = 0; i < 4; i += 1) {\n    total += i; total += i; total += i; total += i;\n    total += i; total += i; total += i; total += i;\n    total += i; total += i; total += i; total += i;\n    total += i; total += i; total += i; total += i;\n  }\n  print(`${total}`);\n}\n",
    );
    assert_eq!(unrolled_block_count(&large_body), 0);

    let trap_source = "export function main(): void {\n  let total: i32 = 0;\n  for (let i: i32 = 0; i < 4; i += 1) { total += 12 / (i + 1); }\n  print(`${total}`);\n}\n";
    let hir = check_program(&[SourceFile::new("unroll-trap.ts", trap_source)])
        .expect("trap loop checks clean");
    let trap_body = lower_module(&hir).expect("trap loop lowers");
    assert_eq!(unrolled_block_count(&trap_body), 0);
    assert!(
        lir_facts::dropped_facts(&hir, &trap_body).is_empty(),
        "declined trap loop keeps the exact HIR/LIR fact multiset"
    );
}

#[test]
fn a22_inner_loop_unrolls_four_times() {
    let accept = corpus::corpus_accept();
    let module = lower_entry(&accept, "a22-matrix-propagation");
    let multiply = module
        .functions
        .iter()
        .find(|function| function.source_name == "multiply")
        .expect("a22 has multiply");
    assert_eq!(
        multiply
            .blocks
            .iter()
            .filter(|block| block
                .source_name
                .as_deref()
                .is_some_and(|name| name.starts_with("for.unrolled.")))
            .count(),
        4
    );
}

fn print_snapshot_module(module: &Module) -> String {
    let mut snapshot = module.clone();
    for function in &mut snapshot.functions {
        for block in &mut function.blocks {
            if let Terminator::Unreachable { pos } = &block.terminator {
                block.terminator = Terminator::Trap(subscript_compiler::lir::Trap {
                    kind: TrapKind::Unreachable,
                    pos: pos.clone(),
                });
            }
        }
    }
    print_module(&snapshot)
}

#[test]
fn coroutine_and_measurement_lir_text_matches_goldens() {
    let accept = corpus::corpus_accept();
    let mut actual = String::new();
    for id in corpus::entry_ids(&accept) {
        let lir = lower_entry(&accept, &id);
        if lir
            .functions
            .iter()
            .any(|function| function.is_generator || function.is_async)
            || matches!(
                id.as_str(),
                "a145-emitted-identifiers"
                    | "a147-switch-body-scope"
                    | "a148-switch-using-scope"
                    | "a149-suspension-state"
                    | "a150-receiver-address-invalidation"
                    | "a151-lambda-env-outlives-block"
            )
        {
            actual.push_str("===== ");
            actual.push_str(&id);
            actual.push_str(" =====\n");
            // The committed review snapshot predates the structural/semantic
            // unreachable split. Keep its bytes stable; verifier and fact
            // tests above inspect the unmodified LIR distinction directly.
            actual.push_str(&print_snapshot_module(&lir));
        }
    }
    if std::env::var_os("SUBSCRIPT_CAPTURE_LIR_GOLDENS").is_some() {
        std::fs::write("/tmp/subscript-lir-goldens.txt", actual).expect("write captured LIR text");
        return;
    }
    let golden_path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/lir-goldens/corpus.txt");
    let expected = std::fs::read_to_string(&golden_path)
        .unwrap_or_else(|error| panic!("read {}: {error}", golden_path.display()));
    if actual != expected {
        let line = actual
            .lines()
            .zip(expected.lines())
            .position(|(actual, expected)| actual != expected)
            .map_or_else(
                || actual.lines().count().min(expected.lines().count()) + 1,
                |index| index + 1,
            );
        panic!(
            "LIR text golden differs at line {line} (actual {} bytes, expected {} bytes); rerun with SUBSCRIPT_CAPTURE_LIR_GOLDENS=1 to inspect",
            actual.len(),
            expected.len()
        );
    }
}

#[test]
fn place_bases_keep_their_own_trap_sites() {
    let lir = lower_source(
        "place-chain.ts",
        r#"
@CStruct class V { x: i32; constructor(x: i32) { this.x = x; } }
function idx(): i32 { return 5; }
export function main(): void {
  const a: V[] = [new V(1), new V(2)];
  a[idx()].x = 9;
}
export function read(): i32 {
  const a: V[] = [new V(1), new V(2)];
  return a[idx()].x;
}
"#,
    );
    verify_module(&lir).expect("place-chain LIR verifies");
    let sites = lir
        .functions
        .iter()
        .flat_map(|function| &function.blocks)
        .flat_map(|block| &block.instructions)
        .filter(|instruction| {
            matches!(
                instruction.kind,
                InstructionKind::AddressOfIndex { checked: true }
            )
        })
        .map(|instruction| {
            instruction
                .traps
                .iter()
                .map(|trap| trap.kind.clone())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    assert_eq!(sites.len(), 2, "one checked place-base index per function");
    assert!(sites.iter().all(|traps| {
        traps.contains(&TrapKind::IndexRead) && traps.contains(&TrapKind::DevOnlyLifetime)
    }));
}

#[test]
fn value_class_array_receiver_signature_ignores_address_provenance() {
    let lir = lower_source(
        "array-receiver.ts",
        r#"
@CStruct class Cell { v: i32 = 0; bump(): void { this.v = this.v + 1; } }
export function main(): void {
  const a: Cell[] = [new Cell()];
  a[0].bump();
  print(`${a[0].v}`);
}
"#,
    );
    verify_module(&lir).expect("array-element value receiver verifies");
}

#[test]
fn value_class_rvalue_receivers_materialize_temporary_addresses() {
    let lir = lower_source(
        "rvalue-receiver.ts",
        r#"
@CStruct class V { x: i32 = 0; get(): i32 { return this.x; } }
function mk(): V { return new V(); }
export function main(): void {
  const a: V = new V();
  const b: V = new V();
  print(`${mk().get()}`);
  print(`${(true ? a : b).get()}`);
}
"#,
    );
    verify_module(&lir).expect("rvalue receiver LIR verifies");
    let temporary_addresses = lir
        .functions
        .iter()
        .flat_map(|function| &function.blocks)
        .flat_map(|block| &block.instructions)
        .filter(|instruction| instruction.kind == InstructionKind::AddressOfValue)
        .count();
    assert_eq!(temporary_addresses, 2);
}

#[test]
fn async_binding_crosses_resume_as_an_ssa_value() {
    let accept = corpus::corpus_accept();
    let lir = lower_entry(&accept, "a139-using-async");
    let main = lir
        .functions
        .iter()
        .find(|function| function.source_name == "main")
        .expect("a139 main function");
    assert!(main.locals.is_empty());
    assert!(main
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .all(|instruction| !matches!(
            instruction.kind,
            InstructionKind::LoadLocal(_)
                | InstructionKind::StoreLocal(_)
                | InstructionKind::AddressOfLocal(_)
        )));
    let resource = main.blocks[0]
        .instructions
        .iter()
        .find_map(|instruction| {
            matches!(instruction.kind, InstructionKind::AllocateClass(_))
                .then_some(instruction.result)
                .flatten()
        })
        .expect("resource value");
    let resume = main
        .blocks
        .iter()
        .find(|block| block.source_name.as_deref() == Some("async.resume"))
        .expect("async resume block");
    let suspend = main
        .blocks
        .iter()
        .find_map(|block| match &block.terminator {
            Terminator::Suspend {
                successor,
                resume_value,
                arguments,
                ..
            } if *successor == resume.id => Some((resume_value, arguments)),
            _ => None,
        })
        .expect("suspend edge to async resume");
    let parameter_offset = usize::from(suspend.0.is_some());
    assert_eq!(
        suspend.1.len(),
        resume.parameters.len() - parameter_offset,
        "the frame is exactly the explicit suspension live-in arguments"
    );
    let resource_argument = suspend
        .1
        .iter()
        .position(|argument| argument == &Operand::Value(resource))
        .expect("resource is passed across the suspension");
    let resumed_resource = resume.parameters[parameter_offset + resource_argument];
    assert_ne!(
        resumed_resource, resource,
        "resume defines a fresh SSA value"
    );
    assert!(resume.instructions.iter().any(|instruction| {
        matches!(instruction.kind, InstructionKind::Call(_))
            && instruction
                .operands
                .contains(&Operand::Value(resumed_resource))
    }));
}

#[test]
fn array_for_of_carries_and_advances_traversal_state() {
    let accept = corpus::corpus_accept();
    let lir = lower_entry(&accept, "a77-for-of-containers");
    let main = lir
        .functions
        .iter()
        .find(|function| function.source_name == "main")
        .expect("a77 main function");
    let header = main
        .blocks
        .iter()
        .find(|block| {
            block.source_name.as_deref() == Some("for-of.cond")
                && block.instructions.iter().any(|instruction| {
                    instruction.operands.first().is_some_and(|operand| {
                        matches!(
                            operand,
                            Operand::Value(value)
                                if matches!(
                                    &main.values[value.0 as usize].ty,
                                    ValueType::Iterator(iterator)
                                        if iterator.kind == ForOfKind::ArrayValues
                                )
                        )
                    })
                })
        })
        .expect("array for-of header");
    let state_types = header
        .parameters
        .iter()
        .map(|value| &main.values[value.0 as usize].ty)
        .collect::<Vec<_>>();
    assert!(matches!(
        state_types.as_slice(),
        [
            ValueType::Iterator(_),
            ValueType::Data(Type::I32),
            ValueType::Data(Type::I32)
        ]
    ));
    let step = main
        .blocks
        .iter()
        .find(|block| {
            block.source_name.as_deref() == Some("for-of.step")
                && block
                    .instructions
                    .iter()
                    .any(|instruction| instruction.kind == InstructionKind::IteratorAdvance)
        })
        .expect("array for-of step");
    let advanced = step
        .instructions
        .iter()
        .find(|instruction| instruction.kind == InstructionKind::IteratorAdvance)
        .and_then(|instruction| instruction.result)
        .expect("advanced cursor value");
    let Terminator::Branch(back_edge) = &step.terminator else {
        panic!("array for-of step must end in a back edge");
    };
    assert_eq!(back_edge.block, header.id);
    assert!(back_edge.arguments.contains(&Operand::Value(advanced)));
    assert!(main
        .locals
        .iter()
        .all(|local| local.source_name != "<for-of cursor>"));
}
