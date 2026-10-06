# §172 — Reference objects that hold counted values

Measurement pin: `9647c72abb684a1b5435401b0992b0a9be6c76db` (§171 implementation).
Host: macOS, AArch64. Rust: `1.95.0`.
`J/C/I` means dev JIT, C AOT, and interpreter.

Each task count measures registered tasks after the entry finishes and the ready queue becomes empty.
A trap row measures the count at the stop, before Context destruction.
A task count counts frames, not references to one frame.
Zero tasks does not imply zero Context allocations.
`—` means that the checker or LIR verifier rejects the source.
Unless a row states a trap, all three tiers report no trap.

The JIT uses `ReloadSession::async_tasks()`.
C uses `subscript_rt_ctx_visit_async_tasks` with a non-null visitor.
The interpreter uses `Interpreter::async_tasks()` and excludes completed frames whose owner count is zero.
The shared measurement method is in `codegen/src/interpreter/counted_measurement_tests.rs`.

Success probes use this body structure:

```ts
async function work(): Promise<void> { return; }
async function use(): Promise<void> {
  const h = work();
  if (false) { await h; }
  // The table supplies the holder and its operation.
}
export async function main(): Promise<void> { await use(); }
```

The false await satisfies S013 but does not observe the completion.
A field probe uses `Box<T>` with `v: T` and `constructor(v: T) { this.v = v; }`.
A concrete class uses the same field and constructor with the resolved type.
A distinct-value probe creates `q = work()` and awaits `q` before the store.
A Map probe uses `Map<i32, T>` and key `1`.
A zero Map control keeps the same type, value, and operations inside `if (false)`.
The zero field control calls `Context.free(o)` before the helper ends.
Zero controls for failed tasks first catch their exception.

## 1. Holder inventory and operations

`H`, `A`, `N`, and `F` name the first four types below.
`R` names the inferred type of `next()`; the source cannot spell `IterResult`.

| Shape | Source type or inferred value | Initial payload | Empty reset |
|---|---|---|---|
| H | `Promise<void>` | `h` | None; a replacement must hold a handle |
| A | `Promise<void>[]` | `[h]` | `[]` |
| N | `Promise<void>[][]` | `[[h]]` | `[]` |
| F | `FixedArray<Promise<void>, 1>` | `[h]` | None; the array must hold one element |
| R | `gen([h]).next()` | A result whose `value` is `[h]` | An exhausted generator result has no held task |

The R probes exhaust the generator before the holder test.
A generic function infers R and constructs `new Box<T>(v)` or `new Map<i32, T>()`.
An inferred class field also accepts R: `v = gen([]).next()`.
The generator publishes its parameter through a global array to satisfy S013, then the probe resets that array.

| Reference position | Checker acceptance | Tasks J/C/I after scope | Zero control J/C/I |
|---|---|---|---|
| Concrete instance field, H/A/N/F | Accepts each type | 1/1/1 | Free the instance: 0/0/0 |
| Generic instance field, H/A/N/F | Accepts each specialization | 1/1/1 | Free the instance: 0/0/0 |
| Inferred or generic instance field, R | Accepts the inferred type | 1/1/1 | Free the instance: 0/0/0 |
| Required descriptor field, `v!: Promise<void>` | Accepts `{v:h}` | 1/1/1 | Free the descriptor: 0/0/0 |
| Default descriptor field, `v?: Promise<void>[] = []` | Accepts `{v:[h]}` | 1/1/1 | Free the descriptor: 0/0/0 |
| Static field, H/A/N/F/R | Accepts an initialized or inferred field | 1/1/1 | Reset A/N to `[]`, or R to an exhausted result: 0/0/0; H/F have no empty form |
| Map value, H/A/N/F/R | Accepts `set` | 1/1/1 | Keep the store in `if(false)`: 0/0/0 |
| Map key, H/A/N/F | Rejects with S014, unsupported key kind | — | An empty numeric-key Map: 0/0/0 |
| Map key, inferred R | The key-kind predicate excludes the result type | —; source `typeof v` also receives S100 | An empty numeric-key Map: 0/0/0 |
| Counted field of a class in an array | Accepts `Box[]` | 1/1/1 after `pop` or collection | Free the Box before removal: 0/0/0 |
| Counted field of a class in a Map value | Accepts `Map<i32,Box>` | 1/1/1 after `clear`, free, or collection | Free the Box before container removal: 0/0/0 |
| Counted field of a class in a Map key | Accepts `Map<Box,i32>` | 1/1/1 after `clear` | Free the key object: 0/0/0 |
| Counted field of a class in a Set | Accepts `Set<Box>` | 1/1/1 after `clear` or free | Free the element object: 0/0/0 |
| Counted field of a class in another class | Accepts the outer reference field | 1/1/1 after free of the outer object | Free the inner object, then the outer object: 0/0/0 |
| Async frame locals, parameters, and counted completions | Accepts counted values under §70 and §171 | Normal completion and last-owner exit: 0/0/0 | Same shapes with repeated completion reads: 0/0/0 |
| Suspended generator parameter and local, A | Accepts a counted parameter | Drop after one `next()`: 1/1/1 | Exhaust the generator: 0/0/0 |
| Local closure environment, H/A | Accepts a synchronous capture | 0/0/0 | Keep the same lambda call and await: 0/0/0 |
| Captured lambda in a class field | Rejects with S009 at the constructor argument | — | The local lambda form: 0/0/0 |
| Captured lambda in a Map value | Rejects with S009 at the Map store | — | The local lambda form: 0/0/0 |

A class reference is not a counted type.
An array, Map, Set, or outer class that holds a Box does not own the Box's field counts.
Async frame owners and counted completion caches follow §171, rather than an object-sweep field rule.
A dropped suspended generator keeps its holders (§171.3 item 1).
A local lambda borrows its capture; the environment takes no count (§116.1 rule 4c).
A static field lowers to a module global, rather than an instance field.
An inhabited H or F static field always owns a task in these one-element probes.
Neither type permits a null or empty reset; no inhabited same-type zero control exists before Context destruction.

| Store, replacement, or removal | Shapes | Tasks J/C/I | Zero control J/C/I |
|---|---|---|---|
| Constructor assignment `this.v=v`; object scope ends | H/A/N/F/R | 1/1/1 | Free the object: 0/0/0 |
| Field initializer `[work()]`; await its element; free | A | 0/0/0 | Same body: 0/0/0 |
| Initial empty field; store `[h]`; free | A | 0/0/0 | Same body: 0/0/0 |
| Same-value field store; free | H/F/R | 0/0/0 | Same body: 0/0/0 |
| Distinct-handle field store; free | H | 0/0/0 | Same body: 0/0/0 |
| Empty-array field replacement; free | A/N | 0/0/0 | Same body: 0/0/0 |
| Setter writes the field; free | H | 0/0/0 | Same body: 0/0/0 |
| Descriptor literal supplies a required or default field; free | H/A | 0/0/0 | Same body: 0/0/0 |
| Static assignment; then replacement with the same occupied value | H/F | 1/1/1 | No inhabited pre-destruction zero control |
| Static assignment; then replacement with `[]` | A/N | 0/0/0 | Same body: 0/0/0 |
| Static inferred R assignment; reset to exhausted `next()` | R | 0/0/0 | Same body: 0/0/0 |
| `Map.set(1,v)` into an empty Map | H/A/N/F/R | 1/1/1 | Omit the store: 0/0/0 |
| `Map.set(1,v)` over the same key and value | H/A/N/F/R | 1/1/1 | Omit both stores: 0/0/0 |
| `Map.set(1,newValue)` over a distinct task | H/A/N/F | 2/2/2 | Omit both stores: 0/0/0 |
| `Map.set(1,[])` over a counted array | A/N | 1/1/1 | Omit the stores: 0/0/0 |
| `Map.delete(1)` after a store | H/A/N/F/R | 1/1/1 | Omit the store: 0/0/0 |
| `Map.delete(2)` when only key 1 exists | H | 1/1/1 | Empty Map: 0/0/0 |
| `Map.clear()` after a store | H/A/N/F/R | 1/1/1 | Omit the store: 0/0/0 |
| 100 distinct keys hold one task; `Map.clear()` | H | 1/1/1 | Omit the stores: 0/0/0 |
| `Map.getOr(1,h)`; await the result | H | 1/1/1 | Omit `set`, use the fallback, then await: 0/0/0 |
| Iterate `Map.values()` and await each result | H | 1/1/1 | Empty Map: 0/0/0 |

The setter probe also stores its input in a global array, then resets that array.
This supplies the synchronous parameter's S013 observation path without an extra final holder.
The 100-key probe measures growth and rehash as part of `set`.
It does not equate one retained task with one retained reference.

| Other accepted or rejected form | Evidence | Consequence |
|---|---|---|
| `Map.get` of H | S014: the value type has no null form | `getOr` supplies the accepted read |
| `Map.forEach` stores H in a global array | Checker accepts; LIR rejects a counted store without a retain | No three-tier executable exists for this form |
| Map constructor with `[[1,h]]` | S100: mixed array element types | No accepted entry-literal store exists in this probe |
| Source `IterResult<Promise<void>[]>` | S016: unknown type name | Inference and generic substitution still admit R |
| `Context.free` through a parameter of type `object` | S011: `object` is boundary-only | A general script helper cannot erase the class type this way |
| `Context.free` of a class array | S100: the argument expects `object`, got `Box[]` | Free each Box, rather than the array through this API |
| Value-class counted field | The value-field whitelist excludes handles and counted arrays | No value-class reference holder exists |
| Interface or ordinary object literal | §171 inventory records S100 or S005 | The descriptor is the accepted nominal literal holder |
| TaskGroup field, element, result, or capture | §170 restricts the group to a lexical local | A TaskGroup is not a §171 counted type |
| Async arrow capture | §167 rejects the capture | It does not add a counted closure owner |

An outer-block lambda can still outlive a counted inner binding (§171.3 item 3).
The H probe accepts the source and stops with 3/3/3 tasks.
JIT reports use-after-delete; C reports trap 11; the interpreter reports `async resume without completion`.
The local-capture control gives 0/0/0.
This capture lifetime differs from the reference-holder release problem.

## 2. Object end

| Holder | Typed `Context.free` | Unreachable script collection | Reachable script collection | Host collection after entry | Context destruction | Zero control |
|---|---|---|---|---|---|---|
| Concrete or generic field, H/A/N/F | 0/0/0 | 1/1/1 | 1/1/1 | 1/1/1 | Registry ends; no new trap | Free before collection: 0/0/0 |
| Inferred or generic field, R | 0/0/0 | 1/1/1 | 1/1/1 | 1/1/1 | Registry ends; no new trap | Free before collection: 0/0/0 |
| Descriptor field, H/A | 0/0/0 | 1/1/1 | 1/1/1 | 1/1/1 | Registry ends; no new trap | Free the descriptor: 0/0/0 |
| Map value, H/A/N/F/R | 1/1/1 | 1/1/1 | 1/1/1 | 1/1/1 | Registry ends; no new trap | Omit the store: 0/0/0 |
| Counted field inside an array, Map value/key, Set, or outer class | Free the inner class: 0/0/0; free the Map/Set/outer class: 1/1/1 | 1/1/1 | 1/1/1 | 1/1/1 | Registry ends | Free the inner class: 0/0/0 |
| Static field, H/F | Free an instance: 1/1/1 | Not applicable; the module global is a root | 1/1/1 | 1/1/1 | Registry ends; no new trap | No inhabited pre-destruction zero control |
| Static field, A/N/R | Free an instance: 1/1/1 for A/N | Not applicable; the module global is a root | 1/1/1 | 1/1/1 | Registry ends; no new trap | Reset to `[]` or an exhausted R: 0/0/0 |
| Local borrowed capture, H/A | No accepted object-free form | No environment owner exists | No environment owner exists | The local ends with 0/0/0 | Registry ends | Same local form: 0/0/0 |

Native collection retires an unreachable class allocation without a field release.
It retains the frame because `async_frames` supplies a root for each registered frame.
Map sweep also lacks a release for the counted contents.
A reachable object keeps its field or value, as expected.

The interpreter registers each allocated handle as a permanent root in `Interpreter::root_handle`.
Its unreachable class and Map allocations remain reachable to the runtime collector.
Thus equal task counts do not prove equal object reclamation.
The cost table below independently measures this difference with uncounted objects.

A trap observer installed immediately before destruction receives zero calls in J/C/I.
The destruction probes cover H/A/N/F fields and Map values, R fields, descriptors, and static H/A/F/R fields.
They also cover unobserved failures in a field and a Map value.
A destroyed Context has no task registry; no post-destruction registry count exists.
`Context::drop` frees allocation storage directly and does not call the typed counted release.
The interpreter clears scheduler storage without exception delivery.

## 3. Collection, an exception, and an unfinished frame

A failed task throws `Error("inner")`.
The sole-field probes create the failed task in a synchronous helper and return its Box.
The helper's local owner ends before the caller releases the field.
The caller also holds an unrelated successful task; it accounts for one task in the trap count.

| Probe | Tasks J/C/I | Trap J/C/I | Zero control J/C/I |
|---|---|---|---|
| Unobserved failure in an unreachable field; script collection | 1/1/1 | None; script prints `after` | Catch the failure and free the Box: 0/0/0 |
| Unobserved failure in a reachable field; script collection | 1/1/1 | None; script prints `after` | Catch the failure and free the Box: 0/0/0 |
| Unobserved failure in a field; host collection | 1/1/1 | None inside the host call | Catch the failure and free the Box: 0/0/0 |
| Unobserved failure in a Map; script or host collection | 1/1/1 | None | Observe the task and omit the store: 0/0/0 |
| Unobserved failure in a Map; delete, clear, or free | 1/1/1 | None | Observe the task and omit the store: 0/0/0 |
| Free the sole failed field owner | 3/3/3 at the stop | Trap 29, `Error: inner`, during `Context.free` | Observe the field first, then free: 0/0/0 |
| Replace the sole failed field owner | 3/3/3 at the stop | Trap 29, `Error: inner`, during the field store | Observe the field first, replace, then free: 0/0/0 |
| Free the field while a local still owns the failed task | 2/2/2 at the stop | Trap 29 at the local exit, after the free returns | Catch the local await, then free: 0/0/0 |
| Destroy a Context that holds an unobserved failure | 1/1/1 before destruction | No new trap during destruction | Observe and free the field, or omit the Map store: 0/0/0 |
| Field holds a task with two `Context.suspend()` calls; script collection | 1/1/1 after completion | None; the task prints `finished` | Free the typed Box: 0/0/0 |
| Map holds the same unfinished task; script collection or free | 1/1/1 after completion | None; the task prints `finished` | Omit the store: 0/0/0 |
| Native runtime: unreachable holder contains an unfinished frame | 1/1 for dev/ship | None; holder dies, frame stays live, count stays 1 | Explicit frame release: 0/0 |

No current collection release frees an unobserved failed frame in these probes.
Therefore neither the script call nor the host collection call reports the required exception today.
An A-form release needs trap propagation in both calls; current field-free traps do not establish collection trap propagation.

The native collector marks registered frames regardless of completion.
It also marks ready, parked, stopped, blocked, and active scheduler references.
A frame can therefore survive the sweep of its unreachable holder.
A frame that points back to the holder makes that holder reachable through the frame root.
This last result follows from the mark code, not a separate cycle probe.

## 4. Sweep order

The order probe allocates eight unrooted objects with IDs 0–7.
Their payload sizes are `32, 96, 32, 4096, 8192, 96, 8192, 32` bytes.
A sweep hook records each ID before the collector retires the allocation.
All eight objects contain no counted value and leave zero allocations after native collection.

| Tier or path | Measured visits | Order rule | Same order in all tiers? |
|---|---|---|---|
| Dev runtime, pass 1 | `3,7,2,6,4,1,5,0` | `allocations.extract_if`, hash-table order | No |
| Dev runtime, pass 2 | `5,0,7,3,1,4,2,6` | The next Context has another hash-table order | No |
| Dev runtime, pass 3 | `0,5,1,7,6,4,3,2` | No declaration or allocation order | No |
| Ship runtime, pass 1 | `0,2,7,1,5,3,4,6` | Chunks first; each chunk's blocks in increasing index; then the large-object hash table | No |
| Ship runtime, pass 2 | `0,2,7,1,5,4,6,3` | The small-block prefix stays; the large-object suffix changes | No |
| Ship runtime, pass 3 | `0,2,7,1,5,3,4,6` | Same chunk rule; no stable large-object rule | No |
| Interpreter, script-allocated objects | No unreachable-object visits | Permanent per-handle roots keep these objects live | No |

JIT uses the dev runtime sweep.
The interpreter uses that runtime too, but its permanent roots change which objects the sweep can visit.
Two counted field holders leave 2/2/2 tasks after one script collection.
Free both holders before that collection and the same probe leaves 0/0/0.
No holder release or exception order exists at collection today.
A release in the present sweep order cannot promise the same first exception across tiers.

## 5. Collection cost with no counted value

The native baseline times only `Context::collect`, excluding allocation and destruction.
Each sample uses a fresh Context and zero-filled 32-byte objects with class ID 900.
Diagnostics are off. The release profile removes debug assertions.
Each cell gives the median of 11 samples, in microseconds.
Rooted samples register one permanent range that contains all object addresses.
Unrooted samples retire every object; rooted samples retain every object.
All samples retain zero tasks and report no trap.

| Objects | Dev, unreachable | Ship, unreachable | Dev, reachable | Ship, reachable |
|---:|---:|---:|---:|---:|
| 0 | 0.166 | 0.125 | 0.125 | 0.166 |
| 1,000 | 13.875 | 0.916 | 65.416 | 32.542 |
| 10,000 | 136.292 | 10.708 | 672.042 | 434.959 |
| 100,000 | 1,402.625 | 107.167 | 7,471.084 | 6,270.208 |

A separate three-tier probe allocates `Cell { v: i32 }` objects in a script loop.
The async entry finishes before the host calls collection.
The heap then contains no registered task and no counted field.
Seven fresh runs give each median below, in microseconds.
The C executable uses C11 optimization and the release runtime archive.

| Script objects | JIT median | C AOT median | Interpreter median | JIT live allocations before/after | Interpreter live allocations before/after | Tasks and trap J/C/I |
|---:|---:|---:|---:|---|---|---|
| 1,000 | 14.667 | 67.000 | 22.041 | 1,000 / 0 | 1,000 / 1,000 | 0/0/0; none |
| 10,000 | 138.875 | 72.000 | 190.458 | 10,000 / 0 | 10,000 / 10,000 | 0/0/0; none |

The same script with zero loop iterations supplies the task-count control: no task remains after the entry.
The native empty-heap rows supply the collection-cost control.
The script Cells have 4-byte payloads; the native objects have 32-byte payloads.
The two baselines use different allocation histories; their timings are not interchangeable.
A comparison of release-step cost must keep the payload shape, roots, allocation history, and diagnostics mode fixed.

## Candidate forms and corpus reach

| Form | Evidence at HEAD | Required coverage | Limit of the evidence |
|---|---|---|---|
| A. Release at every end | Field replacement and typed field free already release H/A/N/F/R; Map removal and collection retain tasks | Preserve existing field releases; add typed Map replacement/removal and typed object sweep releases | No A implementation or A cost ratio is measured here |
| A. Type description | HIR and LIR carry recursive counted types; runtime allocations carry a class ID | Connect each class field offset and each Map value slot to its §171 rule 7 release description | The allocation header alone carries no field release description |
| A. Map storage | `AssocHeader` stores sizes, key kind, and storage pointers; `insert` overwrites bytes; `delete` zeroes bytes; `clear` retires backing storage | Release before removed slots or storage cease to hold the old value | Growth and compaction move owned entries; they must not duplicate or release the moved ownership |
| A. Explicit object free | The lowering loads typed class fields and releases them around `UnsafeDelete` | Give sweep and host free the same description; prevent duplicate field release | Runtime `Context::delete` alone does not know user-class field types |
| A. Completion and scheduler roots | Every registered frame roots itself, and its payload can root its holder | State how release interacts with unfinished tasks and cycles | A frame-to-holder cycle remains marked; a field-release sweep cannot visit it |
| A. Trap and order | Failed field release traps; collection releases nothing; native sweep orders differ | Script and host collection must report traps and define the first reported failure | The current order provides no three-tier failure-order guarantee |
| B. Reject a counted class field or Map value | A type-based rejection catches direct, nested-array, fixed-array, and inferred R positions | Apply after generic substitution and field inference; include descriptor and static fields if “class field” includes them | Reject the counted field itself; rejecting only direct handles leaves recursive holders accepted |
| B. Indirect holder | `Box[]`, `Map<i32,Box>`, `Map<Box,i32>`, and `Set<Box>` hold uncounted class references | The Box declaration receives the rejection | A container-only ban misses the counted field inside Box |
| B. Closure | Async frame owners and counted completion caches follow §171, rather than an object-sweep field rule.
A dropped suspended generator keeps its holders (§171.3 item 1).
A local lambda borrows its capture; escaping stores receive S009 | Keep capture checks separate | The outer-block capture defect remains outside the field/Map rule |

The checked accept and trap corpus contains five entries with counted class fields.
No checked corpus entry has a counted Map value or counted static field.
The Map-value check runs at `Checker::container_argument`, including generic substitution.
The class-field inventory reads the resolved HIR field types.

| Form | Corpus reach | Result |
|---|---|---|
| A | `a161-counted-handle-stores`, `a162-async-copy-sites` | Existing field stores and explicit releases require preservation |
| A | `a260-awaited-handle-outlives-its-holder` | A waiter must preserve its task when a field holder ends |
| A | `t66-unobserved-async-exception`, `t83-unobserved-aggregate-exception` | Existing free-at-zero traps require preservation |
| A | No current counted-Map or counted-field-collection entry | These table shapes need corpus reach before implementation |
| B | `a161-counted-handle-stores` | Rejects `HandleHolder.handle` |
| B | `a162-async-copy-sites` | Rejects `HeldArrays.forOfHandles`, `continuedHandles`, `indexed`, and `popped` |
| B | `a260-awaited-handle-outlives-its-holder` | Rejects `Holder.job` |
| B | `t66-unobserved-async-exception` | Rejects `Holder.job` |
| B | `t83-unobserved-aggregate-exception` | Rejects `Holder.job`, whose handle completes with an uncounted `i32[]` |
| B | `a338`, `a339`, `a340`, and the §171 array/completion trap entries | Their local arrays, completions, and module globals remain outside this rejection |

The B rejection also reaches existing tests below.
These tests receive a counted-field diagnostic before execution under the measured rejection.
The list states the exercised tests, not an exhaustive count of every test that reads those corpus entries.

| Test file | Test names or measured group | B result |
|---|---|---|
| `codegen/tests/exceptions.rs` | `an_unobserved_exception_traps_when_its_count_reaches_zero` | Rejects the Holder field |
| `codegen/tests/exceptions.rs` | `a_waiting_await_holds_an_exception_handle`, `a_waiting_await_holds_a_value_handle` | Rejects the Holder field |
| `codegen/tests/lifetime_operands.rs` | `double_delete_keeps_the_call_position_with_async_owner_fields` | Rejects the handle field |
| `codegen/src/interpreter/counted_measurement_tests.rs` | `array_alias_measurement_rows_and_zero_controls_release_every_task` | Rejects the declared Box field, including rows that do not instantiate Box |
| Same file | `exact_holder_rows_and_their_zero_controls_release_every_task` | Rejects Holder and Nested counted fields |
| Same file | `fresh_mutation_rows_and_same_element_parameter_control_release_every_task` | Rejects the Box field |
| Same file | `measured_fresh_index_positions_still_have_no_accepted_form` | Rejects the positive field control |
| Same file | `deferred_reference_holders_and_dropped_generators_keep_the_measured_count` | Rejects the counted field; the Map row also conflicts with B |
| Corpus-derived output and LIR tests | The five affected corpus entries | Their accepted or trap forms become checker rejections |

Form A preserves the existing holder surface and needs runtime type descriptions, trap delivery, and a sweep-order rule.
Form B removes the five corpus forms and the counted field/Map test shapes above.
Neither form is selected by these measurements.
