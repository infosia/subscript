# Can-raise and hot reload: measurement

Date: 2026-09-28. Pin: `558b23114a9b1d59b589052e728a0539030b92e1`.
Host: aarch64 macOS. Rust toolchain: 1.95.0. Release C compiler: Apple clang 21.0.0.

## Result

HEAD catches the exception after the direct, resource, and transitive swaps.
HEAD recompiles the complete module at each accepted swap.
A retained lambda misses its lexical handler and produces trap 29.
A suspended async frame produces trap 12 before it calls the changed function.

A refuses all five executable swaps. B already exists as a complete-module recompile at HEAD.
C fixes the retained-lambda probe and preserves the stale-frame trap.
The extended hash form, A-all, also refuses all five swaps and breaks one additional reload test.
No candidate is a production change in this record.

## Contracts under measurement

- §115.6 rule 3 derives `can_raise` in HIR. A call uses that fact to select its raise edge.
- A and A-all extend the body-derived facts in the §8.2 declaration hash.
- C gives reload calls conservative raise edges, contrary to the precise selection in §115.6 rule 3.
- §115.6 rule 4 requires one pending-word load and compare on the success path.
- §115.5 and §116.1 rule 4b require dispose hooks and scope-owned handle releases on exception exits.
- §119 keeps synthesized helpers outside reload slots and the declaration hash. These prototypes preserve that exclusion.

## Probe definitions

Each probe has one source file, `live.ts`.
Version 1 starts with `function f(): void {}`.
Version 2 replaces only that body with `throw new Error("x");`.
The declaration set stays constant.

| ID | Harness argument | Retained state and host call |
|---|---|---|
| P1 | `direct` | After the swap, call `g`; its `try` calls `f`. |
| P2 | `using` | After the swap, call `g`; its `try` holds a resource across `f`. |
| P3 | `transitive` | After the swap, call `h`; its `try` calls `g`, which calls `f`. |
| P4 | `lambda` | The initializer saves a lambda in a global; after the swap, `g` calls that old lambda. |
| P5 | `async` | Call `g`, suspend at `Context.suspend()`, swap, then call `async_step` once. |
| X1 | `lambda_using` | A supplemental resource declaration inside a lambda; the checker rejects version 1. |

P2 measures disposal. It does not measure an async handle release count.
The separate exception-exit handle test below checks the precise non-reload form under the C prototype.
P5 has one pending frame before the swap.
A refused swap leaves its epoch unchanged, so that old frame resumes under A.
An accepted swap increments the Context epoch, so that frame traps before the call of `f`.

The scratch harness uses `ReloadSession::new`, `reload`, `call_export`, `async_step`, and `take_output`, as the committed reload tests do.
Each probe runs in a separate process. All harness sources and binaries reside in `$TMPDIR`.
The harness prints stdout with Rust debug escapes. Its transcripts below preserve those bytes verbatim.

```rust
use subscript_codegen::ReloadSession;
use subscript_compiler::{SourceFile,check_program};
fn files(s: &str)->Vec<SourceFile>{vec![SourceFile::new("live.ts",s)]}
fn main(){
 let which=std::env::args().nth(1).unwrap();
 let body=match which.as_str(){
 "direct"=>r#"export function g(): void { try { f(); } catch(e) { print("caught"); } }"#,
 "using"=>r#"class R { [Symbol.dispose](): void { print("dispose"); } }
 export function g(): void { try { using r = new R(); f(); } catch(e) { print("caught"); } }"#,
 "transitive"=>r#"function g(): void { f(); } export function h(): void { try { g(); } catch(e) { print("caught"); } }"#,
 "lambda"=>r#"let saved: () => void = (): void => { try { f(); } catch(e) { print("caught"); } };
 export function g(): void { saved(); }"#,
 "lambda_using"=>r#"class R { [Symbol.dispose](): void { print("dispose"); } }
 let saved: () => void = (): void => { try { using r = new R(); f(); } catch(e) { print("caught"); } };
 export function g(): void { saved(); }"#,
 "async"=>r#"export async function g(): Promise<void> { await Context.suspend(); try { f(); } catch(e) { print("caught"); } }"#,
 _=>panic!("unknown probe")};
 let before=format!("function f(): void {{}}\n{body}\n");
 let after=before.replace("function f(): void {}","function f(): void { throw new Error(\"x\"); }");
 for (v,s) in [("v1",&before),("v2",&after)] { match check_program(&files(s)){Ok(m)=>println!("{v} facts={:?}",m.functions.iter().filter(|f|!f.synthesized_helper).map(|f|(&f.name,f.can_raise)).collect::<Vec<_>>()),Err(e)=>{println!("check={e:?}");return;}} }
 let mut s=match ReloadSession::new(&files(&before)){Ok(s)=>s,Err(e)=>{println!("new={e:?}");return;}};
 if which=="async"{println!("start={:?}",s.call_export("g"));println!("pending={}",s.async_pending());println!("before_stdout={:?}",String::from_utf8_lossy(&s.take_output()));}
 println!("swap={:?}",s.reload(&files(&after)));
 let result=if which=="async"{s.async_step().map(|_|())}else{s.call_export(if which=="transitive"{"h"}else{"g"})};
 println!("host={result:?}");println!("stdout={:?}",String::from_utf8_lossy(&s.take_output()));
}
```

## Part 1: HEAD transcripts

```text
=== direct ===
v1 facts=[("f", false), ("g", false)]
v2 facts=[("f", true), ("g", true)]
swap=Ok(())
host=Ok(())
stdout="caught\n"
exit=0
=== using ===
v1 facts=[("f", false), ("g", false)]
v2 facts=[("f", true), ("g", true)]
swap=Ok(())
host=Ok(())
stdout="dispose\ncaught\n"
exit=0
=== transitive ===
v1 facts=[("f", false), ("g", false), ("h", false)]
v2 facts=[("f", true), ("g", true), ("h", true)]
swap=Ok(())
host=Ok(())
stdout="caught\n"
exit=0
=== lambda ===
v1 facts=[("f", false), ("g", true)]
v2 facts=[("f", true), ("g", true)]
swap=Ok(())
host=Err(Trap(TrapReport { rule: UncaughtException, message: "Error: x", pos: Pos { file: "live.ts", line: 1, col: 22 }, stdout: [] }))
stdout=""
exit=0
=== lambda_using ===
check=[Diagnostic { code: S100, message: "nested declarations are not in the decided surface", pos: Pos { file: "live.ts", line: 3, col: 46 }, divergence: Some(UsingDeclaration) }]
exit=0
=== async ===
v1 facts=[("f", false), ("g", false)]
v2 facts=[("f", true), ("g", true)]
start=Ok(())
pending=1
before_stdout=""
swap=Ok(())
host=Err(Trap(TrapReport { rule: StaleCoroutine, message: "stale coroutine after reload", pos: Pos { file: "live.ts", line: 1, col: 28 }, stdout: [] }))
stdout=""
exit=0
```

P1, P2, P3, P4, and P5 accept the swap.
P1 and P3 print `caught\n`. P2 prints `dispose\ncaught\n`.
P4 prints no script output and returns `UncaughtException` (29), with `Error: x`.
P5 prints no script output and returns `StaleCoroutine` (12).
Each executable probe exits 0 after the harness records its host result. None crashes.
X1 never creates a session; its process exit does not indicate an accepted program.

## Candidate A: hash, with the existing lambda scope

The prototype appends `can_raise` to each user function signature, method signature, and constructor signature.
The lambda walk appends the fact only where the existing hash records carrier-parameter escape facts.
The prototype leaves synthesized helpers outside the hash.

| Measurement | Result |
|---|---|
| P1–P5 swap | All refuse with `function f`. |
| Fixed probes through an accepted swap | None. |
| P4 defect prevention | The new raising body never enters the session. |
| P5 after refusal | The old frame resumes without a trap. |
| Extra generation compilation on these refusals | None; the declaration comparison precedes compilation. |
| Existing reload tests | 27 pass; 3 fail, compared with 30 pass at HEAD. |
| Timing and code-size cost | Not timed for A; the requested cost is swap refusal and test compatibility. |

Verbatim probe transcript:

```text
=== direct ===
v1 facts=[("f", false), ("g", false)]
v2 facts=[("f", true), ("g", true)]
swap=Err(DeclarationChanged { declaration: "function f" })
host=Ok(())
stdout=""
exit=0
=== using ===
v1 facts=[("f", false), ("g", false)]
v2 facts=[("f", true), ("g", true)]
swap=Err(DeclarationChanged { declaration: "function f" })
host=Ok(())
stdout="dispose\n"
exit=0
=== transitive ===
v1 facts=[("f", false), ("g", false), ("h", false)]
v2 facts=[("f", true), ("g", true), ("h", true)]
swap=Err(DeclarationChanged { declaration: "function f" })
host=Ok(())
stdout=""
exit=0
=== lambda ===
v1 facts=[("f", false), ("g", true)]
v2 facts=[("f", true), ("g", true)]
swap=Err(DeclarationChanged { declaration: "function f" })
host=Ok(())
stdout=""
exit=0
=== lambda_using ===
check=[Diagnostic { code: S100, message: "nested declarations are not in the decided surface", pos: Pos { file: "live.ts", line: 3, col: 46 }, divergence: Some(UsingDeclaration) }]
exit=0
=== async ===
v1 facts=[("f", false), ("g", false)]
v2 facts=[("f", true), ("g", true)]
start=Ok(())
pending=1
before_stdout=""
swap=Err(DeclarationChanged { declaration: "function f" })
host=Ok(())
stdout=""
exit=0
```

The three changed tests are:

| Existing test | Measured failure |
|---|---|
| `first_error_family_uses_allow_a_body_swap` | `DeclarationChanged { declaration: "function main" }` on the first JSON parse body edit. |
| `synthesized_helpers_allow_a_body_swap` | `DeclarationChanged { declaration: "function main" }`. |
| `synthesized_helpers_do_not_enter_the_declaration_hash` | The two declaration hashes differ. |

The hash failure compares values `11550157072140332938` and `8565731402274012328`.
The function `main` changes from a non-raising body to a raising body.
The helper exclusion alone does not preserve these accepted body swaps.

## Candidate B: recompile dependents

HEAD already recompiles every function before it replaces the table.
B therefore uses that existing, broader algorithm. The prototype adds a per-function emission trace, with no new dependency algorithm.
The trace records newly defined machine code for both unchanged dependents of P3.
A second recompile pass adds no missing caller replacement in these probes.

| Measurement | Result |
|---|---|
| P1 | Accepted; `caught\n`. |
| P2 | Accepted; `dispose\ncaught\n`. |
| P3 | Accepted; `caught\n`. |
| P4 | Accepted; trap 29, empty stdout. |
| P5 | Accepted; trap 12, empty stdout. |
| Refused executable swaps | None. |
| Extra unchanged user functions versus a changed-body-only recompile, P3 | `g`, `h`: two functions. |
| Extra functions versus HEAD | Zero; HEAD already defines them again. |
| New fixes versus HEAD | None. |
| Separate timing cost | No distinct algorithm to time. |

P3 also recompiles the Error constructor, function-value wrappers, the initializer, and the async runner.
The trace labels functions 0, 1, 2, and 3 as the Error constructor, `f`, `g`, and `h`, respectively.
The following excerpt lists all definitions, in generation order:

```text
compiled LIR function 0 bytes=492
compiled LIR function 1 bytes=4
compiled LIR wrapper 1 bytes=32
compiled LIR function 2 bytes=68
compiled LIR wrapper 2 bytes=32
compiled LIR function 3 bytes=172
compiled LIR wrapper 3 bytes=32
compiled empty LIR initializer bytes=4
compiled async LIR runner bytes=4
compiled LIR function 0 bytes=492
compiled LIR function 1 bytes=564
compiled LIR wrapper 1 bytes=32
compiled LIR function 2 bytes=68
compiled LIR wrapper 2 bytes=32
compiled LIR function 3 bytes=324
compiled LIR wrapper 3 bytes=32
compiled empty LIR initializer bytes=4
compiled async LIR runner bytes=4
swap=Ok(())
```

The retained lambda in Context state still points to its original code.
The new generation includes a new lambda body, but the swap does not rerun the global initializer.
Recompilation therefore does not repair P4's retained handler edge.
The Context epoch invalidates P5, even though its source body does not change.

Verbatim B transcript:

```text
=== direct ===
v1 facts=[("f", false), ("g", false)]
v2 facts=[("f", true), ("g", true)]
swap=Ok(())
host=Ok(())
stdout="caught\n"
exit=0
=== using ===
v1 facts=[("f", false), ("g", false)]
v2 facts=[("f", true), ("g", true)]
swap=Ok(())
host=Ok(())
stdout="dispose\ncaught\n"
exit=0
=== transitive ===
v1 facts=[("f", false), ("g", false), ("h", false)]
v2 facts=[("f", true), ("g", true), ("h", true)]
swap=Ok(())
host=Ok(())
stdout="caught\n"
exit=0
=== lambda ===
v1 facts=[("f", false), ("g", true)]
v2 facts=[("f", true), ("g", true)]
swap=Ok(())
host=Err(Trap(TrapReport { rule: UncaughtException, message: "Error: x", pos: Pos { file: "live.ts", line: 1, col: 22 }, stdout: [] }))
stdout=""
exit=0
=== lambda_using ===
check=[Diagnostic { code: S100, message: "nested declarations are not in the decided surface", pos: Pos { file: "live.ts", line: 3, col: 46 }, divergence: Some(UsingDeclaration) }]
exit=0
=== async ===
v1 facts=[("f", false), ("g", false)]
v2 facts=[("f", true), ("g", true)]
start=Ok(())
pending=1
before_stdout=""
swap=Ok(())
host=Err(Trap(TrapReport { rule: StaleCoroutine, message: "stale coroutine after reload", pos: Pos { file: "live.ts", line: 1, col: 28 }, stdout: [] }))
stdout=""
exit=0
```

## Candidate C: conservative reload call edges

The prototype passes the reload option into shared LIR construction.
In reload mode, each `InstructionKind::Call` with a `Call` trap gains a raise edge if it lacks one.
This covers script calls, methods, constructors, indirect calls, intrinsics, and built-in calls represented by that instruction.
The existing handler resolver selects the lexical handler, dispose path, release path, or propagate exit.
A runtime-only ownership release is not a LIR call instruction; its existing trap check remains unchanged.

The prototype marks reload LIR functions as able to raise and gives direct-await resume blocks their matching raise sites.
The verifier requires those await sites when the target has the conservative fact.
HIR retains its precise facts. Ship C and ordinary dev JIT use the original precise LIR path.

| Measurement | Result |
|---|---|
| P1 | Accepted; `caught\n`. |
| P2 | Accepted; `dispose\ncaught\n`. |
| P3 | Accepted; `caught\n`. |
| P4 | Accepted; `caught\n`; no host trap. |
| P5 | Accepted; trap 12 before the changed call. |
| Refused executable swaps | None. |
| New fixes versus HEAD | P4. |
| Existing reload tests | 30 pass; 0 fail, including the 260-entry golden sweep. |
| Reload corpus compile-time median | 772.560496 ms → 781.326707 ms; +1.13%. |
| Reload corpus emitted code buffers | 1,847,972 → 1,869,360 bytes; +21,388 bytes, +1.16%. |
| Success-path pending-word check | One load and one compare; the state-2 comparison stays on the nonzero branch. |

Verbatim C transcript:

```text
=== direct ===
v1 facts=[("f", false), ("g", false)]
v2 facts=[("f", true), ("g", true)]
swap=Ok(())
host=Ok(())
stdout="caught\n"
exit=0
=== using ===
v1 facts=[("f", false), ("g", false)]
v2 facts=[("f", true), ("g", true)]
swap=Ok(())
host=Ok(())
stdout="dispose\ncaught\n"
exit=0
=== transitive ===
v1 facts=[("f", false), ("g", false), ("h", false)]
v2 facts=[("f", true), ("g", true), ("h", true)]
swap=Ok(())
host=Ok(())
stdout="caught\n"
exit=0
=== lambda ===
v1 facts=[("f", false), ("g", true)]
v2 facts=[("f", true), ("g", true)]
swap=Ok(())
host=Ok(())
stdout="caught\n"
exit=0
=== lambda_using ===
check=[Diagnostic { code: S100, message: "nested declarations are not in the decided surface", pos: Pos { file: "live.ts", line: 3, col: 46 }, divergence: Some(UsingDeclaration) }]
exit=0
=== async ===
v1 facts=[("f", false), ("g", false)]
v2 facts=[("f", true), ("g", true)]
start=Ok(())
pending=1
before_stdout=""
swap=Ok(())
host=Err(Trap(TrapReport { rule: StaleCoroutine, message: "stale coroutine after reload", pos: Pos { file: "live.ts", line: 1, col: 28 }, stdout: [] }))
stdout=""
exit=0
```

### Corpus cost method

The source set is all 260 committed accept entries with goldens, from `codegen/tests/corpus`.
The harness uses the committed native-fixture symbols and ambient mirrors.
Both binaries use release builds. Each variant has one discarded corpus pass and three timed corpus passes.
Each timed value sums complete check, reload lowering, code definition, and JIT finalization spans across those 260 entries.
Source-file reads occur before the span. No script executes. Generation disposal occurs after the span.

Code size sums `compiled_code().code_buffer().len()` after each function definition, including wrappers, the initializer, the async runner, and entry adapters.
It excludes separate data allocations and reservation slack.
Both variants carry the same relaxed byte counter and disabled trace branch per function.
These counters are absent from the final `perf-gate` measurements below.

The corpus runs are serial. No other build runs during them.
The table reports the median of passes 1–3, not the warm-up pass.

```text
HEAD
run=0 entries=260 seconds=0.781786117 bytes=1847972
run=1 entries=260 seconds=0.778403584 bytes=1847972
run=2 entries=260 seconds=0.772560496 bytes=1847972
run=3 entries=260 seconds=0.771683534 bytes=1847972
C
run=0 entries=260 seconds=0.808896794 bytes=1869360
run=1 entries=260 seconds=0.776106701 bytes=1869360
run=2 entries=260 seconds=0.781326707 bytes=1869360
run=3 entries=260 seconds=0.785430081 bytes=1869360
```

### Release performance gate

Command: `perf-gate --gate --warmup 3 --timed 11`.
`SUBSCRIPT_RUNTIME_STATICLIB` selects the release runtime archive in `target/release`.
The final binaries have no measurement counter or trace branch.
All six invocations exit 0. Every execution output and every gate threshold passes.

The sequence is HEAD-1, C-1, HEAD-2, C-2, HEAD-3, C-3.
Each variant has three invocations and 33 timed samples per subject.
Each invocation discards at least three complete operations and 200 ms of warm-up per subject.
Actual warm-up counts differ because the time floor controls them.
No other build runs during these serial invocations.

| Pair | HEAD dev iteration, ms | C dev iteration, ms | HEAD hot reload, ms | C hot reload, ms |
|---|---:|---:|---:|---:|
| 1 | 3.936 | 3.941 | 0.500 | 0.507 |
| 2 | 3.939 | 3.926 | 0.499 | 0.499 |
| 3 | 3.933 | 3.934 | 0.511 | 0.501 |
| Median of the three medians | 3.936 | 3.934 | 0.500 | 0.501 |

The final median-of-medians values are 3.936 → 3.934 ms for dev iteration and 0.500 → 0.501 ms for hot reload.
These are observations from one host, without a statistical significance claim.

Verbatim one-shot rows, warm-up counts, timed samples, and gate results:

```text
head pair 1
dev-iteration       3.936 ms     3.916 ms     3.979 ms      1.1%
hot-reload          0.500 ms     0.496 ms     0.511 ms      2.3%
  dev-iteration     201.282 ms across 50 complete operations
  hot-reload        200.440 ms across 393 complete operations
  dev-iteration   3.916 3.979 3.930 3.936 3.951 3.930 3.940 3.928 3.938 3.950 3.928
  hot-reload      0.503 0.500 0.498 0.497 0.503 0.507 0.499 0.496 0.497 0.511 0.509
  dev-iteration   changed a22 source: check + lower + finalize
  hot-reload      reload.rs::entryless_session_observes_an_accepted_body_swap (`frame` only): check + lower + finalize + atomic table swap
result:      all gated criteria were met
```

```text
C pair 1
dev-iteration       3.941 ms     3.918 ms     3.988 ms      1.2%
hot-reload          0.507 ms     0.503 ms     0.522 ms      3.0%
  dev-iteration     201.150 ms across 50 complete operations
  hot-reload        200.193 ms across 393 complete operations
  dev-iteration   3.932 3.988 3.943 3.965 3.952 3.923 3.941 3.918 3.969 3.933 3.935
  hot-reload      0.519 0.508 0.503 0.522 0.507 0.504 0.513 0.507 0.507 0.507 0.503
  dev-iteration   changed a22 source: check + lower + finalize
  hot-reload      reload.rs::entryless_session_observes_an_accepted_body_swap (`frame` only): check + lower + finalize + atomic table swap
result:      all gated criteria were met
```

```text
head pair 2
dev-iteration       3.939 ms     3.922 ms     3.955 ms      0.4%
hot-reload          0.499 ms     0.496 ms     0.506 ms      1.3%
  dev-iteration     201.055 ms across 50 complete operations
  hot-reload        200.181 ms across 393 complete operations
  dev-iteration   3.954 3.946 3.925 3.955 3.926 3.922 3.941 3.933 3.939 3.934 3.953
  hot-reload      0.506 0.503 0.496 0.498 0.498 0.501 0.496 0.499 0.499 0.500 0.501
  dev-iteration   changed a22 source: check + lower + finalize
  hot-reload      reload.rs::entryless_session_observes_an_accepted_body_swap (`frame` only): check + lower + finalize + atomic table swap
result:      all gated criteria were met
```

```text
C pair 2
dev-iteration       3.926 ms     3.918 ms     3.953 ms      0.7%
hot-reload          0.499 ms     0.495 ms     0.505 ms      1.1%
  dev-iteration     203.728 ms across 51 complete operations
  hot-reload        200.073 ms across 397 complete operations
  dev-iteration   3.926 3.926 3.944 3.918 3.919 3.923 3.953 3.940 3.923 3.927 3.942
  hot-reload      0.496 0.498 0.499 0.505 0.505 0.495 0.503 0.498 0.499 0.503 0.496
  dev-iteration   changed a22 source: check + lower + finalize
  hot-reload      reload.rs::entryless_session_observes_an_accepted_body_swap (`frame` only): check + lower + finalize + atomic table swap
result:      all gated criteria were met
```

```text
head pair 3
dev-iteration       3.933 ms     3.922 ms     3.972 ms      1.0%
hot-reload          0.511 ms     0.500 ms     0.519 ms      2.0%
  dev-iteration     201.304 ms across 50 complete operations
  hot-reload        200.337 ms across 389 complete operations
  dev-iteration   3.933 3.933 3.937 3.972 3.924 3.951 3.953 3.926 3.922 3.922 3.952
  hot-reload      0.512 0.519 0.512 0.506 0.507 0.509 0.512 0.502 0.500 0.511 0.513
  dev-iteration   changed a22 source: check + lower + finalize
  hot-reload      reload.rs::entryless_session_observes_an_accepted_body_swap (`frame` only): check + lower + finalize + atomic table swap
result:      all gated criteria were met
```

```text
C pair 3
dev-iteration       3.934 ms     3.910 ms     3.954 ms      0.6%
hot-reload          0.501 ms     0.495 ms     0.522 ms      4.1%
  dev-iteration     200.969 ms across 50 complete operations
  hot-reload        200.097 ms across 392 complete operations
  dev-iteration   3.910 3.935 3.946 3.954 3.936 3.927 3.932 3.917 3.920 3.944 3.934
  hot-reload      0.498 0.495 0.498 0.508 0.503 0.503 0.522 0.497 0.498 0.507 0.501
  dev-iteration   changed a22 source: check + lower + finalize
  hot-reload      reload.rs::entryless_session_observes_an_accepted_body_swap (`frame` only): check + lower + finalize + atomic table swap
result:      all gated criteria were met
```

Additional cost observations do not enter that final table.
Three counter-instrumented gate invocations per variant also pass, each with 11 timed samples per subject.
Three startup attempts per variant fail before measurement because the copied binary cannot find the runtime archive without the environment setting.
A preliminary HEAD corpus check has one discarded pass and three timed passes; it also compiles all 260 entries.

### Success-path evidence

The finalized Cranelift IR for P3's original `h` contains these instructions after the call of `g`.

HEAD:

```text
    call_indirect sig1, v6(v0)
    v7 = load.i32 notrap aligned v0
    v10 = iconst.i32 0
    v8 = icmp eq v7, v10  ; v10 = 0
    brif v8, block4, block5
```

C:

```text
    call_indirect sig1, v6(v0)
    v7 = load.i32 notrap aligned v0
    v29 = iconst.i32 0
    v8 = icmp eq v7, v29  ; v29 = 0
    brif v8, block5, block7

block7:
    v28 = iconst.i32 2
    v9 = icmp.i32 eq v7, v28  ; v28 = 2
    brif v9, block3, block6
```

The success branch does not enter `block7`. The second comparison reuses the first load.
C retains the previously absent catch code. This function's code buffer grows from 172 to 324 bytes before the swap.

## Additional form: A-all

A-all adds the fact for every lambda that the HIR expression-owner walk visits, including lambdas without carrier parameters.
Its probe transcript is byte-identical to A's transcript above.
The extra hash entries also make the lambda count part of compatibility.

| Measurement | Result |
|---|---|
| P1–P5 swap | All refuse with `function f`. |
| Fixed probes through an accepted swap | None. |
| New generation compilation on these refusals | None. |
| Existing reload tests | 26 pass; 4 fail. |
| Additional failure versus A | `a_retained_lambda_cannot_call_a_new_escaping_parameter`. |
| Timing and code-size cost | Not timed; the measured additional cost is the lost compatible swap. |

The additional failure is verbatim:

```text
unchanged escape facts permit the body edit: DeclarationChanged { declaration: "lambda parameter escapes" }
```

The compatible edit removes an inner lambda. A-all rejects that edit because its extra lambda fact disappears.
No additional behavior-preserving candidate is measured.

## Checks and final tree

| Check | Result |
|---|---|
| HEAD `codegen/tests/reload.rs` | 30 pass. |
| A, existing lambda scope | 27 pass; the three failures above. |
| A-all | 26 pass; the four failures above. |
| C `codegen/tests/reload.rs` | 30 pass; all 260 reload goldens agree. |
| C `coroutine_and_measurement_lir_text_matches_goldens` | Pass; the ordinary LIR snapshot stays precise. |
| C `an_exception_exit_releases_an_unobserved_handle` | Pass on dev JIT, ship C, and the interpreter; trap 29 reports `Error: dropped`. |
| Final release `perf-gate --gate` | HEAD 3/3 and C 3/3 pass. |
| Reload tests after source restoration | 30 pass; 0 fail. |
| `tools/hygiene.sh` | Exit 0. |

All seven prototype source files match their original bytes.
No corpus source or golden changes. No probe source enters the tree.
No git write command runs. `tools/gate.sh` does not run.
The only remaining file is this measurement record.

Final `git status --short`:

```text
?? specs/tracking/can-raise-reload.md
```

## §121 implementation: Red

At `aa33213`, `retained_lambda_catches_a_newly_raising_callee` accepts the body swap and fails at the retained lambda call.
Command: `cargo test --offline --locked -p subscript-codegen --test reload retained_lambda_catches_a_newly_raising_callee -- --exact`.

```text
retained lambda catches x: Trap(TrapReport { rule: UncaughtException, message: "Error: x", pos: Pos { file: "live.ts", line: 2, col: 22 }, stdout: [] })
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 30 filtered out; finished in 0.04s
```

The same test fails against a release build from `git archive 558b231` with the same trap tuple and empty stdout.
Its result is `FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 30 filtered out; finished in 0.01s`.
The pin test adds only the P4 test function to the archived reload test target.

## §121 implementation: form and tests

`Expr::trap_sites_for_reload` takes the compile mode as an input to the shared HIR site derivation.
Reload calls with a `Call` site carry a raise site. Direct awaits use the same conservative mode.
Held-handle creation keeps its exception in the handle; its await carries the raise site.
HIR keeps precise `can_raise` facts. Reload LIR functions carry conservative `can_raise` facts.
The existing LIR builder resolves each handler, disposal path, release path, or propagate exit.
The engines add no edges. The declaration hash is unchanged.

The dev pending-word check is unchanged: one load and one compare precede the success branch.
Only the nonzero branch compares the loaded word with exception state 2.

The P4 test is Red at both `558b231` and `aa33213`.
P1, P2, and P3 have quiet-call controls and raising body swaps.
P5 compares an unchanged resume with a stale resume after the raising body swap.
The P2 handle test checks disposal before trap 29 for an unobserved handle on normal and exception exits.
The HIR test directly checks the mode input and keeps a raising-call control.
The LIR test covers named functions, a method, a constructor, and a lambda, with a raising-call control.
It also executes the control through both LIR modes on the interpreter.
No `.expected` file or LIR text snapshot changes. No additional source split is required.

## §121 implementation: paired cost against `558b231`

The baseline sources come from `git archive 558b231`. Both variants use release builds under the pinned toolchain.
The compile harness uses the same 260 corpus entries, fixtures, timed span, and byte counter as the candidate-C measurement above.
Each variant has one discarded corpus pass and three timed passes. Source reads precede each timed span.
The byte counter sums emitted function code buffers, including wrappers, initializers, async runners, and entry adapters.
The measurement counter is absent from the final sources and the performance-gate binaries.
All measurements run serially; no build runs during a measurement.

| Corpus pass | Baseline ms | Implementation ms | Baseline bytes | Implementation bytes |
|---|---:|---:|---:|---:|
| 0 (discarded) | 798.994916 | 799.568381 | 1,847,972 | 1,869,360 |
| 1 | 776.688624 | 784.404773 | 1,847,972 | 1,869,360 |
| 2 | 779.785291 | 782.570504 | 1,847,972 | 1,869,360 |
| 3 | 776.780202 | 783.359536 | 1,847,972 | 1,869,360 |

Compile median: 776.780202 → 783.359536 ms (+0.85%).
Code buffers: 1,847,972 → 1,869,360 bytes (+21,388 bytes, +1.16%).

Command: `perf-gate --gate --warmup 3 --timed 11`.
`SUBSCRIPT_RUNTIME_STATICLIB` selects each variant's release runtime archive.
The run order is baseline-1, implementation-1, baseline-2, implementation-2, baseline-3, implementation-3.
Each variant has three invocations and 33 timed samples per subject. All six invocations exit 0 and meet every gate criterion.
Each invocation discards at least three operations and 200 ms of warm-up per subject.

All table values are milliseconds. Each pair value is its invocation's median.

| Subject | Baseline 1 | Implementation 1 | Baseline 2 | Implementation 2 | Baseline 3 | Implementation 3 | Baseline median | Implementation median | Change |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| a22 C | 4.002 | 3.959 | 3.959 | 3.975 | 3.974 | 4.000 | 3.974 | 3.975 | +0.03% |
| a22 ship-tier | 5.333 | 5.271 | 5.412 | 5.277 | 5.299 | 5.273 | 5.333 | 5.273 | -1.13% |
| a22 dev-JIT | 79.124 | 78.994 | 78.987 | 78.949 | 79.039 | 78.939 | 79.039 | 78.949 | -0.11% |
| collect C | 33.719 | 32.580 | 32.771 | 32.482 | 32.781 | 32.568 | 32.781 | 32.568 | -0.65% |
| collect ship-tier | 35.872 | 35.329 | 35.667 | 35.489 | 36.821 | 35.638 | 35.872 | 35.489 | -1.07% |
| collect dev-JIT | 121.204 | 116.810 | 117.302 | 117.701 | 117.194 | 117.108 | 117.302 | 117.108 | -0.17% |
| dev-iteration | 3.972 | 3.954 | 3.954 | 3.945 | 4.208 | 3.951 | 3.972 | 3.951 | -0.53% |
| hot-reload | 0.500 | 0.503 | 0.523 | 0.503 | 0.506 | 0.500 | 0.506 | 0.503 | -0.59% |

These values are observations from one host. They carry no statistical significance claim.

## §121 implementation: final checks

| Check | Result |
|---|---|
| Pinned `cargo fmt --all --check` | Exit 0. |
| `cargo build --offline --locked --workspace --all-targets` | Exit 0; no warnings. |
| Reload tests alone | 34 pass; 0 fail. |
| HIR compile-mode site test | 1 pass; 0 fail. |
| LIR tests alone | 52 pass; 0 fail. |
| Full gate | One invocation; exit 0. |
| Additional source splits | None; every changed Rust file is below 2,000 lines. |
| Contract problems | None. |

```text
gate full aa33213636338ee8300d5a590e74a3455979b701 dirty:16 debug 1800/0/3 release 1797/0/3 skips 2/0 clippy 5/18/13 goldens-moved 0 exit 0
```
