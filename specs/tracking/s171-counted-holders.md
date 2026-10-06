# §171 — An array owns its counted elements

Pin: `96f46f35`. Host: macOS, AArch64. Rust: `1.95.0`.

## Measurement round

`J/C/I` means dev JIT, C AOT, and interpreter, in that order.
Each number counts registered tasks after the roots finish and the ready queue becomes empty.
The JIT reads `ReloadSession::async_tasks`. C uses a non-null `subscript_rt_ctx_visit_async_tasks` visitor.
The interpreter reads its async registry and excludes entries whose owner count is zero.
A trap stops execution. A trap row counts the registry at that stop, before Context destruction.
`—` means that the checker rejects the program, or that the C compiler cannot build it.
A zero task count does not imply zero Context allocations.

All successful measurement programs use this task and root shape, unless a row states another shape:

```ts
async function work(): Promise<void> { return; }
export async function main(): Promise<void> { await use(); }
async function use(): Promise<void> { /* probe body */ }
```

A failed task replaces `return` with `throw new Error("inner")`.
An `if (false)` await satisfies the current must-await approximation without an observation.
No measurement invokes an implicit collector.

## 1. Holder positions

The checker admits arbitrary dynamic-array nesting. Depths two, three, and four give the same result.
The count protocol recognizes only a direct handle and a dynamic array whose immediate element is a handle.
`is_async_owner_type`, `acquire_owner`, `release_owner`, and `discard_owner` use this shallow definition.
A nested array can acquire an inner array's element counts at its element store, without an outer-scope release.

| Position or operation | Accepted form | Tasks J/C/I | Trap | Zero control J/C/I |
|---|---|---|---|---|
| `const` local handle | `const h=work(); await h;` | 0/0/0 | None | Same body: 0/0/0 |
| Local handle copy | A lambda returns its captured handle; the caller holds and awaits the result | 0/0/0 | None | A lambda copies the capture to a local: 0/0/0 |
| Dynamic handle array | `const a:Promise<void>[]=[work()]; await a[0];` | 0/0/0 | None | Same body: 0/0/0 |
| `let` array alias | `let b=a; await b[0]; b.pop();` | 1/1/1 | None | Keep the alias; omit `pop`: 0/0/0 |
| Conditional array alias | `const b=true?a:a; await b[0]; b.pop();` | 0/0/0 | None | Omit `pop`: 0/0/0 |
| Nested array, depths 2–4 | `const a:Promise<void>[][]=[[work()]]; await a[0][0];` and deeper equivalents | 1/1/1 | None | Pop the innermost array: 0/0/0 |
| `for...of` binding of an inner handle array | Iterate `[[work()]]`; await its inner element | 1/1/1 | None | After the loop, set `n[0]=[]`: 0/0/0 |
| Fixed array element | `const a:FixedArray<Promise<void>,1>=[h]; await a[0];`, with `h` already awaited | 1/1/1 | None | Put the entire fixed-array body in `if(false)`: 0/0/0 |
| Synchronous handle parameter and return | `identity(h)` returns its parameter; hold and await the result | 0/0/0 | None | Same body: 0/0/0 |
| Synchronous array parameter and return | `identity(a)` returns its parameter; hold `b`, then `await b[0]` | 0/0/0 | None | Same body: 0/0/0 |
| Async handle or array parameter | Await the parameter or its element in the callee | 0/0/0 | None | Same body: 0/0/0 |
| Synchronous method parameter | A method returns the handle parameter; hold and await the result | 0/0/0 | None | Same body: 0/0/0 |
| Async method parameter | A method awaits the handle parameter | 0/0/0 | None | Same body: 0/0/0 |
| Getter result | A getter returns a handle field; await the result, then free the object | 0/0/0 | None | Same body: 0/0/0 |
| Reference-class handle field | Construct `Box(h)`; await `o.h`; let the local object reference end | 1/1/1 | None | `Context.free(o)`: 0/0/0 |
| Reference-class handle-array field | Construct `Box([h])`; await `o.g[0]` | 1/1/1 | None | `Context.free(o)`: 0/0/0 |
| Reference-class nested-array field | Store `[[h]]`; await the element; free the object | 1/1/1 | None | Pop `o.n[0]` before free: 0/0/0 |
| Generic class field | `Box<Promise<void>>(h)`; await its field; free the object | 0/0/0 | None | Same body: 0/0/0 |
| Explicit collection after a field holder's scope | `main` awaits the helper above, then calls `Context.collect()` | 1/1/1 | None | Free the object in the helper: 0/0/0 |
| Module-global handle | Initialize `g=work()`; replace it with an awaited local handle; await `g` | 1/1/1 | None | No inhabited direct-global control can reach zero before Context destruction |
| Module-global handle array | Store `[h]` in `g`; await `g[0]` | 1/1/1 | None | `g.pop()`: 0/0/0 |
| Static handle field | Initialize `Box.h=work()`; await it | 1/1/1 | None | The same lifetime limit applies as for a direct global handle |
| Static handle-array field | Store `[h]` in `Box.a`; await its element | 1/1/1 | None | Set `Box.a=[]`: 0/0/0 |
| Synchronous lambda capture | Capture a `const` handle or handle array; return it; await the result | 0/0/0 | None | Copy the capture inside the lambda: 0/0/0 |
| Generator parameter and local | Copy the parameter to a local; consume one yield; drop the iterator | 1/1/1 | None | Consume the second `next`, which exhausts the body: 0/0/0 |
| Exhausted generator parameter and local | Same handle and array forms; exhaust the generator | 0/0/0 | None | Same body: 0/0/0 |
| Map handle value | `new Map<i32,Promise<void>>()`; `m.set(1,h)` | 1/1/1 | None | Keep the Map; put `set` in `if(false)`: 0/0/0 |
| Map handle-array value | `new Map<i32,Promise<void>[]>()`; `m.set(1,[h])` | 1/1/1 | None | Keep the Map; put `set` in `if(false)`: 0/0/0 |
| Map value removal | Store a handle, then `m.delete(1)` or `m.clear()` | 1/1/1 | None | Omit the store: 0/0/0 |
| Fulfilled async value | Return a handle, handle array, or nested handle array | 1/1/1 | None | See item 3 |

The generator probe transfers an element to a global array and immediately pops that element before its yield.
This transfer satisfies the generator parameter's must-await check without a persistent global holder:

```ts
let stored: Promise<void>[] = [];
function consume(h: Promise<void>): void { stored.push(h); }
function* gen(g: Promise<void>): Generator<i32> {
  const local = g;
  consume(local);
  stored.pop();
  yield 1;
}
```

The array version takes `g:Promise<void>[]` and passes `local[0]` to `consume`.
The caller first awaits `h`, creates the iterator, and calls `next` once or twice.
A suspended generator still holds its locals. Its dropped iterator gives no lexical exit.
An exhausted generator releases those locals.

A direct global handle has no lexical exit during the measured program.
The checker rejects `Promise<void>|null`, an uninitialized module variable, and `Context.free(g)` for a handle.
Thus no legal direct-global zero control exists before Context destruction.
The global array control supplies a removable global counted holder and reaches zero.
The fixed-array zero control preserves the accepted type position but executes no element store.
`FixedArray<Promise<void>,0>=[]` gives zero in JIT and interpreter; C rejects the emitted empty initializer.
An inhabited fixed array has no `pop`, no null element, and no automatic element release at scope exit.

| Rejected position or use | Measured diagnostic | Consequence |
|---|---|---|
| Inferred object literal with a handle field | S100, `object literals are not in the decided surface` | No runtime holder exists |
| Object literal contextualized as a reference class | S005, `object literals do not satisfy nominal class types` | No runtime holder exists |
| Source interface for an object literal | S100, `declaration form outside the decided surface` | No structural counted-field type exists |
| Value-class handle or handle-array field | S100, field outside the value-class whitelist | No value-class counted holder exists |
| `Promise<void>|null` or `Promise<void>[]|null` | S011, nullable non-reference type | Neither type permits a null reset |
| Tuple `[Promise<void>]` | S100, annotation outside the surface | Fixed arrays are the accepted fixed-size form |
| `Set<Promise<void>>` | S014, unsupported Map/Set key kind | No Set element holder exists |
| `Map.get` of a handle value | S014, the value lacks a null form | `Map.set`, `delete`, and `clear` still accept the value type |

A setter that merely stores its handle parameter gives S013, even with a matching getter.
A synchronous parameter that merely keeps an array, without a recognized observation or transfer, also gives S013.
The accepted parameter probes return a held handle or await an element.
These diagnostics restrict uses; they do not forbid the parameter's type position.

## 2. Array aliases and mutations

Each matrix probe first creates `a=[work()]` and awaits `a[0]`.
The `copyWithin` probe uses two distinct tasks and awaits both before the alias operation.
A synchronous helper returns `const keep=b[0]`; its caller holds and awaits that returned handle.
An async helper first awaits `b[0]`.
A return alias uses `identity(b):Promise<void>[] { return b; }`; the caller holds and awaits `b[0]`.
A global alias resets to `[]` after the operation. A field alias frees its object after the operation.

The fresh-value mutations use these bodies:

```ts
b.pop();
b.shift();
b.splice(0, 1);
b.push(work()); await b[b.length - 1];
b[0] = work(); await b[0];
b.unshift(work()); await b[0];
b.fill(b[0]);
b.reverse();
b.copyWithin(0, 1);
```

The synchronous helper uses `keep` for `push`, `unshift`, and the same-element index store.
The distinct-index probe returns `[keep,h]`, so the caller observes both parameters.
The field, global, and nested distinct-index probes store an already-awaited local `h`.
Fresh calls in those three index positions give S013; the observed-local forms are accepted.

Every numeric cell below means the same J/C/I count. Every cell has no trap.
`S` means the same-element index store. `D` means the distinct-element index store.

| Alias form | No mutation | `pop` | `shift` | `splice` | `push` | Index S / D | `unshift` | `fill` | `reverse` | `copyWithin` |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| Synchronous parameter | 0 | 1 | 1 | 1 | 0 | 0 / 1 | 0 | 1 | 0 | 1 |
| Async parameter | 0 | 1 | 1 | 1 | 0 | — / 1 | 1 | 1 | 0 | 1 |
| `const b=a` | 0 | 1 | 1 | 1 | 0 | — / 1 | 1 | 1 | 0 | 1 |
| Module global | 0 | 1 | 1 | 1 | 0 | — / 1 | 1 | 1 | 0 | 1 |
| Class field, explicitly freed | 0 | 1 | 1 | 1 | 0 | — / 1 | 1 | 1 | 0 | 1 |
| Synchronous return value | 0 | 1 | 1 | 1 | 0 | — / 1 | 1 | 1 | 0 | 1 |
| Nested array element | 1 | 1 | 1 | 1 | 1 | — / 1 | 1 | 1 | 1 | 1 |

For every matrix cell, its zero control keeps the alias and omits the mutation: 0/0/0, no trap.
The nested control also sets `n[0]=[]`, which releases the inner array's counted element copy.
These controls isolate the mutation. They do not establish correct ownership for a removal.
The single-owner controls below also execute `pop`, `push`, index store, and `reverse` with zero retained tasks.

| One local array holder | Probe tasks J/C/I | Trap | Zero control J/C/I |
|---|---|---|---|
| Discarded `pop` | 0/0/0 | None | Same operation: 0/0/0 |
| Fresh `push`, then await new element | 0/0/0 | None | Same operation: 0/0/0 |
| Distinct index store, then await new element | 0/0/0 | None | Same operation: 0/0/0 |
| `reverse` | 0/0/0 | None | Same operation: 0/0/0 |
| Discarded `shift` | 1/1/1 | None | Keep the method in `if(false)`: 0/0/0 |
| Hold and await the `shift` result | 1/1/1 | None | Keep the removal in `if(false)`: 0/0/0 |
| Discarded `splice` result | 1/1/1 | None | Keep the method in `if(false)`: 0/0/0 |
| Hold `splice` result; await its element | 1/1/1 | None | Keep the removal in `if(false)`: 0/0/0 |
| `fill` with the current element | 1/1/1 | None | Keep the method in `if(false)`: 0/0/0 |
| `unshift(work())`, then await new element | 1/1/1 | None | Keep the method in `if(false)`: 0/0/0 |
| `unshift(h)`, with an awaited local `h` | 1/1/1 | None | Keep the method in `if(false)`: 0/0/0 |
| `copyWithin(0,1)` over `[work(),h]`, with a live local `h` | 3/1/1 | JIT: use-after-delete; C/I: none | Keep the method in `if(false)`: 0/0/0 |
| Aliased `push(h)`, with a live local `h` | 2/0/0 | JIT: use-after-delete; C/I: none | Fresh `work()` input in the corresponding matrix cell: 0/0/0 |
| Spread, `slice`, or `concat` into a new array; await its elements | 0/0/0 | None | Same operation: 0/0/0 |

The live-local `h` probes expose an additional imbalance.
An alias takes counts only for elements present at its creation.
A later insertion creates too few counts for the number of aliases that release the new contents.
A later removal leaves the removed element's old alias counts without a release site.
`copyWithin` copies bytes without the element ownership operation.
A duplicate replacement can therefore release a task before its separate scalar holder exits.

| Other mutation or array use | Checker result | Tasks and trap |
|---|---|---|
| `b.length=0` | S100, outside the array surface | — |
| `b.sort(...)` on handles | S014, unsupported element kind | — |
| `b.filter(...)` or `b.map(...)` on handles | S014, unsupported element kind | — |
| Fixed-array `pop` | S018, no such method | — |
| `Context.free` on a fixed array | S100, expected `object` | — |
| Failed task removed through a synchronous array parameter, never observed | Accepted | 1/1/1; no trap |
| Failed task removed directly by `pop`, never observed | Accepted | 2/2/2 at trap 29, `Error: inner` |
| Observe that failed task in `try`, then pop directly | Accepted | 0/0/0; no trap |

The failed alias probe uses `if(false)` awaits for the array element and the helper's returned handle.
The direct release traps before the root and helper can finish their cleanup.
The two tasks at the trap are those unfinished enclosing frames; the failed task itself no longer has a registry entry.

## 3. Counted completions

The handle form uses `async function make():Promise<Promise<void>> { return work(); }`.
The array form uses `async function make():Promise<Promise<void>[]> { return [work()]; }`.
The checker accepts both forms. These numbers describe this compiler's three execution tiers.

The once probe holds `make()`, awaits it into `v`, then awaits the inner task.
The twice probe holds the outer handle and reads its completion twice into separate local owners.
The never probe keeps the outer handle through one checkpoint, with its only await in `if(false)`.
The failed-inner probe reads the outer completion and keeps the inner await in `if(false)`.
The observed control catches the inner failure through an actual await.

| Fulfilled shape | Completion reads | Tasks J/C/I | Trap | Zero control J/C/I |
|---|---|---|---|---|
| Handle | Once; inner success observed | 1/1/1 | None | Synchronous `make`, hold and await returned handle: 0/0/0 |
| Handle | Twice through a held outer handle | 1/1/1 | None | Synchronous `make`, await its held result twice: 0/0/0 |
| Handle | None after completion; outer holder exits | 1/1/1 | None | Synchronous `make`, same conditional inner await: 0/0/0 |
| Handle array | Once; inner success observed | 1/1/1 | None | Synchronous `make`, hold and await element: 0/0/0 |
| Handle array | Twice through a held outer handle | 1/1/1 | None | Synchronous `make`, await its held element twice: 0/0/0 |
| Handle array | None after completion; outer holder exits | 1/1/1 | None | Synchronous `make`, same conditional inner await: 0/0/0 |
| Handle | Inner failure remains unobserved | 1/1/1 | None; §116 trap does not fire | Synchronous `make`, catch inner failure: 0/0/0 |
| Handle array | Inner failure remains unobserved | 1/1/1 | None; §116 trap does not fire | Synchronous `make`, catch inner failure: 0/0/0 |
| Handle or handle array | Inner failure observed | 1/1/1 | None | Synchronous `make`, catch inner failure: 0/0/0 |
| Handle or handle array, synchronous return | Inner failure remains unobserved | 2/2/2 | Trap 29, `Error: inner` | Observe the inner failure: 0/0/0 |
| Nested handle array | Once; inner success observed | 1/1/1 | None | Synchronous return, then pop the inner array: 0/0/0 |
| Handle | `await make()` as a discarded statement result | 1/1/1 | None | Synchronous return held and awaited: 0/0/0 |

`async_complete` stores `Completion::Value(Vec<u8>)`.
`async_result` copies those bytes. It acquires no count for a counted result.
`async_release` removes the metadata and frees the frame. It releases no handle named by the cached bytes.
The current local binding acquires a count because the await's resume value is not a fresh owner.
Its lexical release balances that acquisition but leaves the cached owner count.
An unread completion retains that cached count without any local result holder.

## 4. Layout and operation cost

No dynamic-array allocation has a free header word.
The separate data allocation also has a complete allocation header and an element payload, without a holder-count slot.

| Region and offset from the array payload | Current field | Readers |
|---|---|---|
| Allocation header, -16, 8 bytes | Allocation state | `context/memory.rs` membership, mark, sweep, diagnostics; `context/lifecycle.rs` delete and reuse; JIT `lower/func/value.rs` lifetime check |
| Allocation header, -8, 4 bytes | Class id | `header_class_id`; memory inspection and scans; lifecycle delete and class lookup; JIT `lower/func/value.rs`; C `cemit/arith.rs` class tests |
| Allocation header, -4, 4 bytes | Position id | `header_pos_id`; allocation inspection, diagnostics, lifecycle attribution |
| Array payload, 0, 8 bytes | `len` | Memory tail scan; `array_len`, element access, mutation, truncate, extend; async aggregate input snapshot; C `cemit/access.rs`, `intrinsic.rs`, `iterator.rs`; JIT array length/access and iterator lowerings |
| Array payload, 8, 8 bytes | `cap` | Array capacity/growth, tail scan, truncate; runtime array methods through Context operations |
| Array payload, 16, 8 bytes | `elem_size` | Array byte range, element pointer, growth, pop, extend, truncate, tail scan; `array_elem_size`; array-method FFI, interpreter pack/unpack, aggregate input handling |
| Array payload, 24, 8 bytes | `data` | `array_data`, all array element/mutation paths, tail scan, aggregate result writes; C `cemit/access.rs`, `intrinsic.rs`, `iterator.rs`; JIT accesses through runtime array helpers |
| Proposed array payload, 32, 4 bytes | Array holder count | No current reader; the field does not exist |

`ArrayHeader` and emitted `SsArrayHeader` both have size 32 and alignment 8.
The existing layout test checks offsets 0, 8, 16, and 24.
Every existing field occupies its complete word; repurposing a high half requires changes to its full-width readers.

| Measured layout or allocation | Current | With proposed field | Cost |
|---|---:|---:|---|
| `repr(C)` array payload | 32 bytes | 40 bytes; `u32` count at 32 | 4 count bytes plus 4 alignment bytes |
| Dev exact-size allocation, header included | 48 bytes | 56 bytes | 8 bytes per array header |
| Ship arena payload accounting | 48 bytes | 48 bytes | Both requests use the same 64-byte block |
| Fresh ship arena reservation | 65,536 bytes | 65,536 bytes | Same initial chunk reservation |
| `AsyncFrameMeta` | 88 bytes, alignment 8 | 88 bytes with `u8 release_kind` | Tag uses offset 85 on this host |
| Metadata tail | `host_root` at 84; bytes 85–87 unused | Tag at 85; bytes 86–87 unused | No field moves in the measured tag prototype |

These metadata offsets describe this Rust build. They are not a portable ABI promise.
The metadata stores waiters at 0, completion at 24, task id at 48, result size at 56, and kind at 64.
It stores await position at 72, creation position at 76, and epoch at 80.
The existing completion option occupies 24 bytes; its value variant carries bytes, without a release descriptor.
The frame prefix remains state at 0, handle count at 4, and resume pointer at 8.
A metadata release tag needs no frame-prefix or public task-record field.

| Operation, for N elements | Current count work | Form A count work | Form B count work |
|---|---|---|---|
| Array alias acquisition and exit | N handle retains and N current-element releases | One array retain and one array release; the last exit releases N elements | Synchronous borrow: zero retains and releases |
| Store one copied handle | One element retain through the counted-store path | One element retain; release the previous element on replacement | Same element work in the unique owner |
| Store a fresh handle | Transfer where the operation carries the fresh-owner fact | Transfer one count | Transfer one count |
| Remove an element | `pop` transfers; other measured removals omit complete ownership handling | Transfer one count to the result; discard releases it | Same transfer and discard requirement |
| Final array holder exit | Each alias releases the array's current elements | One zero-count branch plus N element releases | N element releases at the unique owner's exit |
| Cached completion read | Byte copy; later local binding retains | Acquire the result's own count before the source frame releases | Handle-array result must transfer or satisfy the chosen single-owner restriction |
| Frame free with counted completion | No payload release | One typed payload release | One typed payload release for admitted result shapes |

The operation counts describe the proposed forms. They are not measured execution-time ratios.
Form A needs element ownership for `fill`, `copyWithin`, `shift`, `splice`, and `unshift`, in addition to `push`, `pop`, and index stores.
Its returned array aliases also need the array-count path. A byte-only intrinsic cannot supply these facts.
Form B removes alias element counts, but it does not repair the single-holder mutation defects in item 2.

## 5. Corpus reach and candidate forms

| Existing corpus id | Counted shape | Current tasks J/C/I | Candidate reach |
|---|---|---|---|
| `a155` | Local handle array, indexed awaits | 0/0/0 | A changes retain/release lowering; B keeps the unique local owner |
| `a161` | Global/field scalar stores, indexed array replacement, fresh spread array | 1/1/1 | A changes array ownership; B can keep fresh spread ownership; the global scalar remains live |
| `a162` | Handle-array constructor parameters, field stores, later field-to-local aliases, discarded pop | 0/0/0 | A changes array ownership; B rejects the borrow-to-field stores and the live field/local alias pattern |
| `a335` | Aggregate snapshots, duplicate inputs, index replacement after aggregate creation | 0/0/0 | A changes input-array lifetime; B keeps one array owner and a separate aggregate input snapshot |
| `a337` | Lexical TaskGroup with scalar task inputs | 0/0/0 | Neither array form changes its group-holder rules |
| `t83` | Handle array passed to `Promise.all`; scalar aggregate field dropped | 2/2/1 at trap 29 | A changes input-array lifetime; B admits its unique array owner |
| `r377` | A forbidden `void[]` aggregate result use | Rejected | Neither form changes the result-use rejection |
| `r378` | A non-handle-array aggregate argument | Rejected | Neither form changes the argument-type rejection |
| `r386` | TaskGroup in a generator; a global handle array in its surrounding program | Rejected | Array ownership changes the surrounding array; the group rejection stays |

| Existing trap id | Tasks J/C/I at stop | Trap | Candidate reach |
|---|---|---|---|
| `t66` | 1/1/1 | 29, `Error: never observed held` | Scalar last-release behavior stays |
| `t67` | 2/2/2 | 29, `Error: dropped` | Exception-exit scalar behavior stays |
| `t83` | 2/2/1 | 29, `Error: aggregate dropped` | Aggregate observation and array cleanup must preserve the report |

For `t83`, the native registries contain two tasks at the trap; the interpreter counted registry contains one.
All three report the same exception and source position.
The count at a trap therefore measures the stop state, not a completed cleanup.

`a186` mutates an `i32[]`, not a handle array.
`a336` stores async function values in an array, not async handles.
Neither is a direct array-count change site.
No committed accept entry directly returns a handle or handle array as an async fulfilled value.
The measured completion shapes reside in the count-form tests and the §166 rejection witness.

| Form | Evidence and required facts | Accepted programs that it changes or rejects | Existing tests and witness ids |
|---|---|---|---|
| A. The array owns its element counts | An array needs its own count. Payload offset 32 gives a 40-byte header. Aliases change that count only. Last release releases the elements. | Admits the measured alias forms. Changes removal, replacement, insertion, and nested-array lifetime. Does not by itself release Map values or fixed-array elements. | `codegen/tests/task_group_array_count_form.rs::synchronous_array_parameter_pop_keeps_an_orphan_count`; `runtime/tests/task_group_array_count_form.rs::empty_array_aliases_cannot_release_the_count_of_a_removed_element`; `codegen/tests/lir.rs::counted_store_verifier_reports_a_missing_retain`; `counted_store_corpus_matches_the_interpreter`; `compiler/src/lir.rs::tests::fresh_async_owner_instruction_table_names_allocations_and_calls` requires the corresponding fresh-result facts |
| B. A handle array has one owner | A synchronous parameter borrows. A borrow cannot escape into a field, global, result, nested element, or escaping closure. Moves need a consumed-source fact. | Rejects `a162` as written. Rejects the measured live `const`, `let`, return, global, field, nested, and conditional alias uses. A move can admit a source that is never read afterward. Async parameters need moves or rejection. | Both array count-form tests above change their expected count to zero for a synchronous borrow. `counted_store_corpus_matches_the_interpreter` loses `a162` acceptance. Constructor/field ownership cases need rejection witnesses. |
| Completion release | Registration carries a result-release kind. Frame free releases the cached owner. Each successful result read acquires one independent owner before frame release. | Repairs the measured handle and immediate handle-array completions. Does not require new acceptance. Does not remove §166's separate counted-result restriction without an explicit contract change. | `codegen/tests/task_group_count_form.rs::async_return_keeps_a_count_after_the_last_script_holder_exits`; `runtime/tests/counted_completion_form.rs::completion_release_does_not_release_a_counted_payload`; `PromiseAllCountedResult`, witness `s166-PromiseAllCountedResult`, variant `Divergence::PromiseAllCountedResult` |

The test inventory below gives the affected ids. No candidate is selected.

| Area | Existing test ids | Effect or limit |
|---|---|---|
| Fresh owners | `compiler/src/lir.rs::tests::fresh_async_owner_instruction_table_names_allocations_and_calls`; `compiler/src/hir/tests.rs::fresh_async_owner_expression_table_keeps_fresh_conditionals` | Completion result owners need a new fact at the await resume and expression discard |
| Aggregate input arrays | `codegen/tests/promise_all.rs::aggregate_corpus_matches_three_tiers_and_node_golden`; `temporary_input_arrays_release_their_counts_with_a_stored_array_control`; `unobserved_aggregate_exception_traps_with_an_observed_control` | A changes array retain/release. B keeps scalar input snapshots independent of the borrowed array |
| Aggregate runtime snapshots | `runtime/src/context/async_all_tests.rs::snapshot_duplicates_counts_and_completed_registration`; `collection_preserves_unread_inputs_partial_results_and_cached_results`; `first_failure_follows_reaction_order_and_later_failures_stay_observed` | Both forms must keep one aggregate count per input, including duplicates |
| Completion cache and collection | `codegen/tests/async_checkpoint.rs::a_completed_owned_result_survives_a_collection_inside_a_drain`; `a_no_print_ownership_loop_ends_with_no_live_context_payload` | Completion release must preserve ordinary reference results and scalar-result costs |
| Scalar count fast path | `codegen/tests/async_count.rs::async_counts_inline_and_check_pending_only_after_runtime_release`; `runtime/src/context/tests_0.rs::held_async_count_uses_emitted_header_and_frees_without_collect` | No array form changes frame offset 4 or the scalar count contract |
| Array layout | `runtime/src/context/tests_0.rs::array_header_offsets_match_the_abi_contract` | A changes size 32 to 40, but can preserve the existing four offsets |
| Corpus acceptance and LIR | `codegen/tests/lir.rs::every_corpus_entry_lowers_to_verified_lir`; `lir_interpreter_profile_matches_corpus_goldens`; `codegen/tests/cemit.rs::trap_corpus_entries_match_dev_stdout_on_both_tiers` | B changes `a162` acceptance. A and completion release must preserve unaffected outputs and trap reports |
| Rejection completeness | `compiler/src/check/rejection_total.rs::every_subset_rejection_carries_its_divergence` | B needs witnesses for new alias and escape rejections. A completion contract expansion changes `s166-PromiseAllCountedResult` |
| TaskGroup exclusion | `codegen/tests/task_group.rs::joined_scopes_release_all_tasks_after_success_and_failure`; `dropped_join_reports_a_late_failure_with_an_observed_control` | Group ownership stays lexical; candidate array and completion changes do not justify a group count |

A recursively held array needs a count operation at every enclosing store and exit.
Form A's immediate-array count alone cannot close the fixed-array, nested-container, Map, or collection shapes in item 1.
Form B must identify every escape of a borrowed or moved array, including generic fields and indirect calls.
Its rejection scope must state whether it also covers fixed arrays and Maps that hold handle arrays.

Completion release needs the result kind at registration, not a guess from the cached pointer bytes.
A compact tag covers a handle, an immediate handle array, and an uncounted value.
Deeper counted containers require a complete typed release operation or an explicit rejection.
With Form B, two awaits of the same cached array cannot each receive a unique owner of the same mutable array.
That combination needs a result copy, a borrow with a proved lifetime, or a rejection.

The await result is currently a borrowed resume value, with `fresh_owner=false`.
After result acquisition, the result must become a fresh owner that one store or discard consumes.
Both the HIR ownership classification and LIR resume-value verification need that fact.
A discarded `await make()` also needs its counted-result release.
A frame release can release a failed inner task and trap 29, so it must keep the existing release trap check.
