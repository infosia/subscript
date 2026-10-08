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
