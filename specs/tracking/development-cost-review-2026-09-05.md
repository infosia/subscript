# Development cost and structural review — 2026-09-05

Written by the coding agent at `3677d1f` as an untracked report and
recorded here as the origin of §85 to §88. The measurements below are
the report's own; the orchestrator verified every file:line citation
against the tree on 2026-09-05. The section that follows is the
report, unchanged.

---


Date: 2026-09-05. Source revision: `3677d1f`.

## Assessment

The project solves difficult problems, but that does not explain all of its development cost.
The strongest avoidable costs are checker state protocols, expensive test compilation, and manual maintenance of the verification process.
The current architecture supports incremental improvement. A compiler rewrite or removal of an execution tier is not justified.

This review examines source, commit history, review records, and local measurements.
Historical measurements are identified separately. They are not new measurements from this review.
No production code, language contract, or existing test was changed.

## What the evidence can establish

The repository records repeated reviews and long test runs.
It does not record consistent request-to-completion times or reviewer effort.
Commit timestamps cannot separate implementation time, queue time, parallel work, and inactive time.
Consequently, this review identifies causes and opportunities; it does not assign percentages of developer time to them.

Physical line counts include comments, blank lines, and unit tests within `src/`.
They measure the material a reader encounters, not cyclomatic complexity or production-only code.

| Source area | 2026-08-25, `d889620` | Current revision | Change |
|---|---:|---:|---:|
| `compiler/src/` | 28,638 | 36,979 | +29.1% |
| `codegen/src/` | 28,153 | 44,011 | +56.3% |
| `runtime/src/` | 21,729 | 24,037 | +10.6% |
| Total | 78,520 | 105,027 | +33.8% |

The LIR migration and its reference interpreter explain part of this growth.
The interpreter alone occupies 5,945 lines and provides useful independent execution of the LIR contract.
Growth is therefore not equivalent to unnecessary complexity.

The largest current files include `codegen/src/lir.rs` (9,780 lines), `compiler/src/check/expr.rs` (8,400),
`codegen/src/cemit.rs` (8,290), and `codegen/src/lower/func.rs` (8,183).
For example, the 8,400-line expression checker contains only a small final test module, which starts at line 8,293.
These sizes do not result only from colocated tests.

Between August 26 and September 5, 65 non-merge commits changed Rust files under a `src/` directory.
Of those commits, 25 touched `cemit.rs`, 25 touched `check/mod.rs`, 22 touched `codegen/src/lir.rs`, and 19 touched `check/expr.rs`.
The median commit touched five such Rust files. This identifies frequent change locations, not an independent productivity metric.

## Findings

### 1. High priority: expression checking still requires manual state protocols

**Evidence.** `check_expr` returns a `hir::Expr`, but rewritten expressions can also write statements into `FnCtx.synthetic_owners`.
Callers must establish an owner, drain its prefix, and close the owner in the correct syntax context.
The implementation exposes separate entry and exit operations; the return type does not express this obligation.
See `compiler/src/check/mod.rs:434`, `:470`, `:485`, `:1354`, and `compiler/src/check/expr.rs:570`, `:2381`.

The current boundary check reports an escaped prefix. This is a useful improvement.
It still depends on callers invoking the closing operation.
The `Checker` also carries 45 public fields, including mutable context flags and type-resolution state.
Field count alone is not a defect, but these protocols make local changes depend on surrounding state.

**Observed consequence.** R39 review round 2 found generated locals that escaped from `for` initializers, conditions, and arrow bodies.
Five accepted programs then failed during lowering.
Round 3 found the missing `switch` case owner.
These were not separate backend ABI problems; they were instances of an incomplete expression-context protocol.
The current fixes close the recorded examples.
See [R39 review record](specs/tracking/r39-six-requests.md).

Another recent example was the operation-signature table, formerly populated as a side effect of expression checking.
Synthesized `for…of` calls bypassed it. The first repair then omitted parameter defaults from its owner walk.
The final repair centralizes expression owners and the callee mapping.
See [operation-signature review record](specs/tracking/for-of-generator-signature.md)
and `compiler/src/hir.rs:77`, `:128`.

**Action.** First make owner entry and exit one scoped operation, with closure-based completion on every normal return path.
Represent permitted expression contexts explicitly instead of distributing context decisions across callers.
For subsequent sugar, evaluate a result form that carries ordered local evaluation with the expression itself.
It must preserve conditional execution, loop frequency, scope, and the current rejection rules.
Merely returning a flat prefix and moving it before a conditional would reproduce the original defect class.

**Acceptance.** Use a compact matrix of expression owners, side-effecting receivers, conditional branches, and loop positions.
Pin evaluation counts, order, and diagnostics from independently specified expectations.
Retain the full corpus gate at the integration boundary.

### 2. High priority: C source emission scales poorly on the large breadth fixture

**Evidence.** The current release run spends 91.30 seconds in the single boundary-scratch breadth test.
The fixture generates widths 1 through 32 inside one `main`, for 528 target positions per program variant.
It builds two variants, emits C for each, and separately checks and compiles both for the JIT.
See `codegen/tests/boundary_scratch_breadth.rs:10`, `:218`, `:317`, and `:377`.

The R39 record measured the same debug test at 659 seconds before R39 and 654 seconds after it.
That isolated comparison rules out R39 as the origin of that particular slowdown.
The full debug run recorded 1,341 seconds wall time and 926 seconds of test execution; this one test occupied 668 seconds.
Those are historical measurements, not a fresh full-debug run.

A separate probe uses the existing fixture generator and the built libraries at this revision.
It measures the empty-sibling variant at three widths. Each program has exactly one function and one basic block.

| Width | LIR instructions | Check, seconds | Lower and verify, seconds | Emit C, seconds | JIT pipeline, seconds |
|---|---:|---:|---:|---:|---:|
| 8 | 2,400 | 0.0045 | 0.0034 | 0.1249 | 0.0995 |
| 16 | 11,072 | 0.0148 | 0.0177 | 2.0219 | 0.8072 |
| 32 | 59,520 | 0.0601 | 0.1049 | 58.8940 | 14.5611 |

These measurements use release libraries. Emit C includes its own lowering and verification, but invokes no native C compiler.
The JIT column includes checking, lowering, compilation, and execution; it is not execution-only time.
At width 32, a separate verifier invocation takes 0.0424 seconds.
The C output contains 5,066,731 bytes.
At width 16, the debug libraries take 19.7669 seconds to emit C and 11.3288 seconds for the JIT pipeline.

The observed instruction count grows 5.4 times from width 16 to 32, while C emission grows 29.1 times.
A second width-32 observation took 45.1751 seconds to emit the same C source.
A final resource-measured run took 44.4192 seconds for C emission and 44.55 seconds for the whole probe.
Run-to-run variation is material; these observations do not establish a precise asymptotic exponent.
They do establish that frontend checking, dominance, and native C compilation do not explain this fixture's measured C-emission cost.
Optimizing verifier duplication is not the first useful intervention here.

The source identifies storage planning as a candidate for deeper profiling:

- `Body::new` computes root storage and then value coalescing: `codegen/src/cemit.rs:2379`, `:2450`.
- Root storage constructs interference sets before it filters values by managed storage needs: `codegen/src/root_storage.rs:283`.
- Coalescing requests another interference calculation: `codegen/src/cemit.rs:2115`.
- That calculation constructs an origin graph and expands it into another graph: `codegen/src/root_storage.rs:165`.

Dense interference graphs have quadratic space requirements.
The stage measurements do not distinguish those calculations from every other operation inside C emission.
They are profiling candidates, not a claimed exclusive hotspot.

The final probe reached 1,783,119,872 bytes of maximum resident memory, approximately 1.66 GiB.
The tool also reported a peak memory footprint of 1,240,450,680 bytes and zero swaps.
These are whole-process observations for checking, lowering, verification, and C emission; that run performed no JIT or native C compilation.
The local Darwin `getrusage(2)` manual specifies bytes for maximum resident memory.
The memory measurement reinforces the need to profile graph allocation, but does not identify one allocation site by itself.

**Action.** Profile storage planning and coalescing inside C emission next.
Evaluate candidate filtering, compact graph representations, and reuse of compatible analysis results against measured time and memory.
Retain the independent storage and lifetime assertions; a faster incorrect root plan is not an improvement.
Optimize the dominant pass before changing the coverage requirement.
Keep a maximum-width case and independent layout observations if the fixture is reorganized.
Small isolated cases alone do not test interference among all positions in one activation.

### 3. High priority: full-gate cost and gate identity need one executable owner

**Evidence.** The main golden test loops over corpus entries serially.
Each entry invokes the JIT and the ship pipeline, including native compilation and linking.
Many preceding tests run individual entries through the same output comparisons again.
See `codegen/tests/golden.rs:114`, `:144`, `:844`, and `:980`.
Some focused tests make additional observations and must stay.

The three kinds of verification are not the same command:

| Command/profile | Interpreter corpus | Performance thresholds |
|---|---|---|
| Default debug tests | 124 runnable entries; omits the cost-only `a22` | Explicitly skipped |
| Default release tests | Full interpreter corpus explicitly skipped | Run |
| Release with `SUBSCRIPT_FULL_INTERPRETER_SWEEP=1` | 125 runnable entries | Run |

The interpreter excludes 56 entries for declared host-dependent reasons.
It does not cover the whole 181-golden inventory.
See `codegen/tests/lir.rs:1408`, `:1986`, and `benchmarks/tests/perf_gate.rs:1`.

The opt-in full sweep is an intentional owner decision, not an accidental omission.
The risk is that a successful generic `cargo test --release` is reported as the full gate.
The Worker record already retracts a release-pass claim because the claimed revision contained a repeatable release failure.
See [Worker review correction](specs/tracking/q35-string-messages.md).
This does not establish who ran which command; it establishes that the recorded evidence was unreliable.

Test independence also affected review cost.
The first operation-signature tests repeated the production derivation, and the first Worker record tests compared serialization with itself.
Reviews then required hand-written facts and rejection controls.
Those specific tests are now repaired.
Their history explains why a large passing test count did not eliminate later review rounds.

**Action.** Provide explicit quick and full gate commands.
The full command must set required environment variables and record revision, dirty state, toolchain, profile, command, exit status, and suite results.
Record declared skips separately from passed tests.
Run focused tests during implementation and one complete gate for the unchanged integration candidate.
An August 26 owner decision already adopted this separation; put it in executable tooling.
See [LIR migration record](specs/tracking/s68-one-ordered-ir.md).

Before review, state which independent expectation checks the changed behavior and which violating input proves that check can fail.
Apply this to the changed invariant; do not add another general checklist to every unrelated edit.

After measuring duplicate executions, retain additional assertions and remove only redundant execution paths.
Use process isolation for corpus parallelism because some native fixtures have process-global state.
Do not simply add threads to the existing loop.

### 4. Medium priority: corpus inventory requires unnecessary coordinated edits

**Evidence.** Corpus discovery derives entries from disk, but multiple suites also pin numeric totals or list the same entries.
Examples include `compiler/tests/corpus_accept.rs:133`, `compiler/tests/corpus_warn.rs:160`,
`compiler/tests/js_corpus.rs:349`, `codegen/tests/golden.rs:930`, and `codegen/tests/lir.rs:1408`.
The debug interpreter list repeats 124 runnable IDs and their reasons.

The initial Worker-string commit `a3b5bc2` changed 26 files.
Changes to both backends, runtime, generated header, and semantics tests are expected for this feature.
Updates to independent inventory counts add mechanical coordination on top of that required work.
The August 30 duplication review records an actual stop caused by missed inventory updates.
See [duplication review](specs/tracking/duplication-review-2026-08-30.md).

**Action.** Give corpus identity, required sidecars, execution capabilities, and cost exclusions one declarative inventory.
Derive each suite's selection from that inventory.
Retain one explicit completeness check that detects missing or unexpected files.
Do not derive expected semantic values from production lowering; enumeration and the result oracle serve different purposes.

### 5. Medium priority: platform defects surface late

**Evidence.** The tracked tree contains no CI configuration.
The project nevertheless supports different C compilers and three distinct JIT aggregate ABIs.
`codegen/src/lower/func.rs:58` explicitly selects AAPCS64, Win64, or SysV.
The September 5 changes repair a SysV argument-size rule and platform-specific benchmark/link paths.
See commits `8189137`, `3677d1f`, and [Linux portability record](specs/tracking/linux-portability.md).

The defects have different underlying mechanisms. A passing arm64 run cannot validate the SysV branch.
External automation can exist outside this repository; this review cannot establish whether it does.

**Action.** For ABI, linking, and runtime-boundary changes, require recorded results from macOS arm64, Linux x86-64, and Windows x86-64.
Use automated runners where available and an explicit verification matrix otherwise.
Preserve the documented platform exclusions and display them in the result.
Keep noisy performance comparisons serialized within their runner.

### 6. Medium priority: active contracts and historical repairs are expensive to reconstruct

**Evidence.** `specs/blocks/compiler.md` contains 11,794 lines and 619,645 bytes.
The eight block documents total 16,778 lines; 84 tracking documents total 13,545 lines.
The compiler contract combines current rules, former designs, amendments, and review records.

Some entry-point documentation still describes HIR-to-Cranelift directly.
The actual JIT path first constructs LIR in `codegen/src/lower/mod.rs:1279`.
Compare `README.md` under "How it works" and `codegen/src/lib.rs:5`.
This is a concrete navigation mismatch, not evidence that the LIR migration is incomplete.

**Action.** Maintain a compact current architecture map and current invariants by stage.
Move historical rationale and completed review discussions behind stable links when the contracts are next revised.
Keep the executable corpus and one active rule location authoritative.
Do not replace useful contract-first work with undocumented implementation choices.

### 7. Secondary concern: Rust crate boundaries enlarge rebuild scope

**Evidence.** `subscript-codegen` depends on the compiler and runtime.
The compiler crate contains the checker, HIR/LIR definitions, and documentation generators.
Both transcribers and the reference interpreter share one codegen crate.
See the package manifests and `compiler/src/lib.rs:13`, `codegen/src/lib.rs:33`.

The first measured release test build rebuilt compiler, runtime, and codegen with existing dependency caches.
It took 50.54 seconds wall time.
Cargo reported 24.27 seconds for the compiler library and 29.98 seconds for the codegen library.
These overlap with other work and must not be added as a serial total.
The corresponding debug test build took 65.30 seconds.
The unchanged release test build took 0.18 seconds.

This is meaningful cost, but the measurements do not justify calling external dependencies or clean builds the primary bottleneck.
No standardized clean build or controlled single-source-edit rebuild was measured.

**Action.** Establish no-op, checker-edit, runtime-edit, and clean-build baselines before splitting crates.
Separate validation, ABI planning, and instruction families for readability where ownership is clear.
File splitting alone does not establish a rebuild improvement.
Consider an IR-only crate or an optional interpreter only if measured rebuild scope warrants the compatibility and dependency work.

## Complexity that is justified

- The JIT and ship tier serve different iteration and execution requirements. Their coexistence is a product requirement.
- C ABI layout, nullable boundary views, callbacks, and ownership interact across native and script code.
- Suspension and hot reload require explicit state, liveness, and invalidation rules.
- TypeScript-compatible syntax with different static semantics creates real acceptance and diagnostic constraints.
- The independent interpreter and hand-written goldens detect errors that backend agreement alone cannot detect.

The common ordered LIR is an appropriate response to duplicated evaluation-order decisions.
The older seven-round suspension repair is historical evidence for that decision, not an open defect in the current lowering.
See [suspension review record](specs/tracking/s67-scoping-and-suspension.md).

The August 30 consolidation and September 3 expression-owner iterator are further improvements already present.
The remaining work is to extend explicit ownership and verification to the places that still rely on caller discipline.

## Recommended order

| Order | Work | Completion evidence |
|---|---|---|
| 1 | One quick/full gate interface and generated result record | Reproducible command, explicit skips, exact revision and exit status |
| 2 | Profile and improve the boundary breadth compilation path | Stage timings at widths 8/16/32; same maximum-width assertions |
| 3 | Make synthetic expression ownership scoped and explicit | Context matrix passes; existing acceptance and diagnostics remain fixed |
| 4 | Centralize corpus inventory and isolate repeated executions | One entry addition requires no unrelated numeric-total edits |
| 5 | Record the platform matrix and simplify current architecture documentation | Required host results and one accurate stage map |
| Later | Adjust crate boundaries if rebuild measurements justify it | Lower edit-to-test time on a controlled source edit |

Do not predict a review-round reduction from file size alone.
Track request start, first passing focused test, first review, distinct defect classes, and gate wall time for the next few changes.
Those measurements can separate domain difficulty from repeated protocol and verification work.

## Local measurement appendix

Measurements use the existing checkout and caches on Darwin arm64, Rust/Cargo 1.95.0.
Build and suite times are single observations. C emission at width 32 has two repeat observations.
These are not controlled hardware benchmarks.
The first build was not a clean build.

The complete release run explicitly enables the full runnable interpreter corpus:

```sh
cargo test --offline --locked --workspace --release --no-run --timings
SUBSCRIPT_FULL_INTERPRETER_SWEEP=1 cargo test --offline --locked --workspace --release
```

| Measurement | Result |
|---|---|
| Initial release test build, existing caches | 50.54 seconds wall |
| Unchanged release test build | 0.18 seconds wall |
| Initial debug test build, existing caches | 65.30 seconds wall |
| Complete release test command, full interpreter enabled | 670.49 seconds wall; exit 0 |
| Aggregate test results | 66 suites; 1,253 passed; 0 failed; 1 ignored |

The repository hygiene script exited 0. All relative Markdown references in this report resolve.

The largest suite times are:

| Suite | Seconds |
|---|---:|
| `codegen/tests/lir.rs`, full interpreter enabled | 292.69 |
| `codegen/tests/golden.rs` | 128.54 |
| `codegen/tests/boundary_scratch_breadth.rs` | 91.30 |
| `codegen/tests/cemit.rs` | 76.91 |
| `examples/tests/gate.rs` | 22.25 |
| `benchmarks/tests/perf_gate.rs` | 10.39 |

These suite times include concurrent tests within each suite. They are not individual pass timings.
Reported suite durations sum to 648.39 seconds; command wall time also includes orchestration and documentation-test compilation outside the harness timers.
The 292.69-second LIR suite includes more tests than the interpreter sweep alone.
The default release run without the full sweep was not timed as a separate complete command.

The full debug suite, clean builds, controlled source-edit rebuilds, and other host platforms were not run in this review.
The debug breadth probe covers widths 8 and 16; the original width-32 test ran in the complete release suite.
No existing test coverage was reduced.

Local logs, suite timings, and the probe source are retained under `target/development-cost-review-2026-09-05/`.
These are ignored build artifacts, not committed source changes.

The first detailed resource report could not complete because the sandbox rejected the timer's system query.
After approval, the same probe completed outside the sandbox with exit 0 and produced the resource figures above.
The initial repeat still produced its C-emission timing; it is not used as a successful resource measurement.
