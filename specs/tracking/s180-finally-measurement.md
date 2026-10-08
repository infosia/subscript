# §180: `finally` measurement

The pin is `3de68c37`. The measurement date is 2026-10-08.
Node is v24.18.0. The host uses Apple arm64 and Apple clang 21.0.0.
The prototype contradicts §115.3 rule 2 and collision C6.
It changes no contract. The final production tree equals the pin.

## A. JavaScript output

Each program below runs with Node. `print` means `console.log` with the same arguments.
The programs use modules, except the uncaught case, which uses stdin.
The generator-drop case invokes explicit GC with `--expose-gc` after the local scope ends.
`\n` denotes one LF byte. Empty output means zero bytes.
The table records stdout exactly. The uncaught case also writes the stderr block below.

| Program | JavaScript body | Node stdout | Exit |
|---|---|---|---:|
| normal | `try { print("body"); } finally { print("finally"); } print("end");` | `body\nfinally\nend\n` | 0 |
| return | `function f(){ try { print("body"); return 7; } finally { print("finally"); } } print(f());` | `body\nfinally\n7\n` | 0 |
| break | `while(true){ try { print("body"); break; } finally { print("finally"); } } print("end");` | `body\nfinally\nend\n` | 0 |
| continue | `for(let i=0;i<2;i++){ try { print(i); continue; } finally { print("finally"); } } print("end");` | `0\nfinally\n1\nfinally\nend\n` | 0 |
| caught | `try { try { throw new Error("old"); } finally { print("finally"); } } catch(e){ if(e instanceof Error) print(e.message); } print("end");` | `finally\nold\nend\n` | 0 |
| uncaught | `try { throw new Error("old"); } finally { print("finally"); } print("end");` | `finally\n` | 1 |
| return-return | `function f(){ try { return 1; } finally { print("finally"); return 2; } } print(f());` | `finally\n2\n` | 0 |
| return-throw | `function f(){ try { throw new Error("old"); } finally { print("finally"); return 2; } } print(f());` | `finally\n2\n` | 0 |
| throw-throw | `try { try { throw new Error("old"); } finally { print("finally"); throw new Error("new"); } } catch(e){ if(e instanceof Error) print(e.message); }` | `finally\nnew\n` | 0 |
| finally-break | `while(true){ try { throw new Error("old"); } finally { print("finally"); break; } } print("end");` | `finally\nend\n` | 0 |
| nested | `try { try { try { throw new Error("inner"); } finally { print("inner finally"); } } finally { print("outer finally"); } } catch(e){ if(e instanceof Error) print(e.message); }` | `inner finally\nouter finally\ninner\n` | 0 |
| nested-replace | `try { try { throw new Error("inner"); } finally { try { throw new Error("middle"); } finally { print("middle finally"); } } } catch(e){ if(e instanceof Error) print(e.message); }` | `middle finally\nmiddle\n` | 0 |
| catch-finally | `try { throw new Error("old"); } catch(e){ if(e instanceof Error) print(e.message); } finally { print("finally"); } print("end");` | `old\nfinally\nend\n` | 0 |
| catch-throws | `try { try { throw new Error("old"); } catch { print("catch"); throw new Error("new"); } finally { print("finally"); } } catch(e){ if(e instanceof Error) print(e.message); }` | `catch\nfinally\nnew\n` | 0 |
| async-no-catch | `async function failing(){ await Promise.resolve(); throw new Error("old"); } async function f(){ try { await failing(); } finally { print("finally"); } } try { await f(); } catch(e){ if(e instanceof Error) print("caller",e.message); } print("end");` | `finally\ncaller old\nend\n` | 0 |
| async-catch | `async function failing(){ await Promise.resolve(); throw new Error("old"); } async function f(){ try { await failing(); } catch(e){ if(e instanceof Error) print("catch",e.message); } finally { print("finally"); } } await f(); print("caller end");` | `catch old\nfinally\ncaller end\n` | 0 |
| async-finally-await | `async function failing(){ await Promise.resolve(); throw new Error("old"); } async function f(){ try { await failing(); } finally { print("finally before"); await Promise.resolve(); print("finally after"); } } try { await f(); } catch(e){ if(e instanceof Error) print("caller",e.message); }` | `finally before\nfinally after\ncaller old\n` | 0 |
| generator-break | `function* g(){ try { yield 1; print("resume"); } finally { print("finally"); } } for(const x of g()){ print(x); break; } print("end");` | `1\nfinally\nend\n` | 0 |
| generator-consumer-return | `function* g(){ try { yield 1; print("resume"); } finally { print("finally"); } } function f(){ for(const x of g()){ print(x); return; } } f(); print("end");` | `1\nfinally\nend\n` | 0 |
| generator-manual-return | `function* g(){ try { yield 1; print("resume"); } finally { print("finally"); } } const it=g(); print(it.next().value); print(it.return().done); print("end");` | `1\nfinally\ntrue\nend\n` | 0 |
| generator-exhausted | `function* g(){ try { yield 1; print("resume"); } finally { print("finally"); } } for(const x of g()){ print(x); } print("end");` | `1\nresume\nfinally\nend\n` | 0 |
| generator-drop | `function* g(){ try { yield 1; print("resume"); } finally { print("finally"); } } { const it=g(); print(it.next().value); } globalThis.gc(); print("end");` | `1\nend\n` | 0 |
| using-inner | `class R { [Symbol.dispose](){ print("dispose"); } } try { using r=new R(); print("body"); } finally { print("finally"); } print("end");` | `body\ndispose\nfinally\nend\n` | 0 |
| using-outer | `class R { [Symbol.dispose](){ print("dispose"); } } { using r=new R(); try { print("body"); } finally { print("finally"); } } print("end");` | `body\nfinally\ndispose\nend\n` | 0 |
| using-finally | `class R { [Symbol.dispose](){ print("dispose"); } } try { print("body"); } finally { using r=new R(); print("finally"); } print("end");` | `body\nfinally\ndispose\nend\n` | 0 |

The uncaught case writes this exact stderr:

```text
[stdin]:2
try { throw new Error("old"); } finally { print("finally"); } print("end");
      ^

Error: old
    at [stdin]:2:13
    at runScriptInThisContext (node:internal/vm:219:10)
    at node:internal/process/execution:451:12
    at [stdin]-wrapper:6:24
    at runScriptInContext (node:internal/process/execution:449:60)
    at evalFunction (node:internal/process/execution:283:30)
    at evalTypeScript (node:internal/process/execution:295:3)
    at node:internal/main/eval_stdin:51:5
    at Socket.<anonymous> (node:internal/process/execution:205:5)
    at Socket.emit (node:events:521:24)

Node.js v24.18.0
```

A normal exit preserves its completion after the finalizer.
A finalizer return replaces an earlier return or exception.
A finalizer throw replaces an earlier exception.
A finalizer break cancels the pending exception and leaves the loop.
Nested finalizers run from the inner scope to the outer scope.
A catch runs before its own finalizer. A catch exception still runs that finalizer.
An awaited failure runs the finalizer before the awaiting caller's handler.
An await inside the finalizer delays that handler until the finalizer completes.

A for-of break and a consumer return close the generator and run its finalizer.
A manual `return()` also runs the finalizer.
Exhaustion runs the finalizer through the normal body path.
A dropped generator runs no finalizer, even after explicit GC in this measurement.
A resource inside the try block disposes before the finalizer.
A resource outside the try statement disposes after the finalizer.
A resource inside the finalizer disposes at the end of the finalizer.

## B. The pin

### Disposal and fault transfer

These source programs run from temporary copies in the dev JIT, C AOT, and interpreter.
`using-trap` prints an array read through string interpolation.
The native runners keep pre-trap output in `TrapReport.stdout`.

```typescript
class R { [Symbol.dispose](): void { print("dispose"); } }
export function main(): void {
  try {
    using r = new R();
    print("body");
    const a: i32[] = [1];
    print(`${a[5]}`);
  } catch { print("handler"); }
  print("end");
}
```

```typescript
class R { [Symbol.dispose](): void { print("dispose"); } }
export function main(): void {
  try {
    using r = new R();
    print("body");
    throw new Error("old");
  } catch(e) {
    if(e instanceof Error) print(`handler ${e.message}`);
  }
  print("end");
}
```

| Program | Dev JIT output | C AOT output | Interpreter output | Fault |
|---|---|---|---|---|
| using-trap | `body\n` | `body\n` | `body\n` | IndexOutOfBounds in all three tiers; no handler or dispose |
| using-exception | `body\ndispose\nhandler old\nend\n` | Same | Same | None |

The pin rejects the normal-finally program with S010 in all three tiers.

### Current lowering and a complete finally form

§60.1 rule 8 describes the retired checker rewrite.
§97 retains nullable guards and the disposal order.
§115.5 rules 5–7 replace the old rewrite with shared HIR-to-LIR exit actions.
The current code follows that replacement.

| File and function | Current action | Finally requirement |
|---|---|---|
| `compiler/src/check/using_scope.rs`, `structure` and `nested` | Build one `Stmt::Using` scope around each resource body | Keep a finalizer body and its lexical environment in one scope node |
| `compiler/src/hir/using.rs`, `UsingBinding::hook` | Produce the hook statement and nullable/active guards | Supply the finalizer statements without a resource allocation |
| `codegen/src/lir/stmt.rs`, `lower_statement` | Evaluate the return operand before `leave` | Preserve the original return value while the finalizer runs |
| `codegen/src/lir/using.rs`, `leave`, `place_exit_actions`, `lower_using` | Run hooks at return, break, continue, and fall-through | Run finalizers on those same scope exits, from inner scope to outer scope |
| `codegen/src/lir/builder.rs`, `terminate_return` | Hold a counted return value through exit actions | Release that held value if a finalizer replaces the return |
| `codegen/src/lir/exception.rs`, `resolve_raise_edges`, `lower_try` | Give each raise a landing and transfer live mutable bindings | Add a catch-all edge around both the try body and its catch body |
| `codegen/src/lir/using.rs`, `lower_exception_edge` | Park the exception, run hooks, and resume the same exception | Preserve the object, report text, and last-throw position on finalizer fall-through |
| `runtime/src/exception.rs`, `Context::park_exception`, `Context::resume_exception` | Push and pop the Context's exception stack | Use a frame-owned exception slot when a finalizer can suspend |
| `codegen/src/interpreter/instruction.rs`, instruction dispatch | Keep parked exception tuples in the interpreter | Use the same frame-owned slot and transfer rules |

The Context word uses 0 for clear, 1 for trap, and 2 for pending exception (§115.6).
A native raise transfers to a catch landing only for an exception.
A trap leaves through the trap path and runs no cleanup.
The interpreter carries a distinct exception error and follows the same edges.

A catch-all plus source `throw e` does not preserve the last-throw position.
`ExceptionPark` and `ExceptionResume` preserve that position, as §116.1 rule 4 requires.
The identity control below also proves that the object survives collection during the finalizer.

A complete finalizer needs an explicit completion: fall-through, return, throw, break, or continue.
On fall-through, it restores the prior completion.
On return, it discards the prior completion and returns the new value.
On throw, it discards the old exception and propagates the new object's last-throw position.
On break or continue, it discards the prior completion and takes the new lexical target.
Each replacement still runs applicable outer finalizers and resource hooks.
The active finalizer must leave the cleanup stack before its body lowers; otherwise, an exit can run it twice.
A loop inside a finalizer needs its own local break and continue targets.
The prototype rejects those statements conservatively instead of modeling their target depth.

### Generator choices under §176

The current native last release calls `Context::generator_release` in `runtime/src/context/counted.rs`.
It releases the saved owners and deletes the frame. It calls no resume function.
`Context::async_release` in `runtime/src/context/async_scheduler.rs` routes generator releases there.
The interpreter's `release_coroutine` in `codegen/src/interpreter/counted.rs` removes the generator and releases `generator_owners`.
`sweep_generator` releases saved owners during collection. It runs no body instruction.

The release paths include lexical exits, assignments, container removals, `Context.free`, and explicit collection.
Native `take_object_releases`, `counted_release_leaves`, and `unreachable_releases` handle container and collection releases.
`runtime/src/context/memory.rs` supplies these releases during collection.
The interpreter's `collect_interpreter` in `codegen/src/interpreter/roots.rs` handles its frame registry.
These paths can run inside a host call that invokes the Context API.
A last release can therefore occur inside a host C frame or during collection.

| Choice | Evidence | Cost and consequence |
|---|---|---|
| (a) Run finally on every last release | The current release functions contain no script resume. Node's plain drop runs no finalizer. | Add script entry to every last-release path and to collection. This changes §176 and diverges from Node's plain drop. |
| (b) Run no finally on a drop | The prototype prints `1\nend\n` on plain drop, break, and consumer return in all tiers. | Keep the current release descriptions. Plain drop agrees with Node; break and consumer return omit Node's `finally` line. |
| (c) Close on for-of break/return; run no finally on plain drop | Node prints `1\nfinally\nend\n` for both consumer exits. The current prototype omits `finally`. | Add a distinct close continuation at the loop exit before holder release. Keep plain release free of script code. |

Choice (a) also needs script roots, trap handling, collection reentry rules, and protection against frame resurrection.
A finalizer can allocate, call a host function, collect again, throw, or publish a reference.
The release paths currently assume that frame retirement does not execute these script operations.
The generator exception boundary currently traps (§115.4 item 3).
Choice (c) therefore also needs a decision about an exception from close and its transfer to the consumer.
A shared alias needs a closed state even when its holder count stays above zero.
Resuming the ordinary next state cannot implement close: it also runs the body statements after the yield.
The prototype's exhausted generator prints `resume`, but its early-drop cases do not.

The measurement implements choice (b) only.
It measures no implementation cost for choices (a) and (c).
The listed costs for those choices identify required code paths; they are not timing estimates.
The §176 note's `B-finally-break` and `B-finally-return` rows reject both forms at the pin.
Its Node rows print `v 1\nclose\nend\n`, consistent with Part A.

## C. Restricted prototype

The prototype reuses `Stmt::Using` with one synthetic i32 binding and a finalizer body in `UsingBinding`.
It allocates no synthetic resource object.
The shared lowering copies the finalizer onto normal exit edges and uses the existing parked exception edge.
The HIR child walk includes the finalizer statements.
The narrowing-effect walk includes the finalizer's effects.
The using-scope pass also structures resource declarations inside the finalizer.
No production codegen or runtime file changes.

The checker rejects explicit return, throw, break, and continue anywhere in a finalizer with S010.
This restriction avoids completion replacement and recursive cleanup in this small prototype.
It also rejects local loop exits and a caught throw inside a finalizer.
Those conservative rejections are prototype limits, not proposed language rules.
A call inside the finalizer can still raise; the extra control below exposes the incorrect transfer.

The language copies use `print(string)`, explicit function types, and string interpolation for numbers.
The async copies use `tick(): Promise<void>` instead of `Promise.resolve()`.
Each CLI invocation reads a source copy from the temporary directory.
The output comparison reads the exact program bytes, including pre-trap output stored by the native runner.

| Part A programs | Three-tier result | Node comparison |
|---|---|---|
| normal, return, break, continue | Accept; identical output in all tiers | Exact match |
| caught, nested, catch-finally, catch-throws | Accept; identical output in all tiers | Exact match |
| uncaught | `finally\n`; UncaughtException, with the original throw at 2:38 in the typed copy | Same stdout and Error message; native/interpreter trap report differs from Node stderr |
| async-no-catch, async-catch, async-finally-await | Accept; identical output in all tiers | Exact match |
| return-return, return-throw, throw-throw, finally-break, nested-replace | S010 in all tiers | Node accepts and replaces the prior completion |
| using-inner, using-outer, using-finally | Accept; identical output in all tiers | Exact match |
| generator-break, generator-consumer-return | `1\nend\n` in all tiers | Each omits Node's `finally\n` |
| generator-exhausted, generator-drop | Accept; identical output in all tiers | Exact match |
| generator-manual-return | S100 in all tiers: return is outside the coroutine surface | Node accepts and closes |

### Additional controls

These controls run in Node and all three prototype tiers.
The Node copies remove type annotations and replace `Context.suspend()` with `Promise.resolve()`.
The identity copy replaces `Context.collect()` with explicit GC.

| Control | Node program output | Three-tier program output | Result |
|---|---|---|---|
| concurrent finalizers | `before A\nbefore B\nafter A\nafter B\na A\nb B\n` | `before A\nbefore B\nafter A\nafter B\na B\nb A\n` | Both exceptions reach the wrong caller |
| cleanup callee throws | `finally\nnew\n` | `finally\n`, then DisposeRaisedDuringExit, kind 30 | A finalizer call traps instead of replacing the exception |
| identity with collection | `finally\ntrue\n` | Same | The same exception object remains live |
| shadowed return local | `99\n7\n99\n` | Same | Cleanup reads the outer local; the return keeps the saved inner value |
| continue with finalizer read | `0\nfinally 0\n1\nfinally 1\nend\n` | Same | The finalizer reads the iteration value before the for-step changes it |

The concurrent control exposes the Context-wide LIFO park stack in native execution.
The interpreter's shared park vector gives the same wrong result.
Suspending finalizers need exception storage owned by each frame.
A successful isolated await does not prove that shared storage is correct.

The cleanup-call control exposes `lower_exception_edge`'s disposal rule.
That edge turns a cleanup raise into trap 30 while an old exception is parked.
A finally edge needs exception replacement instead.
The prototype does not implement these two required form changes.

The continue control changes Part A's finalizer to ``print(`finally ${i}`)`` in the typed copy.
Its Node copy uses the same interpolation.

The concurrent control is:

```typescript
async function fail(tag: string): Promise<void> { throw new Error(tag); }
async function f(tag: string): Promise<void> {
  try { await fail(tag); }
  finally { print(`before ${tag}`); await Context.suspend(); print(`after ${tag}`); }
}
export async function main(): Promise<void> {
  const a = f("A"); const b = f("B");
  try { await a; } catch(e) { if(e instanceof Error) print(`a ${e.message}`); }
  try { await b; } catch(e) { if(e instanceof Error) print(`b ${e.message}`); }
}
```

The cleanup-call control is:

```typescript
function fail(): void { throw new Error("new"); }
export function main(): void {
  try {
    try { throw new Error("old"); }
    finally { print("finally"); fail(); }
  } catch(e) { if(e instanceof Error) print(e.message); }
}
```

### Corpus

| Command | Result | Test execution seconds |
|---|---|---:|
| `cargo test --offline -p subscript-codegen --test golden` | 35 pass; zero fail | 43.81 initially; 38.07 after the final scope-pass change |
| `cargo test --offline -p subscript-codegen --test lir lir_interpreter_profile_matches_corpus_goldens -- --exact --nocapture` | 269 run; 269 golden matches; 66 declared exclusions | 10.65 |

The debug interpreter test excludes the one benchmark entry, a22, through its standing selection rule.
The release cost harness uses that same selection in both versions.
It checks all 269 selected outputs against the committed goldens.
It runs entries serially and excludes Rust compilation.
The whole-process span includes source loading, checking, lowering, interpretation, and golden comparison.
Each binary runs alone. No Rust build or test overlaps a cost sample.

| Version | Sample 1, seconds | Sample 2, seconds | Sample 3, seconds | Best, seconds |
|---|---:|---:|---:|---:|
| pin | 3.139250667 | 3.470421542 | 2.931441125 | 2.931441125 |
| prototype | 2.870097125 | 2.958908833 | 6.549634208 | 2.870097125 |

The best-to-best interpreter ratio is 0.979074. Each of the six samples reports 269 golden matches.

The prototype's third sample takes more than twice its best sample.
These measurements show substantial host timing variation.
The table retains every measured sample; it discards no slow sample.
The release cost set excludes a22 and is not the complete 270-entry release selection.

### Benchmark cost

All Rust measurement binaries use the release profile.
Each standard and async workload version runs alone, with three discarded warm-up calls and three measured calls.
The tables retain all three measured execution samples and use the minimum for each comparison.
The ten standard workloads and a22 use `jit_bench` and `benchmarks/aot-entry.c`.
Those spans time the exported main call and exclude compilation and Context setup.
The three async workloads use `benchmarks/async-cost-entry.c` in C AOT.
That span includes Context setup, execution, all host checkpoints, and Context release.
The standard JIT entry timer excludes the checkpoints, so this note gives no async JIT cost comparison.
The bound-call C AOT control uses its native backend with three measured samples instead of fifteen.
Both versions use the same temporary backend change and its 200-millisecond warm-up floor.
Its span times 1,000 bound-call pairs; its checksum is 60,000.
No measured workload source uses a generator or finally.

All pin and prototype C sources, host headers, and allocation headers match byte for byte for these workloads.
Each version links the same release runtime archive and uses C11, `-O2`, `-fwrapv`, and `-ffp-contract=off`.
All workload checksums match across versions and tiers. Each standard and async AOT run reports `checksum-stable 1`.
The bound-call runs each report `checksum=60000`.

| Workload | Tier | Pin samples, milliseconds | Prototype samples, milliseconds | Best ratio | Checksum |
|---|---|---|---|---:|---|
| callbacks | C AOT | 43.835000, 43.617000, 46.311000 | 53.641000, 52.302000, 52.743000 | 1.199120 | `-662567840` |
| callbacks | Dev JIT | 338.480333, 338.135875, 336.913375 | 404.278708, 448.346500, 419.228958 | 1.199949 | `-662567840` |
| collect | C AOT | 40.547000, 40.406000, 42.550000 | 46.543000, 45.772000, 46.447000 | 1.132802 | `1332546592` |
| collect | Dev JIT | 135.308542, 153.864375, 141.721125 | 151.674458, 164.730917, 158.484959 | 1.120953 | `1332546592` |
| fib-loop | C AOT | 36.638000, 36.413000, 36.595000 | 41.048000, 41.365000, 41.463000 | 1.127290 | `973132000` |
| fib-loop | Dev JIT | 86.754292, 86.458084, 87.697666 | 95.711042, 96.298083, 95.564000 | 1.105322 | `973132000` |
| fib-recursive | C AOT | 4.416000, 4.418000, 4.447000 | 5.033000, 5.018000, 4.935000 | 1.117527 | `1346269` |
| fib-recursive | Dev JIT | 9.845458, 9.679083, 9.637167 | 10.826416, 10.875625, 10.877250 | 1.123402 | `1346269` |
| mandelbrot | C AOT | 149.663000, 152.848000, 156.910000 | 163.785000, 165.370000, 165.041000 | 1.094359 | `43027996` |
| mandelbrot | Dev JIT | 160.482708, 158.302125, 160.729042 | 176.036834, 174.361625, 174.452750 | 1.101448 | `43027996` |
| particles | C AOT | 96.832000, 99.507000, 99.413000 | 107.382000, 107.393000, 107.511000 | 1.108952 | `1712845248` |
| particles | Dev JIT | 591.584584, 589.344417, 589.559042 | 660.178458, 642.030292, 646.284667 | 1.089397 | `1712845248` |
| primes | C AOT | 28.653000, 28.016000, 28.907000 | 28.816000, 28.838000, 28.862000 | 1.028555 | `41538` |
| primes | Dev JIT | 41.874875, 41.834375, 44.826750 | 41.589792, 42.522833, 41.910584 | 0.994154 | `41538` |
| queen | C AOT | 32.510000, 32.751000, 32.340000 | 34.215000, 34.322000, 34.260000 | 1.057978 | `73712` |
| queen | Dev JIT | 47.053834, 47.049250, 48.174875 | 47.710000, 47.688542, 48.409833 | 1.013588 | `73712` |
| sort | C AOT | 26.578000, 23.963000, 25.569000 | 24.070000, 23.493000, 23.817000 | 0.980386 | `3672124540` |
| sort | Dev JIT | 66.502375, 58.987333, 64.534083 | 45.917583, 45.895208, 46.571583 | 0.778052 | `3672124540` |
| tree | C AOT | 288.338000, 271.600000, 329.469000 | 143.939000, 144.292000, 143.202000 | 0.527253 | `3932130` |
| tree | Dev JIT | 656.069875, 869.837333, 684.624417 | 602.510583, 595.494208, 610.531042 | 0.907669 | `3932130` |
| a22-matrix-propagation | C AOT | 14.049000, 8.263000, 8.168000 | 7.097000, 7.037000, 7.088000 | 0.861533 | `40021.875` |
| a22-matrix-propagation | Dev JIT | 121.826459, 117.555333, 126.909584 | 113.079000, 115.054667, 111.636167 | 0.949648 | `40021.875` |
| async-deep-chains | C AOT | 13.785000, 13.673000, 13.325000 | 12.374000, 12.485000, 12.502000 | 0.928630 | `total=80000` |
| async-held-handles | C AOT | 6.889000, 6.516000, 5.514000 | 5.150000, 5.338000, 5.144000 | 0.932898 | `total=39980000` |
| async-settled-awaits | C AOT | 18.794000, 18.980000, 19.021000 | 17.658000, 17.774000, 18.436000 | 0.939555 | `total=200000` |

| bound-call | C AOT | 0.007042, 0.007000, 0.006917 | 0.014875, 0.014917, 0.014875 | 2.150499 | `60000` |

Several best ratios exceed 1.05, while tree AOT falls to about half the pin cost.
Identical emitted C excludes a finalizer code change as the cause of the AOT differences.
The sample set does not establish a causal execution regression or improvement.
The interpreter comparison includes the changed HIR metadata and checker path.
A complete implementation cost still needs frame-owned exception storage and completion replacement.
This prototype cost does not price those absent changes.

## Remaining divergences

1. A trap runs no finalizer, as it runs no dispose hook today.
2. Explicit finalizer return, throw, break, and continue fail with S010.
3. A cleanup callee's exception during exception exit traps instead of replacing the exception.
4. Parallel suspending finalizers exchange exceptions through shared parked storage.
5. A for-of break or consumer return drops a generator without its finalizer.
6. A generator's manual return remains outside the accepted coroutine surface.
7. The measurement covers 269 interpreter entries and excludes the release benchmark entry.
8. The cost set excludes bound-call JIT and async JIT execution costs.

The isolated Part A items 1–4 pass for the accepted forms.
The two additional controls prove that this restricted prototype cannot serve as a complete finally implementation.

## File set and final tree

The temporary prototype changes these production files:

- `compiler/src/check/exception.rs`
- `compiler/src/check/using_scope.rs`
- `compiler/src/hir/effects.rs`
- `compiler/src/hir/expression.rs`
- `compiler/src/hir/using.rs`

The temporary measurement harness adds `codegen/examples/s180_measure.rs`.
The final revert restores all five production files and removes that harness.
Temporary source copies, binaries, raw outputs, and the prototype patch stay outside the repository.
No corpus source, expected output, codegen source, runtime source, or contract file changes in the final tree.
The only remaining file is this new note, `specs/tracking/s180-finally-measurement.md`.
No command runs outside the sandbox. No command runs `tools/gate.sh`. No commit exists for this measurement.

`git status --short` after the revert prints:

```text
?? specs/tracking/s180-finally-measurement.md
```

`git diff --stat` after the revert prints no output.
`git diff --exit-code HEAD` returns 0.
`git diff --check` and `sh tools/hygiene.sh` return 0.


## Implementation: exits, completions, async

The implementation supplies rules 1–8, including generator close, `a351`, and `t109`.
The frame-owned exception slots preserve concurrent finalizer completions.

### Representation and exit behavior

A HIR cleanup scope has an optional finalizer and captures its lexical
bindings. The shared `using` exit-action walk removes an active finalizer
before lowering its body. LIR `FinalizerEnter` records fall-through,
return with its evaluated value, throw with its saved exception, or a
break/continue target. A finalizer completion replaces the prior one;
outer cleanup actions then follow the replacement. Local finalizer loops
keep their own targets. A counted return is acquired before cleanup;
replacement releases it, including same-owner return replacement.

The checker includes finalizer statements in narrowing, local assignment
analysis, captures, effects, fall-through, and warning traversal. Stores
on a terminating catch path also affect the finalizer's entry facts; a
regression rejects a nullable receiver made null before `return` in catch.

A throwing entry snapshots object, original report text, and last-throw
position into three ordinary LIR locals. Suspension liveness promotes
them to the coroutine frame; the object and text remain collection roots.
JIT and emitted C store these in the native frame, and the interpreter
stores them in its invocation frame. The interpreter's position table is
immutable metadata, deduplicated by source position, rather than shared
pending completion state. Restore loads the saved frame values.
The verifier rejects a missing completion, invalid completion payloads,
and restores whose operands are not local loads; existing local and
coroutine-storage verification checks promotion across suspension.

The Context park stack remains for `using` alone. Dispose cannot suspend:
`compiler/src/check/class_shape.rs::resolve_class_method` rejects async dispose
and requires a synchronous zero-argument `void` signature. Accordingly no
suspendable hook needs conversion to frame slots. A hook that throws while
an exception is pending, including from inside its finalizer, traps with
kind 30. Finalizer code itself may replace the exception.

### Evidence and verification

Both new corpus entries failed at `491645fb` with S010 in JIT, C AOT, and interpreter.
The entry headers record this red evidence.

Stock TypeScript 5.9.2 accepted both entries with the ambient prelude
(`--noEmit --strict --target esnext --lib esnext`). Node v24.18.0 matched
all 30 lines of `a349`; its header is `js-comparable: yes`. `a350` cites C11
for explicit `Context.suspend()` and `Context.collect()` and is excluded
from JS comparison. The measurement's concurrent control supplies the
Node reference for ordering. Green corpus checks compare both new entries
in JIT, C-AOT, and interpreter. The callee-throw control prints `finally`
and `new`; identity across collection, shadowed return, continue reads,
replacement completions, counted returns, and nested `using` are covered.

The final offline test results are:

| Package | Passed | Ignored | Targets, including documentation |
| --- | ---: | ---: | ---: |
| compiler | 1104 | 1 | 67 |
| runtime | 447 | 1 | 17 |
| cli | 53 | 0 | 7 |
| codegen | 877 | 1 | 66 |

The CLI library, binary, four integration targets, and documentation tests pass.
Its 14 gate integration tests are excluded because they invoke `tools/gate.sh`.
The codegen profile declares its matrix benchmark exclusion.
`cargo fmt --check` and the final offline release CLI build pass.
The native-frame inventory recognizes the `close_output` ABI header field.
Its independent payload check requires a 40-byte generator header.
The void-generator test checks every next call's non-null result address.
The full suite includes both updated native-form checks.


The debug control costs below include process startup and native C compilation.
They exclude Rust build time. These costs do not compare release performance.

| Test | Wall time | Result |
| --- | ---: | --- |
| `finally_tests::s180_c_suspended_finalizers_have_distinct_frame_slots` | 0.558 s | pass |
| `finally_tests::s180_dispose_during_a_pending_finalizer_exception_still_traps` | 0.555 s | pass |
| `finally_tests::s180_finalizer_without_completion_is_rejected` | 0.007 s | pass |
| `finally_tests::s180_replaced_returns_and_local_loops_keep_owner_counts` | 0.552 s | pass |
| `interpreter::tests::s180_interpreter_suspended_finalizers_have_distinct_frame_slots` | 0.007 s | pass |
| `jit::finally_tests::s180_jit_suspended_finalizers_have_distinct_frame_slots` | 0.023 s | pass |
| `check::exception::finally_tests::terminating_catch_stores_reach_the_finalizer` | 0.228 s | pass |
| `exception::tests::frame_payload_exports_preserve_report_and_last_throw_position` | 0.008 s | pass |

The existing HIR child-walk, retired rejection inventory, accepted-finally,
and independent execution-fact tests were extended rather than adding
extra corpus scans. The one-finalizer and two-finalizer frame controls
have the same program shape in each tier and read the built frame slots;
they also assert that the Context parked-exception stack is empty.

The release cost section below records the acceptance result and the separate stage attribution.
The amended acceptance replaces the earlier 1.05 bound with the recorded spread and code-cause test.

### Changed files

Each file below is changed in this implementation, including additions
and the retired rejection deletion.

| File | Reason |
| --- | --- |
| `codegen/src/cemit/graph.rs` | JIT/C exception snapshot/restore lowering or runtime symbol declarations. |
| `codegen/src/finally_tests.rs` | Verifier, ownership, trap, or native/interpreter frame-storage controls. |
| `codegen/src/interpreter.rs` | Frame-owned exception values and snapshot/restore execution. |
| `codegen/src/interpreter/instruction.rs` | Frame-owned exception values and snapshot/restore execution. |
| `codegen/src/interpreter/tests.rs` | Verifier, ownership, trap, or native/interpreter frame-storage controls. |
| `codegen/src/jit.rs` | Register focused finally test modules. |
| `codegen/src/jit/finally_tests.rs` | Verifier, ownership, trap, or native/interpreter frame-storage controls. |
| `codegen/src/jit/symbols.rs` | JIT/C exception snapshot/restore lowering or runtime symbol declarations. |
| `codegen/src/lib.rs` | Register focused finally test modules. |
| `codegen/src/lir.rs` | Shared cleanup lowering, completion ownership, or verifier integration. |
| `codegen/src/lir/builder.rs` | Shared cleanup lowering, completion ownership, or verifier integration. |
| `codegen/src/lir/exception.rs` | Shared cleanup lowering, completion ownership, or verifier integration. |
| `codegen/src/lir/stmt.rs` | Shared cleanup lowering, completion ownership, or verifier integration. |
| `codegen/src/lir/using.rs` | Shared cleanup lowering, completion ownership, or verifier integration. |
| `codegen/src/lir/verify_instruction.rs` | Shared cleanup lowering, completion ownership, or verifier integration. |
| `codegen/src/lir/verify_lifetime.rs` | Shared cleanup lowering, completion ownership, or verifier integration. |
| `codegen/src/lir/verify_narrowing.rs` | Shared cleanup lowering, completion ownership, or verifier integration. |
| `codegen/src/lir/verify_raise.rs` | Shared cleanup lowering, completion ownership, or verifier integration. |
| `codegen/src/lower/func/instruction.rs` | JIT/C exception snapshot/restore lowering or runtime symbol declarations. |
| `codegen/src/lower/mod.rs` | JIT/C exception snapshot/restore lowering or runtime symbol declarations. |
| `codegen/tests/lir-goldens/corpus.txt` | Owning-test capture of new finalizer coroutine LIR. |
| `codegen/tests/support/lir_facts.rs` | Independent HIR execution-fact oracle must model finalizer placement and completions. |
| `codegen/tests/support/lir_facts/boundary.rs` | Independent HIR execution-fact oracle must model finalizer placement and completions. |
| `codegen/tests/support/lir_facts_using.rs` | Independent HIR execution-fact oracle must model finalizer placement and completions. |
| `compiler/src/check/assignment_flow.rs` | Finalizer checking, completion flow, or exhaustive analysis traversal. |
| `compiler/src/check/capture.rs` | Finalizer checking, completion flow, or exhaustive analysis traversal. |
| `compiler/src/check/exception.rs` | Finalizer checking, completion flow, or exhaustive analysis traversal. |
| `compiler/src/check/fallthrough.rs` | Finalizer checking, completion flow, or exhaustive analysis traversal. |
| `compiler/src/check/initializer_finish.rs` | Finalizer checking, completion flow, or exhaustive analysis traversal. |
| `compiler/src/check/receiver_capture.rs` | Finalizer checking, completion flow, or exhaustive analysis traversal. |
| `compiler/src/check/rejection.rs` | Remove the retired finally rejection and its witnesses. |
| `compiler/src/check/rejection_programs.txt` | Remove the retired finally rejection and its witnesses. |
| `compiler/src/check/rejection_sites.rs` | Remove the retired finally rejection and its witnesses. |
| `compiler/src/check/rejection_targets.txt` | Remove the retired finally rejection and its witnesses. |
| `compiler/src/check/rejection_witness_index.rs` | Remove the retired finally rejection and its witnesses. |
| `compiler/src/check/rejection_witness_sites.rs` | Remove the retired finally rejection and its witnesses. |
| `compiler/src/check/using_scope.rs` | Finalizer checking, completion flow, or exhaustive analysis traversal. |
| `compiler/src/hir.rs` | Shared cleanup-scope finalizer field and complete child/effect traversal. |
| `compiler/src/hir/effects.rs` | Shared cleanup-scope finalizer field and complete child/effect traversal. |
| `compiler/src/hir/expression.rs` | Shared cleanup-scope finalizer field and complete child/effect traversal. |
| `compiler/src/hir/tests.rs` | Shared cleanup-scope finalizer field and complete child/effect traversal. |
| `compiler/src/lir.rs` | Shared completion record and exception snapshot/restore instructions. |
| `compiler/src/warn.rs` | Warning analyses must visit finalizer statements. |
| `compiler/tests/corpus_reject.rs` | Remove retired r233 from the strict rejection inventory. |
| `compiler/tests/exceptions.rs` | Replace obsolete finally rejection with acceptance; preserve suspension checks. |
| `corpus/accept/a349-finally-exits-and-completions.expected` | Acceptance source or exact output. |
| `corpus/accept/a349-finally-exits-and-completions.ts` | Acceptance source or exact output. |
| `corpus/accept/a350-finally-async.expected` | Acceptance source or exact output. |
| `corpus/accept/a350-finally-async.ts` | Acceptance source or exact output. |
| `corpus/reject/r233-finally.ts` | Delete retired r233 finally rejection. |
| `generated-docs/corpus-index.md` | Generated corpus inventory from the documentation generator. |
| `runtime/src/exception.rs` | Frame snapshot/restore ABI, original report/position preservation, runtime test. |
| `specs/blocks/collisions.md` | C6 finally acceptance and retired r233 mapping. |
| `specs/tracking/s180-finally-measurement.md` | Implementation evidence, costs, limitations, and complete changed-file inventory. |

## Implementation: generator close and cost

`Stmt::GeneratorForOf` preserves the subject, element binding, body, and position.
The checker, narrowing, capture, holder, layout, warning, and child analyses consume this statement.
The LIR builder separates exhaustion from consumer exits and evaluates the subject once.
A same-loop `continue` advances the generator without close.
A consumer `break`, return, or exception runs close before the hidden holder release.

Each yield carries a `GeneratorClose` description in LIR liveness.
Its continuation restores the suspended values and runs the lexical cleanup actions before exhaustion.
A separate HIR walk derives the required finalizer positions, innermost first.
The verifier checks the dispatch, each close path, and the lexical finalizer order.
A yield in a close finalizer ends close. Its next invocation resumes that finalizer.
A closed alias returns done. A plain drop runs no finalizer.
The close operation releases the counted value that a finalizer yield returns and the consumer discards.
The void-generator emission test checks each next call's own non-null done address; NULL now selects the §180 generator close continuation.

The native generator header adds the close result pointer at offset 32.
The payload starts at offset 40 instead of 32. Each payload field moves by eight bytes.
The state, epoch, resume, holder count, and cleanup offsets remain 0, 4, 8, 16, and 24.
The runtime layout test and generated C assertions check these offsets.

`a351` covers nested finalizers, consumer break, return, exception, exhaustion, plain drop, closed aliases, and a finalizer yield.
The contract binary at `491645fb` rejects it with S010 at 11:13 and 18:11.
The same binary rejects `t109` with S010 at 11:11.
The entries carry the pin lines. TypeScript 5.9.2 accepts both with the repository prelude.
Node v24.18.0 matches all `a351` golden bytes.
`t109` traps at its throw, 11:13, with `Error: close`, in the three tiers.
C6 lists the entries and the generator exception divergence.

The verifier test builds a close graph without its outer finalizer marker and preserves the independent lexical requirement.
It reads the missing-finalizer diagnostic. Its measured debug cost is 0.016 seconds.
The three-tier `a351` test costs 1.952 seconds in the final control run.
The discarded counted-yield control costs 1.679 seconds and checks the uncaught-exception position in both native tiers.
The interpreter also traps with the same error text.

`cargo fmt --check` passes. The offline locked workspace build passes with no warning line.
Workspace Clippy passes. The actual library counts at HEAD and the tree are compiler 5, runtime 18, and codegen 12.
The gate permits 7, 18, and 13. The warning-kind and source-file comparison finds no new warning.
The HIR fact witness now counts close exits at yields and follows frame-owned exception snapshots through their restore edges.

The compiler suite passes 1,104 tests across 67 targets. The runtime suite passes 447 tests across 17 targets.
Each suite has one ignored test. The selected CLI suite passes 53 tests across seven targets.
The final close controls pass 9 tests. The final C emission target passes 105 tests.
The final LIR target passes 57 tests, including the interpreter corpus, trap positions, HIR facts, and text golden.
The JIT/C AOT golden sweep checks all 339 entries with no output difference.
The full codegen suite passes 877 tests across 66 targets, with 1 ignored test.
The documentation generator updates the corpus index. No existing output golden changes.

The LIR text changes for generator suspension dispatch, close paths, and the retained generator for-of statement.
The changed entry ids are:

a146-scoped-locals, a149-suspension-state, a164-frame-class-locals, a20-coroutine-generator, a215-user-view-method-names, a216-generator-escapes-its-call, a217-generator-in-a-class-field, a220-generator-two-references, a221-generator-two-distinct, a222-generator-field-replaced, a223-generator-in-an-array, a224-generator-as-a-map-value, a225-generator-exhausted-in-storage, a259-try-holds-yield, a309-generator-done-null, a310-generator-done-reference-controls, a311-generator-done-scalar-zero, a312-generator-done-value-zero, a313-generator-done-wire-zero, a327-void-generator, a329-initializer-inference, a339-counted-generator-holders, a346-dropped-generator-frame, a349-finally-exits-and-completions, a350-finally-async, a351-generator-close-runs-finally, a79-for-of-generator.

### Release cost against `3de68c37`

The release selection covers 269 common interpreter entries, with golden checks.
It excludes the matrix benchmark, declared interpreter exclusions, and the three new finally entries.
The measurement runs each binary alone. Each subject keeps its first three timed samples.
Each workload uses three warm-ups before those samples. The result uses the best of three.
The native `bound-call` measurement uses its boundary mirror and three timed samples after the backend warm-up floor.
No sample was repeated to change a result.

| Subject | Pin samples (ms) | Tree samples (ms) | Best ratio |
| --- | --- | --- | --- |
| Interpreter corpus | 2854.998500, 2119.712334, 2666.284084 | 2742.953542, 2226.936250, 2314.020250 | 1.050584 |

The interpreter wall samples, including process start, are:

- Pin: 2877.559000, 2127.500542, 2673.161792 ms.
- Tree: 3422.546625, 2236.125250, 2320.815459 ms.

| Workload | Tier | Pin samples (ms) | Tree samples (ms) | Best ratio |
| --- | --- | --- | --- | --- |
| callbacks | C AOT | 114.885000, 77.264000, 66.712000 | 68.771000, 58.686000, 64.085000 | 0.879692 |
| callbacks | Dev JIT | 493.018084, 724.670542, 802.528667 | 498.630958, 480.230000, 475.115792 | 0.963688 |
| collect | C AOT | 58.458000, 58.872000, 65.381000 | 74.854000, 77.038000, 82.027000 | 1.280475 |
| collect | Dev JIT | 208.690875, 225.155291, 216.435875 | 412.772875, 251.966792, 179.314959 | 0.859237 |
| fib-loop | C AOT | 57.479000, 68.454000, 61.357000 | 54.127000, 70.241000, 51.980000 | 0.904330 |
| fib-loop | Dev JIT | 113.866417, 119.198125, 177.667625 | 150.335000, 163.386500, 153.218000 | 1.320275 |
| fib-recursive | C AOT | 6.833000, 8.312000, 6.685000 | 7.111000, 5.956000, 6.240000 | 0.890950 |
| fib-recursive | Dev JIT | 12.575375, 12.686667, 13.447625 | 12.318334, 13.113125, 12.914125 | 0.979560 |
| mandelbrot | C AOT | 190.595000, 183.703000, 192.658000 | 186.628000, 193.288000, 198.452000 | 1.015922 |
| mandelbrot | Dev JIT | 200.141250, 191.364250, 200.692500 | 312.670417, 357.065667, 404.300291 | 1.633902 |
| particles | C AOT | 158.797000, 141.817000, 121.562000 | 134.752000, 139.317000, 119.630000 | 0.984107 |
| particles | Dev JIT | 889.859875, 1230.849417, 1213.138042 | 904.760916, 1254.728667, 1139.014959 | 1.016745 |
| primes | C AOT | 50.463000, 47.472000, 63.726000 | 30.877000, 32.420000, 31.401000 | 0.650426 |
| primes | Dev JIT | 54.226959, 52.449750, 54.872084 | 69.761583, 73.066292, 61.001500 | 1.163047 |
| queen | C AOT | 40.940000, 41.040000, 40.295000 | 39.426000, 42.244000, 40.341000 | 0.978434 |
| queen | Dev JIT | 53.845917, 54.971250, 55.032333 | 53.322583, 53.520625, 53.912833 | 0.990281 |
| sort | C AOT | 27.096000, 31.429000, 31.160000 | 29.551000, 30.030000, 29.701000 | 1.090604 |
| sort | Dev JIT | 54.653916, 51.979667, 50.568833 | 59.893166, 65.402625, 61.436375 | 1.184389 |
| tree | C AOT | 323.059000, 350.495000, 282.687000 | 214.822000, 212.070000, 198.625000 | 0.702632 |
| tree | Dev JIT | 970.040458, 961.973791, 1623.673333 | 1841.417167, 939.004417, 899.486875 | 0.935043 |
| a22-matrix-propagation | C AOT | 9.772000, 12.761000, 9.559000 | 10.529000, 12.198000, 19.702000 | 1.101475 |
| a22-matrix-propagation | Dev JIT | 135.747208, 123.390167, 121.670875 | 141.815583, 139.166166, 140.734375 | 1.143792 |
| async-deep-chains | C AOT | 18.551000, 17.777000, 15.612000 | 16.930000, 19.300000, 14.493000 | 0.928324 |
| async-held-handles | C AOT | 9.433000, 8.537000, 8.567000 | 6.246000, 8.268000, 7.619000 | 0.731639 |
| async-settled-awaits | C AOT | 20.638000, 20.525000, 20.397000 | 25.530000, 24.754000, 20.534000 | 1.006717 |

All interpreter samples report 269 golden matches. Every workload run exits zero.
The emitted C comparison for each workload differs only in the diagnostic source filename.
After filename normalization, all fourteen C programs match the pin byte for byte.
This comparison finds no executable C change that explains the ratios above 1.05.
The JIT slow subjects also have unchanged C graphs; no JIT cause was established.
The original interpreter ratio is 1.050585. The amended acceptance no longer uses a fixed 1.05 bound.

### Stage attribution and retained dispatch fix

Five release pin/tree pairs run in this order: pin, tree, pin, tree, pin, tree, pin, tree, pin, tree.
Each binary runs alone and discards three corpus warm-ups before its timed sample.
Every pass checks the same 269 golden outputs.
Temporary timers separate checking, HIR-to-LIR lowering, LIR verification, and interpretation.
The total includes source reads, header selection, golden comparison, and module destruction.
The lowering time excludes its mandatory LIR verification.

Every timed stage sample follows, in milliseconds.

| Pair | Subject | Check | HIR-to-LIR | LIR verification | Interpretation | Total |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| 1 | pin | 95.220831 | 51.105458 | 24.792084 | 1240.042089 | 1509.885458 |
| 1 | tree | 144.494885 | 72.080357 | 33.989266 | 1836.221756 | 2235.413750 |
| 2 | pin | 139.363376 | 76.332216 | 34.273947 | 1742.989670 | 2141.987250 |
| 2 | tree | 142.058497 | 77.568292 | 34.904003 | 1787.201874 | 2197.476792 |
| 3 | pin | 137.801795 | 69.941046 | 34.085201 | 1834.935126 | 2220.435750 |
| 3 | tree | 139.656666 | 71.438418 | 33.442626 | 1808.656543 | 2204.923417 |
| 4 | pin | 147.092292 | 78.611352 | 37.351660 | 1856.331005 | 2274.418084 |
| 4 | tree | 256.851171 | 133.771369 | 51.450582 | 1978.424629 | 2666.747625 |
| 5 | pin | 127.212660 | 64.222709 | 32.130298 | 1738.338611 | 2113.032333 |
| 5 | tree | 128.949090 | 68.500526 | 31.443299 | 2466.585587 | 2838.627917 |

The ratio is the median of the five tree/pin pair ratios. No sample is omitted.

| Stage | Median pair ratio |
| --- | ---: |
| Check | 1.019339 |
| HIR-to-LIR | 1.066609 |
| LIR verification | 1.018383 |
| Interpretation | 1.065771 |
| Total | 1.172497 |

The interleaved series retains an increase: total 1.172497, interpretation 1.065771, and lowering 1.066609.
Checking and LIR verification stay below 1.05 at 1.019339 and 1.018383.
The first and fourth pairs also show broad increases in all four stages.
The raw samples show substantial time variation; the stage ratios alone do not establish a code cause.

The particle program, `a24-particle-system`, supplies the largest execution difference.
Its median interpretation times are 1535.656 ms at the pin and 1629.356 ms in the tree, ratio 1.061016.
It contains no generator, yield, finalizer, or exception handler.
Its complete LIR matches the pin after removal of the empty `generator_close` fields.
The HIR statement, LIR instruction kind, and LIR instruction sizes stay at 432, 112, and 240 bytes.
The LIR function grows from 328 to 352 bytes through its close-table vector.
No close-table, saved-exception, or generator-for-of instruction executes in that dominant workload.

The new finalizer handlers enlarge the common interpreter dispatch body.
One prototype moves `GeneratorClose`, `ExceptionMessage`, `ExceptionPosition`, and `ExceptionRestore` into a cold function.
The function carries `#[cold]` and `#[inline(never)]`.
The operand checks, exception fields, coroutine close behavior, and counted-yield release stay the same.
This change adds no LIR form and removes no verifier pass.

Three controls rotate the order: pin/tree/prototype, prototype/pin/tree, and tree/prototype/pin.
Each binary runs alone and keeps all three warm-ups plus its timed sample.
Every pass checks 269 golden outputs.

| Control | Subject | Check | HIR-to-LIR | LIR verification | Interpretation | Total |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| 1 | pin | 150.964759 | 89.454551 | 39.048667 | 2439.168720 | 2894.195708 |
| 1 | tree | 135.034416 | 72.189467 | 33.243238 | 2448.995241 | 2886.237750 |
| 1 | prototype | 157.662996 | 88.247760 | 37.278626 | 2323.132042 | 2809.203500 |
| 2 | prototype | 134.764495 | 72.472379 | 34.364918 | 1673.368793 | 2066.189625 |
| 2 | pin | 142.111540 | 75.741092 | 34.577788 | 2435.069133 | 2854.670333 |
| 2 | tree | 141.916175 | 72.057081 | 35.411510 | 2577.803548 | 2989.639083 |
| 3 | tree | 164.530544 | 83.193749 | 43.732125 | 1905.091576 | 2368.907625 |
| 3 | prototype | 126.591628 | 64.461163 | 31.802834 | 1744.701783 | 2114.991584 |
| 3 | pin | 134.009746 | 68.061877 | 32.593124 | 1722.637298 | 2115.773584 |

| Stage | Retained fix/pin median pair ratio | Retained fix/tree median pair ratio |
| --- | ---: | ---: |
| Check | 0.948301 | 0.949606 |
| HIR-to-LIR | 0.956844 | 1.005763 |
| LIR verification | 0.975753 | 0.970445 |
| Interpretation | 0.952428 | 0.915810 |
| Total | 0.970634 | 0.892813 |

The prototype lowers interpretation time against the unchanged tree in all three controls.
Its interpretation ratio against the tree is 0.915810; its total ratio is 0.892813.
Against the pin, its interpretation ratio is 0.952428 and its total ratio is 0.970634.
The retained fix removes the observed increase in this control series.
The dispatch-body change supports code layout as the execution cause, rather than executed close or exception operations.
This attribution is an inference from the unchanged LIR and the isolated handler change; it does not identify a hardware mechanism.
The fix changes no lowering code. The control's lowering ratio of 0.956844 does not support a stable lowering regression.
The timers stay outside the production tree.
The original measured ratio stays 1.050585.

### Native bound-call

The backend discards at least three warm-ups and reaches 200 ms of measured warm-up execution.
Each binary runs alone. Each tier keeps its first three timed samples and uses their minimum.
Both subjects report checksum 60000 and clock quantum 41 ns.

| Tier | Pin samples (ns) | Tree samples (ns) | Best ratio |
| --- | --- | --- | ---: |
| C AOT | 7959, 7917, 7916 | 18667, 18584, 18583 | 2.347524 |
| Dev JIT | 9541, 9542, 9583 | 9625, 9708, 9583 | 1.004402 |

Each C AOT subject links its own revision's runtime archive.
The emitted C programs match byte for byte.
The C AOT and dev JIT measurements exit zero.
The retained interpreter fix changes neither native tier.
The C AOT ratio exceeds 1.05; this measurement establishes no native cause.

A separate control links both C programs against the tree runtime archive.
Its three samples are retained below; they do not supply the pin-runtime comparison.

| Subject | Common-runtime C AOT samples (ns) |
| --- | --- |
| pin | 8708, 8583, 8666 |
| tree | 8667, 8834, 8667 |

### Changed files

The complete working-tree inventory includes the retained earlier rounds and the round 7 changes.

| File | Status |
| --- | --- |
| `codegen/src/cemit.rs` | Modified |
| `codegen/src/cemit/emitter.rs` | Modified |
| `codegen/src/cemit/graph.rs` | Modified |
| `codegen/src/cemit/intrinsic.rs` | Modified |
| `codegen/src/cemit/suspend.rs` | Modified |
| `codegen/src/finally_tests.rs` | Added |
| `codegen/src/interpreter.rs` | Modified |
| `codegen/src/interpreter/instruction.rs` | Modified |
| `codegen/src/interpreter/tests.rs` | Modified |
| `codegen/src/jit.rs` | Modified |
| `codegen/src/jit/finally_tests.rs` | Added |
| `codegen/src/jit/symbols.rs` | Modified |
| `codegen/src/lib.rs` | Modified |
| `codegen/src/lir.rs` | Modified |
| `codegen/src/lir/address_taken.rs` | Modified |
| `codegen/src/lir/builder.rs` | Modified |
| `codegen/src/lir/exception.rs` | Modified |
| `codegen/src/lir/expr.rs` | Modified |
| `codegen/src/lir/generator_close.rs` | Added |
| `codegen/src/lir/liveness.rs` | Modified |
| `codegen/src/lir/stmt.rs` | Modified |
| `codegen/src/lir/using.rs` | Modified |
| `codegen/src/lir/verify.rs` | Modified |
| `codegen/src/lir/verify_instruction.rs` | Modified |
| `codegen/src/lir/verify_lifetime.rs` | Modified |
| `codegen/src/lir/verify_narrowing.rs` | Modified |
| `codegen/src/lir/verify_raise.rs` | Modified |
| `codegen/src/lower/func/builtin.rs` | Modified |
| `codegen/src/lower/func/coroutine.rs` | Modified |
| `codegen/src/lower/func/frame_root_tests.rs` | Modified |
| `codegen/src/lower/func/instruction.rs` | Modified |
| `codegen/src/lower/mod.rs` | Modified |
| `codegen/src/root_storage.rs` | Modified |
| `codegen/tests/cemit.rs` | Modified |
| `codegen/tests/emission_chain.rs` | Modified |
| `codegen/tests/lir-goldens/corpus.txt` | Modified |
| `codegen/tests/lir.rs` | Modified |
| `codegen/tests/lir/verifier.rs` | Modified |
| `codegen/tests/support/lir_facts.rs` | Modified |
| `codegen/tests/support/lir_facts/boundary.rs` | Modified |
| `codegen/tests/support/lir_facts/iteration.rs` | Modified |
| `codegen/tests/support/lir_facts/sequence_tests.rs` | Added |
| `codegen/tests/support/lir_facts_lifetime.rs` | Modified |
| `codegen/tests/support/lir_facts_release.rs` | Modified |
| `codegen/tests/support/lir_facts_using.rs` | Modified |
| `codegen/tests/using_fallthrough.rs` | Modified |
| `codegen/tests/void_generator.rs` | Modified |
| `compiler/src/check/assignment_flow.rs` | Modified |
| `compiler/src/check/capture.rs` | Modified |
| `compiler/src/check/exception.rs` | Modified |
| `compiler/src/check/fallthrough.rs` | Modified |
| `compiler/src/check/init_effects.rs` | Modified |
| `compiler/src/check/init_order.rs` | Modified |
| `compiler/src/check/initializer_finish.rs` | Modified |
| `compiler/src/check/layout.rs` | Modified |
| `compiler/src/check/narrowing.rs` | Modified |
| `compiler/src/check/pipeline.rs` | Modified |
| `compiler/src/check/receiver_capture.rs` | Modified |
| `compiler/src/check/rejection.rs` | Modified |
| `compiler/src/check/rejection_facts.rs` | Modified |
| `compiler/src/check/rejection_programs.txt` | Modified |
| `compiler/src/check/rejection_sites.rs` | Modified |
| `compiler/src/check/rejection_targets.txt` | Modified |
| `compiler/src/check/rejection_witness_index.rs` | Modified |
| `compiler/src/check/rejection_witness_sites.rs` | Modified |
| `compiler/src/check/stmt.rs` | Modified |
| `compiler/src/check/using_scope.rs` | Modified |
| `compiler/src/hir.rs` | Modified |
| `compiler/src/hir/effects.rs` | Modified |
| `compiler/src/hir/expression.rs` | Modified |
| `compiler/src/hir/tests.rs` | Modified |
| `compiler/src/hir/using.rs` | Modified |
| `compiler/src/lir.rs` | Modified |
| `compiler/src/raise_sites.rs` | Modified |
| `compiler/src/tests/language.rs` | Modified |
| `compiler/src/trap_sites.rs` | Modified |
| `compiler/src/warn.rs` | Modified |
| `compiler/tests/accessor.rs` | Modified |
| `compiler/tests/corpus_accept.rs` | Modified |
| `compiler/tests/corpus_reject.rs` | Modified |
| `compiler/tests/exceptions.rs` | Modified |
| `compiler/tests/exit_predicate_and_arguments.rs` | Modified |
| `compiler/tests/generator_for_of.rs` | Added |
| `compiler/tests/nullable_using.rs` | Modified |
| `compiler/tests/nullish.rs` | Modified |
| `compiler/tests/synthetic_prefix.rs` | Modified |
| `compiler/tests/top_level_using.rs` | Modified |
| `corpus/accept/a349-finally-exits-and-completions.expected` | Added |
| `corpus/accept/a349-finally-exits-and-completions.ts` | Added |
| `corpus/accept/a350-finally-async.expected` | Added |
| `corpus/accept/a350-finally-async.ts` | Added |
| `corpus/accept/a351-generator-close-runs-finally.expected` | Added |
| `corpus/accept/a351-generator-close-runs-finally.ts` | Added |
| `corpus/accept/a352-finalizer-walker-controls.expected` | Added |
| `corpus/accept/a352-finalizer-walker-controls/lib.ts` | Added |
| `corpus/accept/a352-finalizer-walker-controls/main.ts` | Added |
| `corpus/reject/r233-finally.ts` | Deleted |
| `corpus/trap/t109-generator-close-throws.expected` | Added |
| `corpus/trap/t109-generator-close-throws.ts` | Added |
| `generated-docs/corpus-index.md` | Modified |
| `runtime/src/exception.rs` | Modified |
| `runtime/src/generator_layout.rs` | Modified |
| `specs/blocks/collisions.md` | Modified |
| `specs/tracking/s180-finally-measurement.md` | Modified |

## Round 7: finalizer walkers and completion forms

The contract pin is `03974dd6`. The checkout starts at `dab00b58`.
An external amend moves HEAD to `16da712e` during this run.
That amend changes only CLAUDE.md and specs/rules-history.md. This agent makes no commit.
The Red binary comes from an archive of `03974dd6`.
The JIT, C AOT, and interpreter paths reject a349–a352 with S010 before execution.
The trap entry t109 also rejects with S010 at line 11, column 11.
The a349–a351 and t109 source headers now name `03974dd6`.

The walker policy uses exhaustive fields at each statement-list match arm.
Every Using, GeneratorForOf, If, While, For, ForOf, Switch, and Try pattern names all fields.
Each added field therefore requires a decision at each existing pattern.
The shared children and children_mut walkers include finalizers and update statements.
The raise analysis includes finalizer raises. Address analysis visits the finalizer in its lexical environment.

For holds an update statement list. A continue branches through cleanup to the shared update block.
The update runs once after iteration finalizers. A continue in a finalizer also takes that update block.
The initializer completion pass keeps deferred update prefixes in the update list.
The synthetic-prefix tests now assert this retained For form.

Generator and void return completions carry no value. Other return completions carry their declared return type.
A finalizer clears the held-exception hook guard while its body lowers.
A local catch takes a hook exception. An escaping hook exception replaces the held exception.
The hook's own propagating-exception exit still traps with kind 30.

The a352 entry covers each named defect and matches Node v24.18.0.
Its source decorator makes its value-class method case directly comparable to Node.
Its local-hook output ends with `local hook\nouter held local\n`.
Its uncaught-local-hook output ends with `replaced hook\n`.
The three execution tiers match the same golden bytes.
Stock TypeScript 5.9.2 accepts the complete corpus with exit zero.

The verifier tests build invalid completion payloads, instruction signatures, and suspension graphs.
They read the exception, return, jump, fall-through, generator marker, close, close-flag, payload-read, and restore diagnostics.
An extra suspension has no declared close continuation. A replaced dispatch branch cannot select its declared continuation.
The completion enum is closed to the five specified forms.
The unreachable unsupported-completion check is removed; no valid Rust value could exercise it.
This change keeps the checks for every representable invalid payload.

The source fact walkers visit finalizers. Execution facts count the independently derived cleanup placements.
This distinction prevents a declaration from adding an extra execution site.
The sequence tests move to their own file to keep each Rust source below 2,000 lines.

Recorded limits, unchanged in this round:

- Each exception pass through a finalizer allocates one report string in runtime/src/exception.rs.
- Continue-target identity is inferred from control depth. Labeled loops remain S100, so ambiguous targets are unreachable.
- Narrowing after try/finally remains conservative.


### Exhaustive statement-list patterns

The following walkers and statement observers name every field. Test assertion patterns use the same policy.

| File | Functions |
| --- | --- |
| `compiler/src/warn.rs` | `analyze_w002_sequence, analyze_w003_stmts, collect_synthesized_origins, collect_w004_local_bindings, count_w004_bound_names, scan_candidate_stmts, scan_w004_stmts, walk_statements, warn_w002_direct_uses` |
| `compiler/src/trap_sites.rs` | `stmt` |
| `compiler/src/raise_sites.rs` | `a_using_scope_raises_through_its_body_or_a_hook, statement_can_raise` |
| `compiler/src/hir/using.rs` | `a_switch_binding_tests_its_flag_before_its_null_test, hook` |
| `compiler/src/hir/effects.rs` | `narrowing_effects` |
| `compiler/src/hir/expression.rs` | `children, children_mut` |
| `compiler/src/hir/tests.rs` | `stmt_children_yield_every_child` |
| `compiler/src/tests/language.rs` | `absence_capable_alias_member_omission_uses_reserved_discriminant, count_storage_writes, exhaustive_string_literal_union_switch_is_accepted` |
| `compiler/src/check/layout.rs` | `validate_stmts_frame, walk_lets` |
| `compiler/src/check/init_order.rs` | `statement_pos` |
| `compiler/src/check/stmt.rs` | `check_for, check_for_of, check_if, check_switch, check_while` |
| `compiler/src/check/initializer_finish.rs` | `finish_initializer_statement` |
| `compiler/src/check/using_scope.rs` | `a_declaration_scopes_the_rest_of_its_block_and_places_no_hook, a_switch_binding_is_carried_by_its_storage_and_its_flag, a_switch_without_using_keeps_its_form, nested, structure, switch` |
| `compiler/src/check/narrowing.rs` | `statement` |
| `compiler/src/check/exception.rs` | `check_try` |
| `compiler/src/check/json.rs` | `json_array_body, json_array_construction_body, json_array_validation_body, json_construction_body, json_helper_body, json_return_false_unless, json_validation_body, synthesize_json_parser` |
| `compiler/src/check/init_effects.rs` | `stmt` |
| `compiler/src/check/rejection_facts.rs` | `ts_return_coverage` |
| `compiler/src/check/receiver_capture.rs` | `walk` |
| `compiler/src/check/text.rs` | `check_uri_call` |
| `compiler/src/check/fallthrough.rs` | `every_statement_kind_has_its_admitted_exits, exits, for_initializer_exits_compose_before_the_loop` |
| `compiler/src/check/pipeline.rs` | `visit_stmt` |
| `compiler/src/check/assignment_flow.rs` | `statement` |
| `compiler/src/check/capture.rs` | `stmt` |
| `compiler/src/check/expr/operator.rs` | `check_optional_chain_statement` |
| `codegen/src/lir.rs` | `stmt_pos` |
| `codegen/src/lir/generator_close.rs` | `walk` |
| `codegen/src/lir/stmt.rs` | `lower_statement` |
| `codegen/src/lir/address_taken.rs` | `statement` |
| `codegen/tests/support/lir_facts_lifetime.rs` | `statements` |
| `codegen/tests/support/lir_facts_using.rs` | `arrives_after, leaves, statement` |
| `codegen/tests/support/lir_facts_release.rs` | `statement` |
| `codegen/tests/support/lir_facts.rs` | `collect_return_positions, raises_in_statement, statement_exits, walk_statement_expression_roots_mode` |
| `codegen/tests/support/lir_facts/sequence_tests.rs` | `a_conditionless_for_drops_its_trailing_execution_facts` |
| `codegen/tests/support/lir_facts/iteration.rs` | `collect_for_of_bounds` |

### Round 7 release cost against `3de68c37`

Both release binaries use their own revision and runtime. No build or test runs during any sample.
Each binary runs alone. Each subject keeps three timed samples and reports their minimum.
The interpreter discards three corpus warm-ups. Native workloads discard at least three calls and 200 ms of measured warm-up.
The interpreter selection checks the same 269 common golden outputs in every pass.
It excludes declared interpreter exclusions, the matrix benchmark, and a349–a352.
The async C span includes Context setup, execution, checkpoints, and Context release.
Async JIT is absent because its benchmark span does not include host checkpoints.

| Workload | Tier | Pin samples (ms) | Tree samples (ms) | Best ratio |
| --- | --- | --- | --- | ---: |
| corpus | interp | 1229.406167, 1236.357958, 1243.970958 | 1178.216250, 1183.244500, 1176.560000 | 0.957015 |
| callbacks | c | 37.438000, 37.003000, 37.103000 | 41.765000, 41.649000, 41.683000 | 1.125557 |
| callbacks | jit | 259.974708, 258.256375, 258.664250 | 259.582833, 260.614333, 259.517250 | 1.004882 |
| collect | c | 35.711000, 37.450000, 36.673000 | 35.663000, 35.566000, 35.618000 | 0.995940 |
| collect | jit | 121.890875, 118.448292, 116.859958 | 117.386875, 116.752709, 117.339625 | 0.999082 |
| fib-loop | c | 30.976000, 31.048000, 30.948000 | 30.895000, 30.937000, 31.882000 | 0.998287 |
| fib-loop | jit | 73.229583, 73.185542, 72.939500 | 75.272541, 73.093333, 74.385708 | 1.002109 |
| fib-recursive | c | 3.724000, 3.748000, 3.738000 | 3.742000, 3.723000, 3.741000 | 0.999731 |
| fib-recursive | jit | 8.176750, 8.149417, 8.178333 | 8.124250, 8.186875, 8.113042 | 0.995536 |
| mandelbrot | c | 128.424000, 128.241000, 128.064000 | 130.006000, 130.885000, 128.421000 | 1.002788 |
| mandelbrot | jit | 134.345542, 133.594875, 133.737500 | 133.980500, 134.972959, 134.315666 | 1.002887 |
| particles | c | 74.772000, 74.720000, 74.936000 | 74.377000, 74.683000, 74.698000 | 0.995410 |
| particles | jit | 443.276250, 451.793791, 446.427583 | 452.891833, 450.624625, 451.370250 | 1.016577 |
| primes | c | 21.726000, 21.783000, 21.804000 | 21.717000, 21.892000, 21.924000 | 0.999586 |
| primes | jit | 32.776625, 32.910958, 32.874334 | 32.901292, 32.668000, 33.002250 | 0.996686 |
| queen | c | 25.803000, 25.756000, 25.851000 | 25.794000, 25.780000, 25.781000 | 1.000932 |
| queen | jit | 37.151333, 37.064625, 37.593791 | 35.692583, 35.680750, 35.701875 | 0.962663 |
| sort | c | 18.099000, 18.071000, 18.061000 | 18.246000, 18.091000, 18.307000 | 1.001661 |
| sort | jit | 34.708875, 34.550250, 34.454584 | 35.441208, 35.932292, 36.122292 | 1.028635 |
| tree | c | 102.217000, 116.212000, 108.356000 | 102.033000, 102.031000, 102.855000 | 0.998180 |
| tree | jit | 410.691750, 407.001166, 402.136875 | 405.726875, 408.563834, 404.715167 | 1.006411 |
| a22-matrix-propagation | c | 5.273000, 5.310000, 5.281000 | 5.274000, 5.273000, 5.297000 | 1.000000 |
| a22-matrix-propagation | jit | 78.918250, 79.075834, 78.940833 | 79.114250, 78.934083, 78.728542 | 0.997596 |
| async-deep-chains | c | 9.345000, 9.365000, 9.325000 | 9.417000, 9.355000, 9.383000 | 1.003217 |
| async-held-handles | c | 3.925000, 3.850000, 3.877000 | 3.998000, 3.873000, 3.887000 | 1.005974 |
| async-settled-awaits | c | 13.352000, 13.238000, 13.311000 | 13.124000, 13.088000, 13.080000 | 0.988065 |
| bound-call | c | 0.005667, 0.005625, 0.005583 | 0.005625, 0.005584, 0.005625 | 1.000179 |
| bound-call | jit | 0.006292, 0.006166, 0.006292 | 0.006167, 0.006208, 0.006209 | 1.000162 |

All measured binaries exit zero. The C outputs match between the two revisions for every standard and async workload.
The emitted C programs match byte for byte for all standard and async workloads.
The bound-call subjects retain checksum 60000 and three samples per tier.
The common interpreter corpus best ratio is 0.957015.
The largest increase is callbacks/C AOT at 12.56%, within the recorded 15% benchmark spread.
The two emitted C programs are identical, so this comparison shows no executable code cause.
No measured increase has an established code cause. The timing ratios alone do not attribute a growth.

### Round 7 verification and test cost

All commands use the sandbox. No commit is made.
The final workspace all-target build completes with zero warning lines.
Formatting and hygiene checks pass. Clippy completes with existing warnings; it is not a warning-free run.
The final TypeScript check exits zero.
The golden suite passes 35 tests. The LIR suite passes 57 tests, including the interpreter corpus and all HIR facts.
The final crate suites report:

| Crate | Passed | Ignored | Targets, including documentation |
| --- | ---: | ---: | ---: |
| compiler | 1101 | 1 | 66 |
| runtime | 450 | 1 | 18 |
| codegen | 879 | 1 | 66 |

The following costs are debug test-harness times. They exclude Rust compilation.
The crate suites overlap, so these figures do not measure isolated release performance.

| Changed test file or group | Target | Tests | Harness seconds |
| --- | --- | ---: | ---: |
| compiler/tests/generator_for_of.rs | generator_for_of | 1 | 0.00 |
| codegen/tests/cemit.rs | cemit | 105 | 24.44 |
| codegen/tests/emission_chain.rs | emission_chain | 0; one ignored | 0.00 |
| codegen/tests/lir.rs; lir/verifier.rs; lir-goldens/corpus.txt; support/lir_facts.rs and its modules; support/lir_facts_lifetime.rs; support/lir_facts_release.rs; support/lir_facts_using.rs | lir | 57 | 14.35 |
| codegen/tests/using_fallthrough.rs | using_fallthrough | 3 | 0.45 |
| codegen/tests/void_generator.rs | void_generator | 2 | 0.04 |

The generator-for-of figure is below the harness's 0.01-second reporting precision.
The fact witnesses share one corpus lowering pass. The split sequence-test file adds no test or corpus pass.
The payload-signature control also reads the frame-owned-slot diagnostic and passes after the full suite.
Each measurement binary completes. Every build and test process completes. No session-started background task remains.

The §180 verifier tests read diagnostics for incomplete exception captures, displaced complete captures, and a catch entry without its capture prefix.
Try-finally return coverage uses the body or finalizer; exhaustive enum-switch probes retain the plain switch's rejection site and pass TypeScript 5.9.2.
