# §176: await and dropped-generator counts

The measurement pin is `e8cf6e09`.
The results use the debug JIT, optimized emitted C, and the interpreter.
J/C/I identifies these tiers, in that order.

Each accepted entry resides in `$TMPDIR/s176-measure` before the CLI reads it.
The JIT CLI uses `subscript run`.
The count harness uses `ReloadSession`, emitted C with a host entry, and `Interpreter`.
The harness reads each count before Context destruction.
It drains the ready queue unless a trap stops execution.

The JIT reads `ReloadSession::async_tasks().len()`.
C counts callbacks from a non-null `subscript_rt_ctx_visit_async_tasks` visitor.
The interpreter reads `Interpreter::async_tasks().len()`, which excludes completed entries with zero owners.
A trap row gives the count at the stop.
The C measurement host returns zero after it records the trap state; that exit does not mean script success.
`—` means rejection before execution.
Task counts do not count generator frames or other Context allocations.

Node version 24.18.0 executes the TypeScript entries with its built-in type removal.
The Node adapter binds `print` to `console.log` and `Context.suspend()` to `Promise.resolve()`.
It gives `Map.getOr(k, fallback)` the result of `has(k) ? get(k) : fallback`.
It makes `Context.collect()` and `Context.free()` no-ops.
Thus Node compares task values and body effects, but it does not compare explicit memory release.
No external result replaces a corpus golden.

## Item A: a borrowed element across await

`A` means the exact stdout `reset\ngot 7\nend\n`, with no trap.
Node gives `A` and exit zero for every row below.

| Entry and program shape | Dev JIT | C AOT | Interpreter | Tasks J/C/I | Node |
|---|---|---|---|---|---|
| `A-base`: `await G[0]`; another task resets `G` | A | A | A | 0/0/0 | A |
| `A-push`: reset `G`, then push a new task; clear `G` after both readers finish | A | A | A | 0/0/0 | A |
| `A-collect`: reset `G`, then call `Context.collect()` | A | A | A | 0/0/0 | A; collect adapter |
| `A-static`: `await Box.jobs[0]`; reset the static field | A | A | A | 0/0/0 | A |
| `A-field`: `await box.jobs[0]`; another task calls `Context.free(box)` | A | A | A | 0/0/0 | A; free adapter |
| `A-nested`: `await G[0][0]`; reset the outer array | A | A | A | 0/0/0 | A |
| `A-loop`: `for (let i: i32 = 0; i < G.length; i++)` with `await G[i]` | A | A | A | 0/0/0 | A |
| `A-map`: hold `m.getOr(1, fallback)` in `held`; await `held`; clear the Map during suspension | A | A | A | 0/0/0 | A; getOr adapter |
| `A-map-array`: `await m.getOr(1, [fallback])[0]`; clear the Map during suspension | A | A | A | 0/0/0 | A; getOr adapter |
| `A-map-values`: iterate `m.values()` and await the binding; clear the Map during suspension | A | A | A | 0/0/0 | A |
| `A-map-get`: direct `await m.get(1)` | S018 | S018 | S018 | — | A |
| `A-map-direct`: direct `await m.getOr(1, fallback)` | S018 | S018 | S018 | — | A; getOr adapter |

The direct Map calls give `has no async method`.
A local holder, a Map array element, and a values iteration provide accepted forms.
The local and iteration binding acquire their own counts in addition to the await registration.
The Map array element remains a borrowed input to await.

### Source and variants

The base entry contains this complete program:

```ts
let G: Promise<i32>[] = [];
async function slow(): Promise<i32> {
  await Context.suspend();
  await Context.suspend();
  return 7;
}
async function store(): Promise<void> {
  const h = slow();
  G = [h];
  if (G.length > 5) { await h; }
}
async function reader(): Promise<void> { print(`got ${await G[0]}`); }
async function resetter(): Promise<void> {
  await Context.suspend();
  G = [];
  print("reset");
}
export async function main(): Promise<void> {
  await store();
  const r = reader();
  const s = resetter();
  await s;
  await r;
  print("end");
}
```

The push variant replaces the reset with this body tail:

```ts
G = [];
const h = slow();
G.push(h);
if (G.length > 5) { await h; }
print("reset");
```

Its `main` clears `G` before `end`.
The collect variant calls `Context.collect()` immediately after the reset.
The static variant replaces `G` with `Box.jobs` and declares `class Box { static jobs: Promise<i32>[] = []; }`.
The field variant declares `class Box { jobs: Promise<i32>[] = []; }` and a module-global `const box = new Box()`.
It stores `[h]` in `box.jobs` and replaces the reset with `Context.free(box)`.
The nested variant uses `Promise<i32>[][]`, stores `[[h]]`, and reads `G[0][0]`.
The loop variant uses the loop header in the table instead of one direct read.

The Map variants replace `G` with a module-global Map and use `m.set(1, h)` or `m.set(1, [h])`.
Their resetter uses `m.clear()`.
The store helper uses `if (m.size > 5) { await h; }`.
The getOr variants declare a module-global `spare: Promise<i32>[] = []`.
Their reader creates `fallback = slow()`, stores `[fallback]` in `spare`, and includes `if (spare.length > 5) { await fallback; }`.
Their `main` clears `spare` before `end`.
The values variant needs no fallback.

### Await ownership evidence

`codegen/src/lir/lambda.rs::lower_async_handle_await` emits `Terminator::Suspend` with `SuspendKind::AsyncHandle { handle, owned }`.
An array element read is borrowed, so `owned` is false.
The LIR does not need a separate `AsyncHandleRetain` instruction for this registration.

`codegen/src/lower/func/coroutine.rs` selects `async_await` when `owned` is false.
`codegen/src/cemit/suspend.rs` selects `subscript_rt_async_await` for the same LIR fact.
`runtime/src/context/async_scheduler.rs::Context::async_await` calls `async_retain(handle)` before it registers the continuation.
That operation acquires one count on the awaited task.
The resume reads the completion and releases that registration count.

`codegen/src/interpreter.rs::register_continuation` increments `owners` when `owned` is false.
The interpreter also releases that count after the completion read.

The counted completion acquire is a different operation.
`AwaitRaise.count_action` describes the fulfilled result, as §171 rule 8 requires.
These entries return `i32`, so that result has no count.
The task registration still has its count.

**Result:** None of these good-faith await forms reaches wrong output, a trap, a use-after-delete, or a retained task.
The measurement does not reproduce §171.3 item 4 at this pin.
Both an unfinished-frame root and an independent await count protect the awaited frame.

## Item B: a dropped generator

`B` means stdout `v 1\nend\n`.
`E` means stdout `v 1\nresumed\nv 2\ndone\nend\n`.
`F` means stdout `v 1\nresumed\nv 2\ndone\n`, then trap 29 with `Error: lost`.
`N` means stdout `unused\nend\n`.
`L` means stdout `v 1\nresumed\nv 2\ndone\nexhaust true\nend\n`.
A cell without an explicit trap reports no trap.
`UR` means that Node reports an unhandled rejection with `Error: lost` and exits 1.

| Entry and program shape | Dev JIT | C AOT | Interpreter | Tasks J/C/I | Node |
|---|---|---|---|---|---|
| `B-original`: generator parameter is a handle; copy it to a local without a consume call | S013 | S013 | S013 | — | B; UR |
| `B-break`: generator holds a handle array; break after the first yield | B | B | B | 1/1/1 | B; exit 0 |
| `B-return`: return from the consumer after the first yield | B | B | B | 1/1/1 | B; exit 0 |
| `B-local`: hold the generator in a local; call `next()` once | B | B | B | 1/1/1 | B; exit 0 |
| `B-local-never`: create the generator; never call `next()` | N | N | N | 1/1/1 | N; exit 0 |
| `B-collect`: let the local iterator end after one yield; explicitly collect after the consumer | B | B | B | 1/1/1 | B; collect adapter |
| `B-scoped`: copy the parameter inside a block; drop the generator at a yield inside that block | B | B | B | 1/1/1 | B; exit 0 |
| `B-handle-local`: consume the handle parameter through a helper; drop the generator after one yield | B | B | B | 1/1/1 | B; exit 0 |
| `B-break-fail`: break with a failed unobserved task in the array | B | B | B | 1/1/1 | B; UR |
| `B-return-fail`: return with the same failed task | B | B | B | 1/1/1 | B; UR |
| `B-local-fail`: drop the local iterator with the same failed task | B | B | B | 1/1/1 | B; UR |
| `B-handle-local-fail`: drop the iterator that holds a failed handle parameter | B | B | B | 1/1/1 | B; UR |
| `B-exhaust`: exhaust the generator | E | E | E | 0/0/0 | E; exit 0 |
| `B-exhaust-fail`: exhaust the generator with a failed unobserved task | F | F | F | 2/2/2 at trap | E; UR |
| `B-alias`: drop a local copy, then resume and exhaust the original iterator | L | L | L | 0/0/0 | L; exit 0 |
| `B-symbol`: obtain the iterator through `[Symbol.iterator]()` | S016, S100 | S016, S100 | S016, S100 | — | B; exit 0 |
| `B-symbol-fail`: use that form with a failed task | S016, S100 | S016, S100 | S016, S100 | — | B; UR |
| `B-finally-break`: break from a generator with a `finally` block | S010 | S010 | S010 | — | `v 1\nclose\nend\n`; exit 0 |
| `B-finally-return`: return from that consumer | S010 | S010 | S010 | — | `v 1\nclose\nend\n`; exit 0 |

S013 names the original generator's handle parameter.
The accepted form matches the generator row in the §171 count tests.
It uses an array parameter and passes the element to a helper that stores it.
The helper also permits a direct handle parameter after a consume call.
S016 names the unknown `Symbol`; S100 states that the generator is not indexable.
S010 rejects `finally`.

The successful entries await `work()` before the generator receives its handle.
The failed entries use `if (false) { await h; }` instead.
That branch satisfies the current must-await approximation without observing the failure.
The failed task finishes before the iterator drops.
Its held count suppresses the required last-release trap.

### Source and variants

The S013-clean local entry contains this complete program:

```ts
let transfer: Promise<i32>[] = [];
function consume(h: Promise<i32>): void { transfer.push(h); }
async function work(): Promise<i32> { return 7; }
async function fail(): Promise<i32> { throw new Error("lost"); }
function* gen(a: Promise<i32>[]): Generator<i32> {
  const local = a;
  consume(local[0]);
  transfer.pop();
  yield 1;
  print("resumed");
  yield 2;
  print("done");
}
async function use(): Promise<void> {
  const h = work();
  await h;
  const it = gen([h]);
  print(`v ${it.next().value}`);
}
export async function main(): Promise<void> {
  await use();
  print("end");
}
```

The break variant replaces the local iterator statements with `for (const v of gen([h])) { print(\`v ${v}\`); break; }`.
The return variant uses `return` instead of `break`.
The exhausted variant omits both abrupt exits.
The never-started variant keeps `const it = gen([h])` and replaces `next()` with `print("unused")`.
The collect variant calls `Context.collect()` after `await use()`.
The failed variants replace `work()` and its await with `fail()` and the false branch above.

The handle variants change the parameter to `a: Promise<i32>`, call `consume(local)`, and pass `h` instead of `[h]`.
The scoped variant puts `const local`, `consume`, `pop`, and the first yield inside one block.
Its second yield and final print stay outside that block.
The alias control uses this consumer tail:

```ts
const it = gen([h]);
{
  const copy = it;
  print(`v ${copy.next().value}`);
}
print(`v ${it.next().value}`);
print(`exhaust ${it.next().done}`);
```

The Symbol variants insert `[Symbol.iterator]()` after `gen([h])`.
The finally variants put both yields in `try` and use `finally { print("close"); }`.

### Iterator close and body effects

Node's break test gives `v 1`, `return()`, `finally`, and `end`, in that order.
The test wraps an actual generator's `return` method and records each call.
It uses this JavaScript:

```js
function* gen() {
  try { yield 1; console.log("resumed"); yield 2; }
  finally { console.log("finally"); }
}
function observed() {
  const it = gen();
  const close = it.return.bind(it);
  it.return = value => { console.log("return()"); return close(value); };
  return it;
}
for (const value of observed()) { console.log(`v ${value}`); break; }
console.log("end");
```

The accepted language break and return entries print no `resumed` or `done` line at the abrupt exit.
The exhausted control prints both lines.
Thus those exits execute no further generator-body code in the measured accepted form.
The rejected finally examples do not establish an accepted cleanup effect in this language.
Node with `--unhandled-rejections=warn` also prints `UnhandledPromiseRejectionWarning` for the failed break entry.
Its default mode exits 1 after the same stdout.

**Result:** Good-faith early exits retain one task, even if the program explicitly collects afterward.
The completed-task entries show a count that does not reach its deterministic release point.
The failed-task entries show an observable defect: the last-release trap 29 never occurs.
The measurement finds no wrong numeric value or use-after-delete in these entries.

### Last-holder prototype

The prototype contradicts §171 rule 13 and §171.3 item 1, which preserve suspended-generator locals after iterator drop.
The prototype covers local generator holders and for-of subject holders in the interpreter.
It does not implement native generator destruction or recursive ownership through arrays, fields, Maps, or yielded generators.

The LIR builder treats `Type::Generator` as an owner type.
A generator call creates a fresh owner.
A local copy emits `AsyncHandleRetain`; a lexical exit emits `AsyncHandleRelease`.
The for-of subject uses the existing hidden owner binding, so break and return emit its release.
The instruction verifier accepts a generator operand for these two instructions.
These instruction names serve only the prototype; they do not define a production generator ABI.

The interpreter starts a generator with one owner.
`Interpreter::release_coroutine`, in `codegen/src/interpreter/counted.rs`, handles its last release.
It removes the generator registry entry and releases saved counted operands without executing the body.
It then clears the frame values and locals.
A completed generator needs no second local release.

The cleanup candidate reads releases from the first normal-return block of the generator's LIR.
It resolves each operand through `Function::liveness.value_origins` to a saved value.
It preserves duplicate release instructions because two holders can own the same array.
The candidate uses the actual saved value and its static type with `counted_owner`.

For break, return, one-next local, never-started local, explicit collect, and the direct-handle local, the prototype gives zero retained tasks.
Their stdout stays unchanged.
The failed break, return, array-local, and handle-local entries print only `v 1`, then trap 29; two enclosing tasks remain at the stop.
The exhausted controls preserve their baseline outcomes.
The alias control preserves `L` and zero tasks, so the first copy's exit does not close the live original.

The scoped entry still gives `B` and one retained task.
Its normal-return block releases the parameter, but the inner block's earlier exit contains the local's release.
The candidate misses that release at the suspended yield.
A normal-return cleanup list does not describe every holder that exists at a suspension.
The LIR needs a cleanup description for the lexical owners live at each suspension and before the first resume.

The interpreter corpus reports 268 runs, 268 golden matches, and 64 declared exclusions in 15.788 seconds.
The debug corpus omits the `a22` benchmark, although the full-sweep environment variable selects all other eligible entries.
This output corpus does not detect the retained count in the scoped entry.
The prototype measurement test reports its counts separately.

The native runtime needs a separate generator owner count and a frame cleanup description at creation and suspension.
Native generator offset 4 stores the reload epoch (`codegen/src/lower/func.rs`), unlike an async frame's count at offset 4.
`Context::async_release` accepts only frames in the async registry.
Reusing the native async release cannot destroy a generator or release its saved locals.
No native-tier prototype result supports this interpreter experiment.

### Changed files and final state

The prototype and measurement harness temporarily change these files:

- `codegen/src/lir.rs`
- `codegen/src/lir/builder.rs`
- `codegen/src/lir/verify_instruction.rs`
- `codegen/src/interpreter.rs`
- `codegen/src/interpreter/counted.rs`
- `codegen/src/interpreter/counted_measurement_tests.rs`

All six files equal their HEAD contents after the revert.
The sole remaining repository file is `specs/tracking/s176-count-measurement.md`.
`git status --short` reports only that untracked note.
`git diff --stat` reports no tracked change.
`git diff --check` reports no whitespace error.
No commit or `tools/gate.sh` run forms part of this measurement.

## Implementation: the LIR cleanup input

The implementation pin is `a8e40135`.
The cleanup verifier of §176 rule 4 needs the identity and lifetime of each counted holder.
The LIR carries value liveness, but it does not carry these holder facts.
Two holders can use one value id and own two counts.

This checked program exposes the difference:

```ts
let transfer: Promise<i32>[] = [];
function consume(h: Promise<i32>): void { transfer.push(h); }
function* gen(a: Promise<i32>[]): Generator<i32> {
    {
        const local = a;
        consume(local[0]);
        transfer.pop();
        yield 1;
    }
    yield 2;
}
export function main(): void {}
```

`check_program` accepts the program.
`lower_module` produces verified LIR.
A temporary integration test prints the LIR and its `Function::liveness`.
The test passes at the pin: one test, 0.01 seconds after compilation.

| State | Counted holders in the frame | Required releases | Counted value origins in the successor live-in |
|---|---|---|---|
| Start | parameter `a` | 1 | `%0` |
| First yield | parameter `a`, local `local` | 2 | `%0` |
| Second yield | parameter `a` | 1 | `%0` |

The local declaration emits `AsyncHandleArrayRetain(%0)`.
It defines no new LIR value or local storage.
The first suspension passes `%0` to successor parameter `%7`.
The inner block exit releases `%7`.
The second suspension passes `%7` to successor parameter `%8`.
The function exit releases `%8`.
Both successor parameters have origin `%0`.

The exact liveness result is:

```text
live_ins: [[ValueId(0)], [ValueId(0)], [ValueId(0)]]
value_origins: [ValueId(0), ValueId(1), ValueId(2), ValueId(3),
               ValueId(4), ValueId(5), ValueId(6), ValueId(0), ValueId(0)]
```

`compiler/src/lir.rs::Liveness` stores sets of value ids and their origins.
It stores no holder identity, exit boundary, or ownership multiplicity.
`codegen/src/lir/builder.rs::declare_binding` stores lexical aliases only in the builder's binding table.
`FunctionBuilder::finish` does not transfer that table to the LIR function.
`release_scopes_from` reads that table to emit the ordinary exit releases.
The absence of these facts also appears in §171 rule 9 and §171.3 item 2.

A cleanup description that releases each live counted value once misses the local's count at the first yield.
A description that always releases it twice releases an extra count at the second yield.
A cleanup verifier cannot distinguish these states from the value live-ins alone.
The emitted count instructions expose the actions, but they do not supply independent holder liveness for the required check.
The consumer needs counted-holder lifetime facts in the LIR, separately from the cleanup description.
Core principle 8 and the handoff's stop condition apply to this missing input.


## Round 2 implementation evidence

The contract pin is `6c9be0f2`.
The pin binaries use the unchanged compiler, LIR lowering, runtime, and interpreter.
The count harness reads registered tasks before Context teardown.
It checks the interpreter registry, `ReloadSession::async_tasks`, and the C task visitor.

### Red corpus

TypeScript 5.9.2 accepts all three new entries with exit 0.
The temporary tsconfig extends the repository tsconfig and includes every prelude declaration.
The CLI reads temporary copies of the entries.
Node v24.18.0 gives the `a346` golden with exit 0 through the explicit adapter.
The corpus header states `no Q6`, because the standard Node runner has no Context.free shim.
The Node adapter maps `print` to `console.log` and `Context.suspend()` to `Promise.resolve()`.
It makes `Context.free()` and `Context.collect()` no-ops.

| Entry | Dev JIT | C AOT | Interpreter | Retained tasks J/C/I |
|---|---|---|---|---|
| `a346-dropped-generator-frame` | golden output; no trap | same | same | 6/6/6 |
| `t103-dropped-generator-break` | `v 1`, `after drop`, `end`; no trap | same | same | 1/1/1 |
| `t104-dropped-generator-local` | `v 1`, `after drop`, `end`; no trap | same | same | 1/1/1 |

The `a346` output agrees with Node, but its six dropped frames keep six tasks.
Each trap entry misses trap 29 and prints two lines beyond its golden.
Each shape below runs alone, so its count does not include a previous shape.

| `a346` shape | Retained tasks J/C/I |
|---|---|
| for-of break | 1/1/1 |
| consumer return | 1/1/1 |
| local iterator after one next | 1/1/1 |
| never-started iterator | 1/1/1 |
| field release through Context.free | 1/1/1 |
| array pop with a discarded result | 1/1/1 |
| local alias, then original exhaustion | 0/0/0 |

The pin count harness takes 5.96 seconds for ten inputs across the three tiers.
Each input uses one interpreter run, one JIT session, and one C build.
Three additional JIT and C runs check corpus output parity.
This time excludes Rust compilation.

### Pin cost

No benchmark source uses a generator.
The search covers `benchmarks/workloads/subscript` and `benchmarks/src/bin`.
The interpreter corpus runs alone with `SUBSCRIPT_FULL_INTERPRETER_SWEEP=1`.
The pin cost excludes the new `a346` entry and Rust compilation.

| Run | Interpreter corpus seconds | Golden matches |
|---|---:|---:|
| 1 | 8.998 | 268/268 |
| 2 | 9.177 | 268/268 |
| 3 | 9.163 | 268/268 |

The best of three is 8.998 seconds.
The debug profile omits `a22-matrix-propagation`, whose header states `cost: benchmark`.
The corpus also declares 64 interpreter exclusions.


### Ordinary joins need count routes

The count verifier needs an ownership route on an ordinary block edge.
The current LIR carries a payload route, but it carries no count route.
The new cleanup description of rule 4 does not supply that independent input.

This accepted generator exposes the missing route:

```ts
let transfer: Promise<i32>[] = [];
function consume(h: Promise<i32>): void { transfer.push(h); }
function* gen(a: Promise<i32>[], b: Promise<i32>[], flag: boolean): Generator<i32> {
    let selected = a;
    if (flag) { selected = b; }
    yield selected.length;
    consume(a[0]); transfer.pop();
    consume(b[0]); transfer.pop();
}
export function main(): void {}
```

`check_program` accepts this source at the pin.
`lower_module` produces verified LIR.
The diagnostic unit test takes 0.01 seconds after compilation.
The generator has this graph:

```text
b0:
  AsyncHandleArrayRetain(%0)
  branch flag, b1, b2
b1:
  AsyncHandleArrayRetain(%1)
  AsyncHandleArrayRelease(%0)
  branch b3(%0, %1, %2, %1)
b2:
  branch b3(%0, %1, %2, %0)
b3(%3, %4, %5, %6):
  %7 = Length(%6)
  yield %7 -> b4(%3, %4, %6)
b4(%16, %17, %18):
  ...
  AsyncHandleArrayRelease(%18)
  AsyncHandleArrayRelease(%17)
  AsyncHandleArrayRelease(%16)
  return
```

The instruction actions give these physical-origin balances before the ordinary edge:

| Predecessor | Origin `%0` | Origin `%1` |
|---|---:|---:|
| `b1` | 1 | 2 |
| `b2` | 2 | 1 |

The frame owns three holders on both paths: `a`, `b`, and `selected`.
Their payloads differ, but their holder lifetimes agree.
The required join balance is one count each for `%3`, `%4`, and `%6`.
The required suspension cleanup releases these three holder counts.

`BlockTarget::arguments` maps payloads to block parameters.
It does not state which count each argument transfers, or whether an argument borrows a value.
Every counted join parameter above has `fresh_owner: false` and no source name.
Their `value_origins` entries are `%3`, `%4`, and `%6`, respectively.
The suspension versions `%16`, `%17`, and `%18` map back to those three entries.
No origin entry maps the ordinary join to its predecessor's count holders.

`BlockDraft::state_bindings` supplies the missing holder-to-parameter map inside the builder.
`new_state_block`, `block_target`, and `enter_block` use that map.
`FunctionBuilder::finish` discards it.
`BasicBlock::parameters` then carries only value ids.
`Value::fresh_owner` identifies a fresh result; it does not identify each holder that an ordinary edge carries.

An unchanged-origin dataflow reports different balances at this accepted join.
A payload rename cannot decide how to divide two counts between two destination holders.
A verifier that treats each counted block parameter as one owner assumes a builder convention that the LIR does not state.
A verifier that takes the division from the cleanup description checks the description against itself.

The LIR needs explicit ownership routes on ordinary edges, or persistent holder roles and provenance for block parameters.
That fact must distinguish an owned count from a borrowed payload and preserve multiplicity.
The independent Phase Review confirms this missing input in a realistic mutable alias across an ordinary branch.
Core principle 8 and the handoff stop condition apply.
The native layout and the release implementation remain at the pin.
No post-implementation cost exists.

The new corpus count test requires zero tasks after `a346`.
It is Red at the pin: the interpreter reports six retained tasks.
Its successful path costs one interpreter run, one JIT session, and one C build.


### Check results

The workspace all-target build passes with `--offline --locked`.
The compiler tests pass after the JS header correction and the `js_corpus` target rerun.
The first compiler run reports 1,090 passes and one JS header failure.
The corrected JS target reports 11 passes and zero failures.
The runtime tests report 429 passes and zero failures.
The codegen tests report 831 passes and three Red test failures:

- `dropped_generator_corpus_releases_every_task`: six retained tasks instead of zero.
- `trap_corpus_entries_match_dev_stdout_on_both_tiers`: the two new entries do not trap.
- `lir_interpreter_debug_subset_traps_at_declared_sites`: the new break entry does not trap.

The isolated Red count test takes 0.01 seconds, without Rust compilation.
The complete native trap gate takes 18.03 seconds; that time includes all existing trap entries.
The interpreter trap gate shares its target with other LIR tests, so the target time does not measure its individual cost.
The successful count test path still needs a cost measurement after the implementation.

`cargo fmt --check` passes.
Workspace all-target clippy passes with existing warnings and no new warning.
The only warning site in a changed Rust file is the existing `repeat().take()` in `codegen/tests/cemit.rs`.
`git diff --check` passes.
The Phase Review reports zero findings in the current diff.
`tools/hygiene.sh` passes.

### Changed files

- `codegen/src/interpreter/counted_measurement_tests.rs`
- `codegen/tests/cemit.rs`
- `codegen/tests/lir.rs`
- `codegen/tests/lir-goldens/corpus.txt`
- `corpus/accept/a346-dropped-generator-frame.ts`
- `corpus/accept/a346-dropped-generator-frame.expected`
- `corpus/trap/t103-dropped-generator-break.ts`
- `corpus/trap/t103-dropped-generator-break.expected`
- `corpus/trap/t104-dropped-generator-local.ts`
- `corpus/trap/t104-dropped-generator-local.expected`
- `generated-docs/corpus-index.md`
- `specs/tracking/s176-count-measurement.md`

## Round 3: count semantics for an element replacement

The contract pin is `9cac37e5`.
The compiler accepts this generator and produces verified LIR:

```ts
let transfer: Promise<i32>[] = [];
function consume(h: Promise<i32>): void { transfer.push(h); }
function* gen(a: Promise<i32>[]): Generator<i32> {
    a[0] = a[1];
    yield 1;
    consume(a[0]);
    transfer.pop();
}
export function main(): void {}
```

A temporary integration test checks the source and prints the LIR.
The test passes: one test, 0.01 seconds, without Rust compilation.
The replacement has these instructions:

```text
%1 = AddressOfIndex(%0, 1)
%2 = LoadAddress(%1)
%3 = AddressOfIndex(%0, 0)
%4 = LoadAddress(%3)
AsyncHandleRetain(%2)
StoreAddress(%3, %2)
AsyncHandleRelease(%4)
yield 1 -> b1(%0)
```

Both loads have `fresh_owner: false`.
The store has `count_action: None`.
The frame owns one count of the array `%0`.
The array owns the element counts (§171 rule 2).
The store consumes the retained count of `%2` and replaces the slot's old owner.
The release of `%4` ends that displaced owner; it does not reduce the frame's array count.

The typed address, old load, store, and release describe the replacement independently of a cleanup description.
The verifier can bind the displaced owner to that sequence.
It must exclude an intervening replacement or invalidating effect.
A frame balance that subtracts every release without this distinction reports a false negative balance for `%4`.

The independent review identifies no missing LIR input beyond the cleanup descriptions and ownership flags that the contract requires.
The temporary test leaves no repository test file.
The production code remains at the pin.
No implementation result or post-implementation cost exists.
`tools/hygiene.sh` passes.


## Round 4: the lexical exit needs a source position

The contract pin is `fde3e1a3`.
The LIR does not carry the source position of a lexical block exit.
Its HIR input also lacks that position.

`compiler/src/hir.rs` declares `Stmt::Block(Vec<Stmt>)`.
The block has no span and no closing-brace position.
`codegen/src/lir/stmt.rs::lower_scoped` selects the last statement position for every normal exit action.
For an empty block, it selects the function position.
The builder's binding table supplies the owners, but not the position of their lexical exit.

The direct test `codegen/tests/generator_exit_positions.rs` builds two accepted programs.
Each program creates a local generator and calls `next()` inside an inner block.
The second program moves two blank lines from after the closing brace to before it.
Every statement stays at the same source position.
The enclosing function also keeps its source position.

| Input | Inner closing brace | Last statement | Lowered LIR |
|---|---|---|---|
| First source | line 6, column 5 | line 5, column 9 | Same module |
| Second source | line 8, column 5 | line 5, column 9 | Same module |

The test compares the complete LIR modules and finds them equal.
A temporary counted-generator implementation also produces equal modules.
Its generator release instruction uses line 5, column 9 in both inputs.
That position names `print`, not the lexical exit.
The prototype test passes in 0.01 seconds, without Rust compilation.

Rule 5 requires the position of the operation that releases the generator.
The `t104` witness requires the inner closing brace, at line 31, column 5.
The current lowering substitutes its `print` at line 30, column 5.
No rule supplies a closing-brace position or defines the last statement as its replacement.
The count description and the edge ownership flags do not supply this source fact.

The lexical exit position must pass from the syntax form through HIR to the LIR release instruction.
A consumer cannot recover it from the current LIR.
Core principle 8 and the handoff's missing-input condition apply.
The independent Phase Review confirms this missing input in a realistic local-generator block exit.

### The trap witnesses need a sole task holder

The original `t103` and `t104` sources keep `h` in `use()` after the generator exits.
A generator release cannot end that local count (§171 rule 2 and §116.1 rule 4).
The final local release occurs after `after drop`, so those sources cannot prove a trap at the generator exit.

Each corrected entry creates `h` in an async helper that returns the generator.
The consumer awaits that helper before it starts the generator.
The helper's local `h` therefore ends before the generator exits.
The generator's array then holds the last count of the failed task.
The helper contains the same never-taken `await h` that satisfies the static observation check.

TypeScript 5.9.2 accepts `a346` and both corrected trap entries with exit 0.
The temporary tsconfig extends the repository tsconfig and includes all prelude declarations.
The pin count test uses one interpreter run, one JIT session, and one C build for each corrected entry.

| Corrected entry | Dev JIT | C AOT | Interpreter | Retained tasks J/C/I |
|---|---|---|---|---|
| `t103-dropped-generator-break` | `v 1`, `after drop`, `end`; no trap | Same | Same | 1/1/1 |
| `t104-dropped-generator-local` | `v 1`, `after drop`, `end`; no trap | Same | Same | 1/1/1 |

The two-entry pin count test takes 1.94 seconds, without Rust compilation.
Both entries remain Red.
Their goldens remain `v 1` only.
The interpreter trap table now names the corrected `break` at 28:54 and the block exit at 31:5.

### Prototype evidence and native layout

A temporary three-tier prototype gives zero retained tasks after the complete `a346` source.
That count test passes in 1.60 seconds, without Rust compilation.
The prototype preserves count multiplicity at each suspension and adds one ownership flag for each edge argument.
It handles the native generator's holder count through its static recursive release description.

| Native generator field | Pin offset | Prototype offset |
|---|---:|---:|
| State | 0 | 0 |
| Reload epoch | 4 | 4 |
| Resume function | 8 | 8 |
| Holder count | None | 16 |
| Static state cleanup pointer | None | 24 |
| Payload start | 16 | 32 |

The prototype stores one static cleanup pointer at each yield.
It does not allocate a cleanup description at each yield.
The prototype interpreter reports trap 29 at the failed task's `throw` position.
Rule 5 instead requires the generator release position.
A native correction also needs the complete release position from its LIR input.

The final production compiler, LIR lowering, runtime, and interpreter equal the pin.
The native layout therefore remains the pin layout.
No final implementation cost or generator workload cost exists.
The earlier interpreter pin cost remains 8.998 seconds, best of three.

### Final checks and file set

The C trap table also requires `t103` at 28:54 and `t104` at 31:5.
Both trap tables now use these positions.

| Check | Result |
|---|---|
| Workspace build, all targets, offline and locked | Pass |
| Compiler package tests, offline and locked | 1,091 pass; zero fail |
| Runtime package tests, offline and locked | 429 pass; zero fail |
| Codegen package tests, offline and locked | Library: 285 pass; one Red count test fails with six retained tasks |
| C emitter integration tests | 104 pass; the Red generator trap test fails |
| LIR integration tests | 55 pass; the Red interpreter generator trap test fails |
| Lexical exit position test | One pass; distinct exits still produce identical LIR |
| `cargo fmt --check` | Pass |
| Workspace Clippy, all targets, offline and locked | Pass; no new warning |
| `git diff --check` | Pass |

The codegen package command stops after the library failure.
The separate integration command checks the C emitter, LIR, and lexical exit test targets.
The native and interpreter trap tests emit `after drop` and `end` instead of the required trap.
The LIR corpus text and interpreter stdout goldens pass.
Every changed Rust file stays below 2,000 lines.
The implementation remains incomplete because the release instruction lacks its required source position.

The final file set contains these 13 files:

- `codegen/src/interpreter/counted_measurement_tests.rs`
- `codegen/tests/cemit.rs`
- `codegen/tests/lir.rs`
- `codegen/tests/lir-goldens/corpus.txt`
- `codegen/tests/generator_exit_positions.rs`
- `corpus/accept/a346-dropped-generator-frame.ts`
- `corpus/accept/a346-dropped-generator-frame.expected`
- `corpus/trap/t103-dropped-generator-break.ts`
- `corpus/trap/t103-dropped-generator-break.expected`
- `corpus/trap/t104-dropped-generator-local.ts`
- `corpus/trap/t104-dropped-generator-local.expected`
- `generated-docs/corpus-index.md`
- `specs/tracking/s176-count-measurement.md`

`tools/hygiene.sh` passes with exit 0.
