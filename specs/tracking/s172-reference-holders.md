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

## 6. Contract-pin corpus and input forms

The contract pin is `dbccd4d8`.
TypeScript 5.9.2 accepts each of the seven new corpus entries with exit 0.
Each TypeScript configuration extends the repository configuration and includes the ambient prelude.
The CLI reads copies of the entries from a temporary directory.
The pin CLI and the interpreter test binary use the pin production code.

| Entry | Pin JIT | Pin C AOT | Pin interpreter | Contract result |
|---|---|---|---|---|
| `a341-reference-holder-release` | Expected output; 6 tasks | Expected output; 6 tasks | Expected output; 6 tasks | Expected output; 0 tasks |
| Same-shape `a341` control without stores | 0 tasks | 0 tasks | 0 tasks | 0 tasks |
| `t94-counted-map-delete` | Prints `after`; no trap | Prints `after`; no trap | Prints `after`; no trap | Trap 29 before the print |
| `t95-counted-map-clear` | Prints `after`; no trap | Prints `after`; no trap | Prints `after`; no trap | Trap 29 before the print |
| `t96-counted-map-replace` | Prints `after`; no trap | Prints `after`; no trap | Prints `after`; no trap | Trap 29 before the print |
| `t97-counted-field-collect` | Prints `after`; no trap | Prints `after`; no trap | Prints `after`; no trap | Trap 29 at collect, with `Error: first` |
| `t98-counted-field-task-order` | Prints `after`; no trap | Prints `after`; no trap | Prints `after`; no trap | Trap 29 at collect, with `Error: first` |
| `r388-counted-map-for-each` | Exit 2; LIR store error | Exit 1; LIR store error | LIR store error before execution | S014 at `Map.forEach` |

The `a341.expected` values come from the constant arguments of `work` and the last value that each key holds.
The sequence is `11, 22, 44, 44, 66, 66, 88, 88, 101, 102`, with one newline after each value.
The trap goldens contain zero bytes: the last-owner release must trap before the only print.
Rules 1 and §116.1 rule 4 give the Map traps their failed task's throw position, line 10, column 40.
Rule 5 gives `t97` the collect position at line 19, column 3, and `t98` line 20, column 3.
Rule 4 selects `Error: first` in `t98`, because that failed task receives the lower task id.

The checker rejects counted `Map.forEach` value types with S014.
The direct test covers a handle, an array, a nested array, a FixedArray, and generic substitution.
The same-shape uncounted Map callback remains accepted.
The checker also rejects a counted callback result on an uncounted Map, under §171 rule 5a.
TypeScript 5.9.2 accepts that callback-result witness with exit 0.
The §154 table names both Map callback witnesses.

| Additional path | Classification | Required count action |
|---|---|---|
| `Map.getOr`, including its fallback argument | Borrow | Only a holder that stores the result acquires |
| `Map.values()` and `Map.entries()` element reads | Borrow | The loop binding or another stored holder acquires |
| `new Map(source)` | Copy site under §70 rule 2a | The new Map acquires each counted value |
| `Map.forEach` with a counted callback result | Callback under §171 rule 5a | The checker rejects it with S014 |

### Pin allocation and registry evidence

At `dbccd4d8`, three forms lack facts that a reference release needs.
The contract at `0f6ba597` adds the allocation descriptions and the tier-supplied release consumer.
The production source trees of these two pins are identical.

| Form and consumer | Available fact | Missing fact |
|---|---|---|
| Runtime class allocation; `Context::delete` and sweep | Class id, payload size, position | Counted field offsets and recursive release descriptions after generic substitution |
| `AssocHeader`; Map replacement, removal, free, and sweep | Key kind, value width, entry stride, storage pointers | The counted value's recursive release description |
| Interpreter packed handle; runtime collection release and task-id order | `Rc<RefCell<Coroutine>>` address in `async_handles` | A release consumer and task-id resolver for the interpreter registry |

The interpreter writes an Rc address in `memory::pack_into` for `Type::AsyncHandle`.
The runtime release leaf calls `Context::async_release`, which resolves only the runtime's `async_frames` registry.
An interpreter Rc address has no entry in that registry, so this consumer releases nothing.
The collector receives no interpreter registry or resolver before it frees allocation storage.
A native release description alone cannot give this consumer the missing interpreter facts.
The pin measurements establish that the native registry does not resolve an interpreter handle.
The contract supplies separate release consumers and task-id resolvers for the two registries.

## 7. Map ownership on the tree

The LIR records the Map value's count action for creation, copy, reads, mutation, and explicit free.
The verifier derives the action from the Map type.
A counted action carries a Call trap site.
The direct verifier tests construct missing actions at four counted depths and a release without a Call trap.
The guarded release supplies the same-shape control.

The lowering acquires only the stored argument of `Map.set`.
The fallback argument of `Map.getOr` borrows its value.
The Map read result also borrows; its stored holder acquires through the existing copy path.
A fresh fallback stays live until the result receives its own count.

The native Map header carries a static recursive value description.
The JIT and C emitter install it before the first store.
Replacement snapshots the old value, stores the acquired value, then releases the old value.
Delete snapshots the removed value before it clears the slot.
Clear snapshots all active values before it retires the backing storage.
The interpreter performs these releases through its own frame registry.
Explicit Map free uses the same release actions.

A Map copy acquires each value after the destination stores it.
An allocation failure stores no value and acquires no unmatched count.
The failure test covers the three allocation points and the successful control.
The corrected allocation-failure test reports no retained count.

Recursive array and FixedArray releases continue after the first failed leaf.
The native Context keeps the first trap, and the interpreter keeps the first error.
Both consumers release the remaining leaves and retire the array storage.
The Map clear test includes two arrays with two failed leaves each and a same-shape fulfilled control.

| Shape | Interpreter/JIT/C tasks | Control tasks |
|---|---|---|
| Map handle, array, nested array, and FixedArray mutations and borrowed reads from `a341`, with the field holder omitted | 0/0/0 | 0/0/0 |
| Copy a counted Map, clear its source, await the copy, then free both Maps | 0/0/0 | 0/0/0 |
| Free a Map that owns a handle array | 0/0/0 | 0/0/0 |
| Full `a341`, including the unreachable class field | 1/1/1 | 0/0/0 |

The native `t94`, `t95`, and `t96` results match the interpreter.
Each reports trap 29, `Error: lost`, at line 10, column 40, with no output.
`t97` and `t98` still print `after` with no trap.
Class sweep release and collection trap order remain absent.

### Coverage and checks

| Handoff item | Tree state |
|---|---|
| 1: Red corpus, TypeScript headers, pin results, goldens, registrations, generated index | Complete; the amended contract pin has the same production source as the measured pin |
| 2: Map ownership and LIR actions in all tiers | Complete; direct count-action and trap-site tests cover the verifier |
| 3: Class descriptions and object free/sweep releases | Class allocations carry resolved field descriptions; class and Map free release their values; sweep releases remain absent |
| 4: Ordered collection releases and script/host traps | Absent |
| 5: Last release of an unfinished frame | No new implementation or coverage |
| 6: Interpreter roots and allocation-count parity | The interpreter still uses permanent allocation roots |
| 7: Measurement-table unit tests with controls | Map tests cover mutations, borrows, copy, free, and allocation failure; class free covers H/A/N/F/R and descriptors; collection rows remain uncovered |
| 8: Collection cost and async-cost ratios | No tree/pin ratios measured |

The codegen library run reports 264 passes and two existing Red test failures.
The direct trap-site test passes.
The HIR fact check, host-entry declaration check, and LIR text golden check pass.
The native trap corpus test fails only for `t97` and `t98`.
The existing interpreter collection tests retain their previous results.
The three runtime Map tests pass.
The Map-only tests create six C builds and six JIT sessions.
Their separate debug runs take 2.66, 0.94, and 0.87 seconds after the Rust build.
The workspace builds with all targets.
The compiler and runtime suites pass.
Rustfmt passes, and no changed Rust file exceeds 2,000 lines.
Clippy exits 0; no new warning refers to the implementation or its new tests.
The existing `group_by` warning moves with its source line.
The hygiene check passes.
No existing `.expected` file changes.

## 8. Class allocation descriptions and explicit free

The LIR class carries each counted field offset and its recursive count action after generic substitution.
The verifier compares the release records against independently computed class-layout offsets and types.
A direct test constructs a class with an omitted counted field description and a complete control.
The class-free check derives the release obligation from the receiver type and requires a Call trap.
A direct test constructs an unguarded class free and its guarded control.

The JIT, C emitter, and interpreter derive field offsets from the resolved C class layout.
Each counted class allocation installs its field description in the Context.
The Context copies the description, so its storage does not depend on a reload generation's code storage.
An uncounted class installs no description.
The interpreter skips description construction for an uncounted class.
The LIR text shows each field release action.

An explicit free removes the description before it releases any leaf.
The release plan returns handle keys and the storage of arrays whose last holder ends.
Native free resolves each key through the native frame registry.
Interpreter free resolves each key through the interpreter frame registry.
Each consumer releases every leaf before it frees the returned array storage and the class object.
Both consumers retain the first error and continue the remaining releases.
The lowering no longer snapshots and releases class fields after the runtime delete.

Sweep removes dead descriptions before an address can be reused.
A direct test reuses a reclaimed class address for an uncounted allocation and verifies that free preserves its borrowed frame.
Sweep still reclaims class storage without a field release.
The collection release consumer, task-id order, and collection trap propagation remain absent.
The interpreter still roots each script allocation permanently.

| Test shape | Interpreter/JIT/C tasks | Control tasks |
|---|---|---|
| Explicit class free: handle, array, nested array, FixedArray, inferred generator result, and required descriptor field | 0/0/0 | 0/0/0 |

The class-free test creates two JIT sessions and two C builds.
Its separate debug run takes 1.24 seconds after the Rust build.
The three class-description verifier tests pass.
The five runtime object tests pass.
The runtime tests cover native free, foreign registry keys, deferred array storage free, address reuse, and all failed-field releases.
Each runtime shape includes a control.
The generated-code description API has a direct test.
The execution-fact audit requires one receiver check for a class free.
The runtime field description replaces the former extra LIR field reads.
The HIR execution-fact check passes with that contract.
The integration tests report one remaining failure: the native trap corpus still receives no trap for `t97` and `t98`.
The LIR text golden check passes.
The full LIR test suite reports 55 passes; its former field-read count failure passes in the direct execution-fact run.
The hygiene check passes.
The host collection API does not change.

The compiler suite reports 1,063 passes, and the runtime suite reports 418 passes.
The codegen library reports 264 passes and two existing Red failures.
The failures cover the retained class task in `a341` and the missing collection traps in `t97` and `t98`.
The existing interpreter collection tests retain their results.
The tests cover rooted locals, dropped references, FixedArray references, and aggregate string references.
The build with all workspace targets passes.
Rustfmt passes, and each changed Rust file has at most 2,000 lines.
Clippy exits 0 with no new warning.
No existing `.expected` file changes.
Collection cost and async-cost ratios remain unmeasured.

### Changed files

- `codegen/src/cemit/access.rs`
- `codegen/src/cemit/collection.rs`
- `codegen/src/counted.rs`
- `codegen/src/interpreter.rs`
- `codegen/src/interpreter/collections.rs`
- `codegen/src/interpreter/counted.rs`
- `codegen/src/interpreter/instruction.rs`
- `codegen/src/interpreter/memory.rs`
- `codegen/src/interpreter/reference_holder_tests.rs`
- `codegen/src/jit/symbols.rs`
- `codegen/src/lir.rs`
- `codegen/src/lir/array_ownership.rs`
- `codegen/src/lir/builder.rs`
- `codegen/src/lir/call.rs`
- `codegen/src/lir/lowering.rs`
- `codegen/src/lir/reference_description_tests.rs`
- `codegen/src/lir/verify.rs`
- `codegen/src/lir/verify_counted_operations.rs`
- `codegen/src/lower/func/expr.rs`
- `codegen/src/lower/func/intrinsic.rs`
- `codegen/src/lower/mod.rs`
- `codegen/tests/cemit.rs`
- `codegen/tests/lir-goldens/corpus.txt`
- `codegen/tests/support/lir_facts_lifetime.rs`
- `compiler/src/check/expr/method.rs`
- `compiler/src/check/rejection_witness_index.rs`
- `compiler/src/check/rejection_witnesses.txt`
- `compiler/src/hir/sites.rs`
- `compiler/src/hir/tests.rs`
- `compiler/src/lir.rs`
- `compiler/src/lir_text.rs`
- `compiler/tests/corpus_reject.rs`
- `corpus/accept/a341-reference-holder-release.expected`
- `corpus/accept/a341-reference-holder-release.ts`
- `corpus/reject/r388-counted-map-for-each.ts`
- `corpus/trap/t94-counted-map-delete.expected`
- `corpus/trap/t94-counted-map-delete.ts`
- `corpus/trap/t95-counted-map-clear.expected`
- `corpus/trap/t95-counted-map-clear.ts`
- `corpus/trap/t96-counted-map-replace.expected`
- `corpus/trap/t96-counted-map-replace.ts`
- `corpus/trap/t97-counted-field-collect.expected`
- `corpus/trap/t97-counted-field-collect.ts`
- `corpus/trap/t98-counted-field-task-order.expected`
- `corpus/trap/t98-counted-field-task-order.ts`
- `generated-docs/corpus-index.md`
- `runtime/src/assoc_copy.rs`
- `runtime/src/assoc_counted.rs`
- `runtime/src/assocops.rs`
- `runtime/src/context.rs`
- `runtime/src/context/counted.rs`
- `runtime/src/context/lifecycle.rs`
- `runtime/src/context/memory.rs`
- `runtime/src/ffi/associations.rs`
- `runtime/src/ffi/memory.rs`
- `runtime/tests/counted_map.rs`
- `runtime/tests/counted_object.rs`
- `specs/tracking/s172-reference-holders.md`


## 9. Sweep release, task order, and interpreter roots

The collection separates mark, release, and sweep.
It takes every unreachable class and Map description before it frees any holder storage.
The tier sorts all handle leaves by task ID and releases every leaf after the first exception.
The native consumer uses the Context registry.
The interpreter consumer uses its coroutine registry.
The collector then frees array storage and sweeps the heap.

`Context::with_collection` supplies the release consumer and completes the sweep before it returns.
Direct runtime tests check both APIs with foreign class and Map descriptions.
`Context::describe_map` stores the interpreter's resolved Map value description.
Native Map creation and copies register their existing static descriptions.
The host collection API does not change.

A script collection reports the first unobserved exception at the collect call.
The JIT and C emitter call `subscript_rt_collect_at` with that source position.
The compiler marks collection as a possible trap.
A host collection retains the first exception in the Context trap state.
Every consumer completes the remaining releases before it reports that exception.

A last release preserves an unfinished frame with zero owners.
The registered frame remains a collection root until completion.
The completion release then retires it.
Runtime tests check both allocators and a second-owner control.
The existing counter, ID reuse, and completion tests now complete the frame before they expect retirement.

The interpreter uses the native managed-value root storage plan.
It roots live globals, frame values, local slots, closure environments, registered tasks, scheduler queues, aggregates, and task groups.
Suspension replaces the old activation roots with successor roots.
Teardown clears these roots to break coroutine cycles.
Allocation alone no longer creates a permanent root.
Manual LIR tests supply live-in sets and value origins before they test execution errors.

Array callback arguments borrow their values for the synchronous call.
Callback output arrays, sort scratch values, and closure environments retain temporary roots for their storage lifetimes.
These paths carry no counted callback value.
The existing S014 rejection under §171 rule 5a remains in force.

The collection controls cover H/A/N/F/R in generic classes and Maps, concrete classes, inferred fields, and required/default descriptor fields.
They also cover reference holders through array elements, Map values and keys, Set entries, and outer class fields.
Empty holders supply the same-shape controls.
A separate control compares explicit free with unreachable-holder collection while work remains unfinished.
Mixed Map/class tests reverse holder allocation order and require release by task ID.
Failed-task and fulfilled-task controls check first-error selection, complete release, and no second release.
Map free retires its header even if a value release reports an exception.

An interpreter sort control replaces the receiver values before a comparator calls collection.
The interpreter retains every scratch value and prints `1,2,3`.
The JIT reports UseAfterDelete in this contrived receiver-mutation shape.
This existing native scratch-root gap remains MINOR and does not block the reference-holder changes.

### Verification

The workspace all-target build passes.
The compiler, codegen, and runtime package suites pass.
The codegen package suite includes the existing interpreter collection tests.
The nullable boundary-copy collection test keeps its expected output.
The started-handle collection test retains the completed aggregate result and keeps its expected output.
The full interpreter corpus retains its existing collection goldens.
The fixed-array, aggregate-string, iterator-result, and callee-root native tests also keep their expected outputs.
Clippy reports no new warning against the round 3 warning list.
The format check, diff whitespace check, and repository hygiene scan pass.
No existing `.expected` file changes.
Every changed Rust file remains below 2,000 lines.
The unchanged `runtime/src/arrops.rs` has 2,927 lines at HEAD and on the tree.

### Collection and async cost

The comparison uses the amended contract pin `0f6ba597` and the final tree.
Both binaries use the release profile on the same AArch64 macOS host.
Each comparison uses the best median of three separate runs.
No other build or test runs during a timed measurement.
Allocation, compilation, and Context destruction stay outside the collection timer.
The native shapes use eleven samples per run.
The script shapes use seven fresh executions per run.
The async driver uses its standard eleven timed samples and 200 ms warm-up floor.

The first comparison exceeded 1.05 in async-cost and some collection shapes.
The final tree keeps reference-holder traversal outside ordinary deletion.
It removes redundant description scans after the release plan removes every unmarked description.
The arena sweep publishes its free-list head and byte subtraction once per chunk.
An empty native heap skips mark and release-plan setup but retains regex cleanup.
An empty interpreter heap delegates to that native path.
The runtime suite checks these paths.

The table gives microseconds and tree/pin ratios.
Diagnostics stay off in the native shapes.
Each shape uses zero-filled 32-byte objects with class ID 900.

| Mode | Objects | Reachable | Pin | Tree | Ratio |
|---|---:|---|---:|---:|---:|
| Dev | 0 | No | 0.125 | 0.125 | 1.000 |
| Dev | 0 | Yes | 0.125 | 0.125 | 1.000 |
| Dev | 1,000 | No | 13.875 | 14.292 | 1.030 |
| Dev | 1,000 | Yes | 65.250 | 66.375 | 1.017 |
| Dev | 10,000 | No | 137.917 | 139.542 | 1.012 |
| Dev | 10,000 | Yes | 691.083 | 695.750 | 1.007 |
| Dev | 100,000 | No | 1395.750 | 1400.167 | 1.003 |
| Dev | 100,000 | Yes | 7484.209 | 7457.000 | 0.996 |
| Ship | 0 | No | 0.125 | 0.084 | 0.672 |
| Ship | 0 | Yes | 0.125 | 0.125 | 1.000 |
| Ship | 1,000 | No | 0.917 | 0.625 | 0.682 |
| Ship | 1,000 | Yes | 32.542 | 33.583 | 1.032 |
| Ship | 10,000 | No | 10.708 | 8.541 | 0.798 |
| Ship | 10,000 | Yes | 429.542 | 435.000 | 1.013 |
| Ship | 100,000 | No | 106.000 | 84.208 | 0.794 |
| Ship | 100,000 | Yes | 6293.584 | 6269.833 | 0.996 |

The script shape uses the item 5 async entry and `Cell { v: i32 }` allocations.
Collection starts after that entry finishes.
The JIT calls a collection export; C and the interpreter use the host collection path.
The empty script supplies the zero-task control.
The C timer reports zero for that sub-microsecond control, so no ratio applies.

| Script objects | Tier | Pin microseconds | Tree microseconds | Ratio |
|---:|---|---:|---:|---:|
| 0 | JIT | 0.458 | 0.333 | 0.727 |
| 0 | C AOT | 0.000 | 0.000 | Below timer resolution |
| 0 | Interpreter | 0.333 | 0.333 | 1.000 |
| 1,000 | JIT | 14.375 | 14.917 | 1.038 |
| 1,000 | C AOT | 1.000 | 1.000 | 1.000 |
| 1,000 | Interpreter | 19.000 | 15.250 | 0.803 |
| 10,000 | JIT | 136.375 | 136.125 | 0.998 |
| 10,000 | C AOT | 9.000 | 6.000 | 0.667 |
| 10,000 | Interpreter | 182.125 | 139.000 | 0.763 |

The allocation control requires `0 / 0`, `1,000 / 0`, and `10,000 / 0` live allocations before/after collection in all tiers.
It retains zero tasks.
The interpreter timing probe also reports those counts.
The pin interpreter retains all `1,000` and `10,000` Cells after collection.
The final interpreter retains none.

| Async workload | Pin milliseconds | Tree milliseconds | Ratio |
|---|---:|---:|---:|
| settled-awaits | 14.978 | 15.013 | 1.002 |
| held-handles | 4.298 | 4.331 | 1.008 |
| deep-chains | 10.244 | 10.369 | 1.012 |

Every collection and async shape with a resolved ratio meets the 1.05 limit.
The largest ratio is 1.0377.
Async output stays stable, and every workload reports zero unfinished tasks.

### Current status

| Handoff step | State |
|---|---|
| 1. Red corpus, TypeScript evidence, pin results, and registrations | Complete in sections 6 and 7 |
| 2. Map ownership and callback rejection | Complete in section 7 |
| 3. Class descriptions and release at free and sweep | Complete |
| 4. Ordered collection and script/host trap reporting | Complete |
| 5. Unfinished frame roots and release at completion | Complete |
| 6. Interpreter roots and allocation-count agreement | Complete |
| 7. Unit controls and release-order coverage | Complete |
| 8. Collection and async-cost comparisons | Complete; every resolved ratio is at most 1.05 |

### Open findings

The contrived native sort receiver-mutation case above remains MINOR.

### Cumulative changed files

- `codegen/src/cemit/access.rs`
- `codegen/src/cemit/collection.rs`
- `codegen/src/cemit/intrinsic.rs`
- `codegen/src/counted.rs`
- `codegen/src/interpreter.rs`
- `codegen/src/interpreter/async_all.rs`
- `codegen/src/interpreter/collection_tests.rs`
- `codegen/src/interpreter/collections.rs`
- `codegen/src/interpreter/counted.rs`
- `codegen/src/interpreter/dispatch.rs`
- `codegen/src/interpreter/instruction.rs`
- `codegen/src/interpreter/intrinsics.rs`
- `codegen/src/interpreter/memory.rs`
- `codegen/src/interpreter/operations.rs`
- `codegen/src/interpreter/reference_holder_tests.rs`
- `codegen/src/interpreter/roots.rs`
- `codegen/src/interpreter/task_group.rs`
- `codegen/src/interpreter/tests.rs`
- `codegen/src/jit/symbols.rs`
- `codegen/src/layout.rs`
- `codegen/src/lir.rs`
- `codegen/src/lir/array_ownership.rs`
- `codegen/src/lir/builder.rs`
- `codegen/src/lir/call.rs`
- `codegen/src/lir/lowering.rs`
- `codegen/src/lir/reference_description_tests.rs`
- `codegen/src/lir/verify.rs`
- `codegen/src/lir/verify_counted_operations.rs`
- `codegen/src/lower/func/expr.rs`
- `codegen/src/lower/func/intrinsic.rs`
- `codegen/src/lower/mod.rs`
- `codegen/tests/cemit.rs`
- `codegen/tests/lir-goldens/corpus.txt`
- `codegen/tests/lir/verifier.rs`
- `codegen/tests/support/lir_facts_lifetime.rs`
- `compiler/src/check/expr/method.rs`
- `compiler/src/check/rejection_sites.rs`
- `compiler/src/check/rejection_surface_classes.rs`
- `compiler/src/check/rejection_witness_index.rs`
- `compiler/src/check/rejection_witnesses.txt`
- `compiler/src/hir/intrinsics.rs`
- `compiler/src/hir/sites.rs`
- `compiler/src/hir/tests.rs`
- `compiler/src/lir.rs`
- `compiler/src/lir_text.rs`
- `compiler/tests/corpus_reject.rs`
- `corpus/accept/a341-reference-holder-release.expected`
- `corpus/accept/a341-reference-holder-release.ts`
- `corpus/reject/r388-counted-map-for-each.ts`
- `corpus/trap/t94-counted-map-delete.expected`
- `corpus/trap/t94-counted-map-delete.ts`
- `corpus/trap/t95-counted-map-clear.expected`
- `corpus/trap/t95-counted-map-clear.ts`
- `corpus/trap/t96-counted-map-replace.expected`
- `corpus/trap/t96-counted-map-replace.ts`
- `corpus/trap/t97-counted-field-collect.expected`
- `corpus/trap/t97-counted-field-collect.ts`
- `corpus/trap/t98-counted-field-task-order.expected`
- `corpus/trap/t98-counted-field-task-order.ts`
- `generated-docs/corpus-index.md`
- `runtime/src/assoc_copy.rs`
- `runtime/src/assoc_counted.rs`
- `runtime/src/assocops.rs`
- `runtime/src/context.rs`
- `runtime/src/context/async_scheduler.rs`
- `runtime/src/context/counted.rs`
- `runtime/src/context/lifecycle.rs`
- `runtime/src/context/memory.rs`
- `runtime/src/context/tests_0.rs`
- `runtime/src/ffi/associations.rs`
- `runtime/src/ffi/memory.rs`
- `runtime/tests/async_inspection.rs`
- `runtime/tests/counted_collection.rs`
- `runtime/tests/counted_completion_form.rs`
- `runtime/tests/counted_map.rs`
- `runtime/tests/counted_object.rs`
- `specs/tracking/s172-reference-holders.md`

## 10. Reachable generators and release-description checks

The dropped-generator probe retained two allocations in the interpreter and zero in JIT and C before the correction.
The corrected probe retains zero allocations in all three tiers after a host collection.
The module-global iterator control retains two allocations in the interpreter and three in each native tier.
The native tiers also allocate the generator frame in the Context.
The interpreter stores that frame in its Rust registry, as before.
Reachable iterators also resume after collection through class fields and array elements.
The interpreter traverses reachable heap words to find packed generator keys.
It removes every unreachable generator registration at collection.
An empty heap still removes unreachable generator registrations.
The runtime traversal includes registered host root ranges and skips scalar string payloads.

The class release record carries byte offsets and count actions.
The builder takes its offsets from the HIR class layout.
The verifier compares records against offsets and types from a separate LIR class layout.
Hand-built forms with an incorrect offset or an omitted counted field fail.
A complete description passes.
The generated LIR golden changes two field-id records to byte-offset records.
The verifier requires a Call trap on Collect.
The hand-built missing-trap form fails, and its guarded control passes.
The interpreter removes the unused scalar Collect dispatch arm.

The collection API uses a closure instead of a public release plan.
Its guard completes the sweep on return, early error, and unwind.
The foreign-registry test still releases leaves before sweep.
The early-error and unwind controls leave no stale mark on the next collection.

The object-description FFI rejects null pointers before it constructs a slice.
It rejects sizes above the representable slice limit.
The object release decoder returns an error for malformed field or node sizes.
It validates the entire description before it releases any field.
The malformed-size and null-pointer tests pass.

The t97 purpose states that one failed field traps at the collect position.
The Map callback check uses MapCallbackCountedValue.
The §154 witness index assigns the existing counted-Map callback witness to that site.
The diagnostic code and text do not change.
The corpus index comes from the document generator.

### Interpreter corpus cost

The debug corpus runs alone at 9647c72a and on the tree.
The comparison uses three separate runs on the same AArch64 macOS host.
The test reports elapsed execution time; compilation stays outside each measurement.
The pin runs 264 entries; the tree also runs a341, for 265 entries.
Both runs exclude the declared benchmark and retain 64 interpreter exclusions.
All selected entries match their goldens.

The interpreter builds Layouts once per module.
Class and Map allocations reuse that layout.
The round 5 tree supplies a root snapshot at each instruction.
The round 5 corpus ratio includes the interpreter snapshots and the other lowering changes.
It does not isolate the snapshot cost from the other changes.

| Tree | Entries | Run 1 seconds | Run 2 seconds | Run 3 seconds | Median seconds |
|---|---:|---:|---:|---:|---:|
| 9647c72a | 264 | 8.983 | 9.054 | 9.077 | 9.054 |
| Round 5 tree | 265 | 15.437 | 15.593 | 15.580 | 15.580 |

The median tree/pin ratio is 1.721.
The interpreter corpus costs 72.1% more than the pin.

### Verification

The workspace build passes with all targets.
The compiler package reports 1,063 passes.
The runtime package reports 426 passes.
The complete codegen package suite passes.
The nine collection tests and four release-description tests pass.
The format check and diff whitespace check pass.
Clippy exits zero and adds no warning against the round 4 warning list.
Every changed Rust file has at most 1,971 lines.
No existing corpus .expected file changes.
The repository hygiene check passes.

## 11. Collection-time interpreter roots

The interpreter no longer creates a root snapshot at each instruction.
Each active frame carries its current LIR instruction coordinate.
A collection reads active frames and derives live origins from the shared LIR interference relation.
The frame walk reads live temporaries, valid locals, resume values, and delivered exceptions.
The collection also reads globals, coroutine registries, scheduler queues, task groups, and host root ranges.
Reachable heap iterator keys retain generator frames; unreachable keys leave the generator registry.
The active-frame registration covers each frame execution, including nested Rust calls into script functions.
Callable argument registrations cover each script callback.
Array callback registrations cover the receiver, operands, result arrays, and sort scratch for the callback operation.
These registrations end when the corresponding call returns, including a returned error.

The new callback test uses the same array-map shape with collection enabled and disabled.
It checks the accumulated result array and a caller local across a nested script call.
Both controls print `1,2,3,4`.
All ten collection tests pass.
All 47 interpreter tests pass.
The format, diff whitespace, and repository hygiene checks pass.
Every changed Rust file stays below 2,000 lines.

### Interpreter corpus cost

Both debug binaries run alone, with compilation outside the measured interval.
The pin source matches `9647c72a`.
The pin runs 264 entries; the tree runs 265 entries, including a341.
Both selections exclude the benchmark and retain 64 declared interpreter exclusions.
Every selected output matches its golden.

| Tree | Run 1 seconds | Run 2 seconds | Run 3 seconds | Median seconds |
|---|---:|---:|---:|---:|
| 9647c72a | 9.045 | 8.964 | 9.033 | 9.033 |
| Round 6 tree | 9.240 | 9.919 | 17.081 | 9.919 |

The tree/pin median ratio is 1.0981.
The best tree time is 9.240 seconds; the best pin time is 8.964 seconds.
Their ratio is 1.0308. The amended acceptance 3 uses the best of three runs, so this result passes.
The tree samples show substantial variation; the measurements do not identify its external cause.

### Time attribution

A separate diagnostic run records elapsed time for each entry's lowering and interpretation.
Both diagnostic binaries run alone and retain the normal corpus worker pool.
The temporary timing instrumentation does not remain in the source tree.
Worker totals overlap and therefore do not equal corpus wall time.

| Diagnostic measure | Pin seconds | Tree seconds |
|---|---:|---:|
| Process wall time | 9.859 | 9.770 |
| Process user CPU time | 12.061 | 12.798 |
| Process system CPU time | 0.153 | 0.163 |
| Sum of entry lowering intervals | 2.573282 | 3.160941 |
| Sum of entry interpretation intervals | 10.426869 | 10.668008 |
| a24-particle-system interpretation | 8.823697 | 9.008060 |
| a204-static-long-string interpretation | 0.891373 | 0.926599 |
| a23-game-loop interpretation | 0.529720 | 0.558183 |

Particle interpretation accounts for 84.4% of the tree's summed interpretation intervals.
That program performs numeric array and field loops and never calls collection.
Collection-time root traversal therefore cannot account for its interpretation cost.
The diagnostic run locates the dominant workload but does not isolate a specific instruction cost.
Its tree/pin wall ratio differs from the best-of-three ratio.
The diagnostic run does not replace the acceptance measurement.
The round 4 async and collection cost tables remain historical evidence; round 6 supplies no new values.
The round 6 evidence does not include the full workspace build, package suites, or Clippy results.

### Changed files

The following list includes the preserved implementation and the round 6 changes.

- `codegen/src/cemit/access.rs`
- `codegen/src/cemit/collection.rs`
- `codegen/src/cemit/intrinsic.rs`
- `codegen/src/counted.rs`
- `codegen/src/interpreter.rs`
- `codegen/src/interpreter/async_all.rs`
- `codegen/src/interpreter/collection_tests.rs`
- `codegen/src/interpreter/collections.rs`
- `codegen/src/interpreter/counted.rs`
- `codegen/src/interpreter/dispatch.rs`
- `codegen/src/interpreter/instruction.rs`
- `codegen/src/interpreter/intrinsics.rs`
- `codegen/src/interpreter/memory.rs`
- `codegen/src/interpreter/operations.rs`
- `codegen/src/interpreter/reference_holder_tests.rs`
- `codegen/src/interpreter/roots.rs`
- `codegen/src/interpreter/task_group.rs`
- `codegen/src/interpreter/tests.rs`
- `codegen/src/jit/symbols.rs`
- `codegen/src/layout.rs`
- `codegen/src/lir.rs`
- `codegen/src/lir/array_ownership.rs`
- `codegen/src/lir/builder.rs`
- `codegen/src/lir/call.rs`
- `codegen/src/lir/lowering.rs`
- `codegen/src/lir/reference_description_tests.rs`
- `codegen/src/lir/verify.rs`
- `codegen/src/lir/verify_counted_operations.rs`
- `codegen/src/lower/func/expr.rs`
- `codegen/src/lower/func/intrinsic.rs`
- `codegen/src/lower/mod.rs`
- `codegen/src/root_storage.rs`
- `codegen/tests/cemit.rs`
- `codegen/tests/lir-goldens/corpus.txt`
- `codegen/tests/lir/verifier.rs`
- `codegen/tests/support/lir_facts_lifetime.rs`
- `compiler/src/check/expr/method.rs`
- `compiler/src/check/rejection_sites.rs`
- `compiler/src/check/rejection_surface_classes.rs`
- `compiler/src/check/rejection_witness_index.rs`
- `compiler/src/check/rejection_witnesses.txt`
- `compiler/src/hir/intrinsics.rs`
- `compiler/src/hir/sites.rs`
- `compiler/src/hir/tests.rs`
- `compiler/src/lir.rs`
- `compiler/src/lir_text.rs`
- `compiler/tests/corpus_reject.rs`
- `corpus/accept/a341-reference-holder-release.expected`
- `corpus/accept/a341-reference-holder-release.ts`
- `corpus/reject/r388-counted-map-for-each.ts`
- `corpus/trap/t94-counted-map-delete.expected`
- `corpus/trap/t94-counted-map-delete.ts`
- `corpus/trap/t95-counted-map-clear.expected`
- `corpus/trap/t95-counted-map-clear.ts`
- `corpus/trap/t96-counted-map-replace.expected`
- `corpus/trap/t96-counted-map-replace.ts`
- `corpus/trap/t97-counted-field-collect.expected`
- `corpus/trap/t97-counted-field-collect.ts`
- `corpus/trap/t98-counted-field-task-order.expected`
- `corpus/trap/t98-counted-field-task-order.ts`
- `generated-docs/corpus-index.md`
- `runtime/src/assoc_copy.rs`
- `runtime/src/assoc_counted.rs`
- `runtime/src/assocops.rs`
- `runtime/src/context.rs`
- `runtime/src/context/async_scheduler.rs`
- `runtime/src/context/counted.rs`
- `runtime/src/context/lifecycle.rs`
- `runtime/src/context/memory.rs`
- `runtime/src/context/tests_0.rs`
- `runtime/src/ffi/associations.rs`
- `runtime/src/ffi/memory.rs`
- `runtime/tests/async_inspection.rs`
- `runtime/tests/counted_collection.rs`
- `runtime/tests/counted_completion_form.rs`
- `runtime/tests/counted_map.rs`
- `runtime/tests/counted_object.rs`
- `specs/tracking/s172-reference-holders.md`


## 12. Instruction entry roots and cached liveness

An executing instruction does not root its result slot.
The frame walk uses a strict interval start and an inclusive last-use point.
Suspended block entry uses point zero; block parameters remain roots there.
Each active caller keeps its instruction entry coordinate during a nested call.
The same rule applies to an unfinished generator `next` result.

Frame execution and collection use one registered raw pointer per frame.
Operand and local references end before a call can collect.
The result write starts after the call returns.
The frame registration ends on success or error.
No mutable frame reference spans a collection read.

Each interpreter caches the interference relation by function ID.
A function builds its relation on its first collection walk.
Later collections and generator closure passes reuse that relation.
No root snapshot or interference build runs per instruction.

### Corpus and controls

`t99-loop-result-collect` requires the second collect to release the previous loop result.
It reports `uncaught-exception`, `Error: lost`, at line 13, column 3, after `step 0`.
This follows §172 rule 7: the unfinished call result cannot retain the previous iteration's Holder.
The corpus test registers its trap kind, position, message, and stdout for both native tiers.
The debug interpreter trap subset includes this entry.
TypeScript 5.9.2 accepts the entry with exit zero.
The generated corpus index includes the entry.
The LIR text record covers accepted entries, so this trap adds no text-record row.

The pin binary uses production sources identical to `04963f81`; only the contract differs from `0f6ba597`.
All three pin tiers print `step 0` and `step 1` without a trap.
The pre-fix interpreter unit test also prints both lines and fails its expected-trap assertion.
After the fix, all three tiers report the required t99 trap and stdout.
The no-collect unit control prints both lines without a trap.
A generator loop tests the unfinished IterResult slot with the same collect and no-collect controls.

The heap-generator test covers a global, a class field, and an array element.
Each reachable generator resumes and prints `2` after collection.
Each removal control replaces that same storage position before collection and prints `removed`.
The registry keeps three current generators and removes exactly one original generator in each removal control.
The interval unit test covers definition exclusion, last-use retention, dead-value exclusion, and suspended live-ins.
All twelve collection tests pass.

### Collection and async cost

The pin is `0f6ba597`, whose production sources also match `04963f81`.
The binaries use the release profile on the same AArch64 macOS host.
Each cell reports the best median of three separate runs.
Every timed process runs alone, with no other build or test.
The native driver uses the same dependency configuration in both trees.
Native runs use eleven samples; script runs use seven fresh executions.
Allocation, compilation, and Context destruction stay outside the collection timer.
The async driver uses eleven samples and its 200 ms warm-up floor.
The temporary script timing module does not remain in the tree.

The native table gives microseconds and tree/pin ratios.
Objects have zero-filled 32-byte payloads and class ID 900.
Diagnostics remain off.

| Mode | Objects | Reachable | Pin | Tree | Ratio |
|---|---:|---|---:|---:|---:|
| Dev | 0 | No | 0.166 | 0.125 | 0.753 |
| Dev | 0 | Yes | 0.125 | 0.083 | 0.664 |
| Dev | 1,000 | No | 14.292 | 13.709 | 0.959 |
| Dev | 1,000 | Yes | 66.417 | 64.500 | 0.971 |
| Dev | 10,000 | No | 139.750 | 135.208 | 0.967 |
| Dev | 10,000 | Yes | 692.333 | 674.709 | 0.975 |
| Dev | 100,000 | No | 1376.792 | 1372.125 | 0.997 |
| Dev | 100,000 | Yes | 7450.709 | 7496.208 | 1.006 |
| Ship | 0 | No | 0.125 | 0.084 | 0.672 |
| Ship | 0 | Yes | 0.125 | 0.084 | 0.672 |
| Ship | 1,000 | No | 0.875 | 0.625 | 0.714 |
| Ship | 1,000 | Yes | 32.541 | 33.959 | 1.044 |
| Ship | 10,000 | No | 11.000 | 8.542 | 0.777 |
| Ship | 10,000 | Yes | 432.625 | 421.584 | 0.974 |
| Ship | 100,000 | No | 106.458 | 84.833 | 0.797 |
| Ship | 100,000 | Yes | 6160.292 | 6221.959 | 1.010 |

The script allocates Cells in an async entry and collects after the entry finishes.
JIT uses a collection export; C and the interpreter use host collection.

| Script objects | Tier | Pin microseconds | Tree microseconds | Ratio |
|---:|---|---:|---:|---:|
| 0 | JIT | 0.500 | 0.333 | 0.666 |
| 0 | C AOT | 0.000 | 0.000 | Below timer resolution |
| 0 | Interpreter | 0.333 | 0.333 | 1.000 |
| 1,000 | JIT | 15.000 | 14.542 | 0.969 |
| 1,000 | C AOT | 1.000 | 1.000 | 1.000 |
| 1,000 | Interpreter | 18.958 | 15.500 | 0.818 |
| 10,000 | JIT | 134.875 | 136.292 | 1.011 |
| 10,000 | C AOT | 8.000 | 6.000 | 0.750 |
| 10,000 | Interpreter | 184.417 | 140.000 | 0.759 |

The allocation-count unit controls require `0 / 0`, `1,000 / 0`, and `10,000 / 0` in all three tiers.
The interpreter timing probe also reports those before/after counts.
The interpreter and JIT timing probes assert zero tasks.
The pin interpreter retains its Cell allocations, as the earlier table records.

| Async workload | Pin milliseconds | Tree milliseconds | Ratio |
|---|---:|---:|---:|
| settled-awaits | 14.929 | 14.842 | 0.994 |
| held-handles | 4.329 | 4.334 | 1.001 |
| deep-chains | 10.583 | 10.591 | 1.001 |

Every resolved collection and async ratio meets the 1.05 limit. The largest ratio is 1.0436.
Async output remains stable, and each workload reports zero unfinished tasks.

### Interpreter corpus cost

The debug binary runs alone, with all builds outside the measured interval.
It excludes the benchmark and keeps the 64 declared interpreter exclusions.
Each run checks 265 outputs against their goldens.

| Tree | Run 1 seconds | Run 2 seconds | Run 3 seconds | Best seconds |
|---|---:|---:|---:|---:|
| 9647c72a, section 11 | 9.045 | 8.964 | 9.033 | 8.964 |
| Current tree | 9.294 | 9.228 | 9.269 | 9.228 |

The best-of-three ratio is 1.0295. This passes amended acceptance 3.

### Acceptance evidence

| Acceptance item | Evidence |
|---|---|
| 1. Corpus and rejection | a341, t94–t99, and r388 pass their registered checks |
| 2. Unit controls | Twelve collection tests, including caller, IterResult, and heap-generator removal controls, pass |
| 3. Cost | Interpreter corpus, collection, and async-cost ratios are at most 1.05 |
| 4. Goldens | Existing outputs remain unchanged; t99 adds its required stdout golden |

### Verification

`cargo test --offline --locked -p subscript-compiler`: 1063 passed, 1 ignored, zero failures.
`cargo test --offline --locked -p subscript-codegen`: 812 passed, 1 ignored, zero failures.
`cargo test --offline --locked -p subscript-runtime`: 426 passed, 1 ignored, zero failures.

`cargo build --offline --locked --workspace --all-targets`: passes.
`cargo fmt --check`: passes.
`cargo clippy --offline --locked --workspace --all-targets`: passes with no new warning.
Warning counts and affected targets match the previous full workspace Clippy run.
`tools/hygiene.sh`: passes.
All changed Rust files have at most 1,971 lines.
The generated documentation comes only from `generate-api-reference`.

### Files changed in this round

- `codegen/src/interpreter.rs`
- `codegen/src/interpreter/collection_tests.rs`
- `codegen/src/interpreter/instruction.rs`
- `codegen/src/interpreter/roots.rs`
- `codegen/src/root_storage.rs`
- `codegen/tests/cemit.rs`
- `codegen/tests/lir.rs`
- `corpus/trap/t99-loop-result-collect.expected`
- `corpus/trap/t99-loop-result-collect.ts`
- `generated-docs/corpus-index.md`
- `specs/tracking/s172-reference-holders.md`

### Complete changed-file inventory

This inventory includes the preserved implementation and rounds 7 and 8.

- `codegen/src/cemit/access.rs`
- `codegen/src/cemit/collection.rs`
- `codegen/src/cemit/intrinsic.rs`
- `codegen/src/cemit/suspend.rs`
- `codegen/src/counted.rs`
- `codegen/src/interpreter.rs`
- `codegen/src/interpreter/async_all.rs`
- `codegen/src/interpreter/collections.rs`
- `codegen/src/interpreter/counted.rs`
- `codegen/src/interpreter/instruction.rs`
- `codegen/src/interpreter/intrinsics.rs`
- `codegen/src/interpreter/memory.rs`
- `codegen/src/interpreter/operations.rs`
- `codegen/src/interpreter/task_group.rs`
- `codegen/src/interpreter/tests.rs`
- `codegen/src/jit/symbols.rs`
- `codegen/src/layout.rs`
- `codegen/src/lir.rs`
- `codegen/src/lir/array_ownership.rs`
- `codegen/src/lir/builder.rs`
- `codegen/src/lir/call.rs`
- `codegen/src/lir/lowering.rs`
- `codegen/src/lir/verify.rs`
- `codegen/src/lir/verify_counted_operations.rs`
- `codegen/src/lower/func/coroutine.rs`
- `codegen/src/lower/func/expr.rs`
- `codegen/src/lower/func/intrinsic.rs`
- `codegen/src/lower/mod.rs`
- `codegen/src/root_storage.rs`
- `codegen/tests/cemit.rs`
- `codegen/tests/lir-goldens/corpus.txt`
- `codegen/tests/lir.rs`
- `codegen/tests/lir/verifier.rs`
- `codegen/tests/support/lir_facts_lifetime.rs`
- `compiler/src/check/expr/method.rs`
- `compiler/src/check/rejection_sites.rs`
- `compiler/src/check/rejection_surface_classes.rs`
- `compiler/src/check/rejection_witness_index.rs`
- `compiler/src/check/rejection_witnesses.txt`
- `compiler/src/hir/intrinsics.rs`
- `compiler/src/hir/sites.rs`
- `compiler/src/hir/tests.rs`
- `compiler/src/lir.rs`
- `compiler/src/lir_text.rs`
- `compiler/tests/corpus_reject.rs`
- `generated-docs/corpus-index.md`
- `runtime/src/assoc_copy.rs`
- `runtime/src/assocops.rs`
- `runtime/src/context.rs`
- `runtime/src/context/async_scheduler.rs`
- `runtime/src/context/counted.rs`
- `runtime/src/context/lifecycle.rs`
- `runtime/src/context/memory.rs`
- `runtime/src/context/tests_0.rs`
- `runtime/src/ffi/associations.rs`
- `runtime/src/ffi/memory.rs`
- `runtime/tests/async_inspection.rs`
- `runtime/tests/counted_completion_form.rs`
- `specs/tracking/s172-reference-holders.md`
- `codegen/src/interpreter/collection_tests.rs`
- `codegen/src/interpreter/dispatch.rs`
- `codegen/src/interpreter/reference_holder_tests.rs`
- `codegen/src/interpreter/roots.rs`
- `codegen/src/lir/reference_description_tests.rs`
- `codegen/tests/interpreter_roots.rs`
- `corpus/accept/a341-reference-holder-release.expected`
- `corpus/accept/a341-reference-holder-release.ts`
- `corpus/reject/r388-counted-map-for-each.ts`
- `corpus/trap/t100-conditional-await-collect.expected`
- `corpus/trap/t100-conditional-await-collect.ts`
- `corpus/trap/t94-counted-map-delete.expected`
- `corpus/trap/t94-counted-map-delete.ts`
- `corpus/trap/t95-counted-map-clear.expected`
- `corpus/trap/t95-counted-map-clear.ts`
- `corpus/trap/t96-counted-map-replace.expected`
- `corpus/trap/t96-counted-map-replace.ts`
- `corpus/trap/t97-counted-field-collect.expected`
- `corpus/trap/t97-counted-field-collect.ts`
- `corpus/trap/t98-counted-field-task-order.expected`
- `corpus/trap/t98-counted-field-task-order.ts`
- `corpus/trap/t99-loop-result-collect.expected`
- `corpus/trap/t99-loop-result-collect.ts`
- `runtime/src/assoc_counted.rs`
- `runtime/tests/counted_collection.rs`
- `runtime/tests/counted_map.rs`
- `runtime/tests/counted_object.rs`

## 13. Per-value roots and suspension storage

### Red measurements

The baseline is `9de0b638` with the preserved round 7 implementation.
Each program runs in the three tiers before the round 8 fix.
The t100 source is the handoff's conditional-await loop.

| Program | Dev JIT | C AOT | Interpreter |
|---|---|---|---|
| t100, empty tick | Prints step 0 and step 1; no trap | Same | Same |
| t100, tick awaits Context.suspend | Prints step 0 and step 1; no trap | Same | Same |
| t100, first holder stays in a global through the collect | Prints step 0, step 1, kept | Same | Same |
| t99, current round 7 implementation | Traps at second collect; prints step 0 | Same | Same |
| t99, kept first result through the second call and collect | Prints step 0, step 1, kept | Same | Same |

The native tiers also keep a stale suspension argument in the coroutine allocation after its restore.
The interpreter uses an origin interval for distinct value slots.
An interpreter-only per-value change makes t100 trap only in the interpreter.
The root defect therefore exists in all three tiers on this baseline.
Rule 7 requires per-value roots; rule 4 and §116 require the last failed holder's release to trap.
The native suspension-slot fix and the interpreter interval fix make t100 trap in all three tiers.

### Local and completed-frame measurements

Each local program uses `FixedArray<Holder, 1>` and reads `h[0].task` before the collect.
The addressed local has `Activation` storage; the across-suspension local has `Frame` storage.
The last read precedes the collect, so the local has no later read.
The finished generator stays in a module global.
The finished async invocation stays in a module-global handle array.

The source programs are in `codegen/tests/interpreter_roots.rs`.

| Program | Dev JIT | C AOT | Interpreter before fix | Interpreter after fix |
|---|---|---|---|---|
| Addressed Activation local in its active invocation | Prints after; no trap | Same | Same | Same |
| Frame local after its last read, before completion | Prints after; no trap | Same | Same | Same |
| Frame local in a held, finished generator | Prints after; no trap | Same | Same | Same |
| Frame local in a held, finished async invocation | Prints after; no trap | Same | Same | Same |
| Activation local declared after the generator's last yield, then completed | Traps at collect; Error: lost | Same | Prints after; no trap | Traps at collect; Error: lost |

The native tiers root these locals for their storage lifetime, rather than only through their last read.
Frame storage remains reachable in a held, finished coroutine.
Activation storage ends when its invocation returns.
The interpreter now excludes Activation locals of a finished coroutine and keeps its Frame locals.

### Implementation and controls

The interpreter derives actual-value live-ins once at collection and caches the resulting value intervals per function.
It never maps a frame slot through `value_origins` for the root test.
The root test retains the existing before-instruction coordinate.
No instruction adds root-analysis work.

Both native lowerings clear each suspension argument after its restore.
The JIT copies aggregate arguments into activation storage before the clear.
The clear covers yield, host suspension, async-call await, and held-handle await resumes.
It does not release a count; the restored value still holds the same ownership.
Frame-class local storage remains intact.

`t100` reports `uncaught-exception` at line 17, column 5, with message `Error: lost` and stdout `step 0`.
The same-shape control keeps the first holder through both collects and prints `step 0`, `step 1`, and `kept`.
Both programs have direct interpreter unit coverage and three-tier integration coverage.

The t99 survival control keeps `const kept = step(0)` through the next loop call and its collect.
It reads `kept.task` after the loop and prints `step 0`, `step 1`, and `kept` in all three tiers.
The t99 header now records the current result: all three tiers trap at the second collect and print only `step 0`.
`t100` passes TypeScript 5.9.2 with exit 0.

### Files changed in round 8

- `codegen/src/cemit/suspend.rs`
- `codegen/src/interpreter/collection_tests.rs`
- `codegen/src/interpreter/roots.rs`
- `codegen/src/lower/func/coroutine.rs`
- `codegen/tests/cemit.rs`
- `codegen/tests/interpreter_roots.rs`
- `codegen/tests/lir.rs`
- `corpus/trap/t100-conditional-await-collect.expected`
- `corpus/trap/t100-conditional-await-collect.ts`
- `corpus/trap/t99-loop-result-collect.ts`
- `generated-docs/corpus-index.md`
- `specs/tracking/s172-reference-holders.md`

### Interpreter corpus cost

The debug test binary runs alone; all compilation stays outside its measured interval.
Each run checks 265 outputs, excludes the benchmark, and retains 64 declared interpreter exclusions.
All three runs match every selected golden.

| Tree | Run 1 seconds | Run 2 seconds | Run 3 seconds | Best seconds |
|---|---:|---:|---:|---:|
| 9647c72a, section 11 | 9.045 | 8.964 | 9.033 | 8.964 |
| Round 8 tree | 9.381 | 9.440 | 9.299 | 9.299 |

The best-of-three ratio is 1.0374. This passes amended acceptance 3's 1.05 limit.

### Verification

`cargo build --offline --locked --workspace --all-targets`: passes.
`cargo test --offline --locked -p subscript-compiler`: 1063 passed, 1 ignored, zero failures.
`cargo test --offline --locked -p subscript-codegen`: 819 passed, 1 ignored, zero failures.
`cargo test --offline --locked -p subscript-runtime`: 426 passed, 1 ignored, zero failures.
`cargo fmt --check`: passes.
`cargo clippy --offline --locked --workspace --all-targets`: passes with no new warning.
The new integration target reports no Clippy warning; the existing target warning counts remain unchanged.
`tools/hygiene.sh`: passes.
`git diff --check`: passes.
All changed Rust files have at most 1,971 lines.
The generated corpus index comes only from `generate-api-reference`.

## 14. One root definition for coroutine storage

### Red measurements

The baseline is `1dcaafb8` with the preserved round 8 implementation.
The unit probe runs each source in all three tiers before the frame-field change.
All failed tasks report `Error: lost`.

| Program | Dev JIT | C AOT | Interpreter |
|---|---|---|---|
| t101, unused async parameter | Prints start and after; no trap | Same | Traps at collect; prints start |
| t102, generator parameter used before its first yield | Prints after; no trap | Same | Prints after; traps at generator exit |
| t103, unstarted generator with an unused parameter | Prints after; no trap | Same | Traps at collect; no stdout |
| t104, closure called before an await | Prints after; no trap | Same | Traps at collect; no stdout |
| Value class with a Holder field | Checker rejects S100 | Same checker rejection | Same checker rejection |

A value class cannot hold a reference-class field.
Its whitelist admits sized numerics, boolean, value classes, FixedArray, and enums.
The direct checker unit test requires the S100 whitelist diagnostic.

The four new trap entries retain their interpreter outputs as the rule 7 expectations.
TypeScript 5.9.2 accepts all four entries with exit zero.
Each entry has a same-shape three-tier control that reads the value after the collect.
The controls print after and report no trap; the t101 control also prints start.
The generator control stores its parameter in a global after the collect.
The unstarted control reads its parameter when the generator first starts after the collect.

### Shared plan and field inventory

The root-storage plan carries separate value and stable-address clear coordinates.
Both native transcribers consume these coordinates beside the shadow-slot clears.
Per-value analysis distinguishes suspend aliases and earlier loop values.
Stable address storage uses the shared origin intervals because its target survives an address alias.
The interpreter uses the same per-value analysis and caches it per function.
The native plan omits the additional analysis when a function has no corresponding frame fields.

The C and JIT frame definitions select the same stable value IDs.
A stable field exists only for an address result that crosses a suspension.
C uses activation storage for the other address results.

| Frame field | Root lifetime and clear point |
|---|---|
| state, reserved, resume | Header metadata; no script reference value |
| parameter p | Stored only when live at initial entry; transferred and cleared on initial resume; later resumes restore suspension fields |
| local l | The LIR LocalStorageClass selects its storage lifetime; Frame locals clear at completion |
| suspension argument b_v | Live across its suspension; copied to activation storage and cleared at restore |
| pending child b_child | Held through the await; cleared after the cached child completion is read |
| closure environment env_v | Cleared at its value interval end, or at a block entry where the preceding path no longer needs it |
| stable value stable_v | Cleared at its address-origin interval end, or at the corresponding block entry |
| completion result | Held by completion storage outside the coroutine payload |

A JIT aggregate parameter receives an activation copy before its parameter field clears.
This prevents the aggregate value from referring to the cleared frame bytes.
A normal return or an unwind clears every payload field.
The header retains the existing scheduler and reload state rules.
A finished interpreter frame contributes no values, locals, resume value, or delivered exception.
Its completion retains its result or exception separately.
Address-taken storage retains the existing §68.2 rule 8b lifetime.
The active addressed-local and Frame-local controls retain their objects.
The held finished async and generator controls now trap at collection in all three tiers.

### Total suspension check

The native unit check covers the accepted and trap corpora.
It checks 318 coroutine functions and 513 suspension points in both native tiers.
The check reads generated C writes and Cranelift stores, rather than a separately maintained clear record.
Cranelift source locations and C point comments exist only under cfg(test).
They identify the LIR point of each actual write.

The check propagates occupied environment and managed stable fields through the control-flow graph.
It compares each suspension's occupied set with the independently derived shared live set.
It also checks fresh parameter roots, parameter transfer, spill restore, child retirement, and stable-field definition parity.
A C source with its environment clears removed fails the check.
A Cranelift form with its zero stores removed also fails the check.
The check performs one native lowering per coroutine corpus entry and executes no script twice.

Captured function values add transitive environment dependencies to the shared intervals.
An outer closure keeps the environments of its captured function values through its last use.
The nested closure trap and its control agree in all three tiers.

A coroutine factory copies a live function parameter environment into its own frame before returning.
Initial dispatch transfers parameters once; later dispatch does not reinitialize saved environments or Frame locals.
The JIT resolves coroutine function values through their own environment fields even when the source block order emits a use before its dominating definition.
C snapshots function edge arguments before assigning each destination environment.
A null environment assignment clears the previous destination capture in both native tiers.
The loop replacement trap and its retained-capture control agree in all three tiers.
The existing suspension, transitive-capture, and closure-reuse goldens stay unchanged.

The field check includes factory-owned parameter environments in its initial occupied set.
Conditional null assignment stores are excluded from unconditional interval-clear evidence.
The final review of the frame-lifetime form reports no CRITICAL and no MAJOR finding; §172.3 items 2 to 5 record its MINOR findings.

The mutable generator parameter control keeps its Holder through the first suspension and collect.
All three tiers print kept and found, then trap after the generator completes and its frame clears.

### Performance

The release async-cost binary runs three times alone, with no other build or test running.
Each workload keeps its three-iteration and 200 ms warm-up floors and eleven timed samples.
C compilation remains outside the measured program execution.
The comparison pin is the section 12 baseline.

| Workload | Run 1 ms | Run 2 ms | Run 3 ms | Best ms | Pin ms | Best / pin |
|---|---:|---:|---:|---:|---:|---:|
| settled-awaits | 15.156 | 15.127 | 15.380 | 15.127 | 14.929 | 1.0133 |
| held-handles | 4.371 | 4.354 | 4.431 | 4.354 | 4.329 | 1.0058 |
| deep-chains | 10.534 | 10.211 | 10.625 | 10.211 | 10.583 | 0.9648 |

Each best-of-three ratio passes the 1.05 limit.

The debug interpreter corpus binary runs three times alone after all builds and tests finish.
Each run checks 265 outputs, omits the benchmark, and retains 64 declared exclusions.
All selected outputs match their existing goldens.
The table records process wall time, including test startup, with compilation excluded.

| Tree | Run 1 seconds | Run 2 seconds | Run 3 seconds | Best seconds |
|---|---:|---:|---:|---:|
| 9647c72a, section 11 | 9.045 | 8.964 | 9.033 | 8.964 |
| Round 9 tree | 9.925 | 9.397 | 9.198 | 9.198 |

The best-of-three ratio is 1.0261 and passes the 1.05 limit.

### Verification

Round 9 is complete, with no open phase-review finding.
`cargo build --offline --locked --workspace --all-targets`: passes.
`cargo test --offline --locked -p subscript-compiler`: 1063 passed, 1 ignored, zero failures.
`cargo test --offline --locked -p subscript-codegen`: 825 passed, 1 ignored, zero failures.
`cargo test --offline --locked -p subscript-runtime`: 426 passed, 1 ignored, zero failures.
The codegen suite includes ten three-tier root tests and the total native suspension check.
`cargo fmt --check`: passes.
`cargo clippy --offline --locked --workspace --all-targets`: passes with no new warning message or target warning count.
All changed Rust files have at most 1,971 lines.
No existing corpus `.expected` golden changes.
The generated corpus index comes only from `generate-api-reference`.
`tools/hygiene.sh`: passes.
`git diff --check`: passes.
The working tree includes the preserved rounds 7 and 8 changes; it has no new commit.

### Changed files from the contract pin

This inventory covers all 106 modified or new files in the complete working tree against `1dcaafb8`.
It includes the preserved implementation and the round 9 changes.

- `codegen/src/cemit.rs`
- `codegen/src/cemit/access.rs`
- `codegen/src/cemit/arith.rs`
- `codegen/src/cemit/body.rs`
- `codegen/src/cemit/collection.rs`
- `codegen/src/cemit/emitter.rs`
- `codegen/src/cemit/frame_roots.rs`
- `codegen/src/cemit/graph.rs`
- `codegen/src/cemit/intrinsic.rs`
- `codegen/src/cemit/suspend.rs`
- `codegen/src/cemit/terminator.rs`
- `codegen/src/counted.rs`
- `codegen/src/interpreter.rs`
- `codegen/src/interpreter/async_all.rs`
- `codegen/src/interpreter/collection_tests.rs`
- `codegen/src/interpreter/collections.rs`
- `codegen/src/interpreter/counted.rs`
- `codegen/src/interpreter/dispatch.rs`
- `codegen/src/interpreter/instruction.rs`
- `codegen/src/interpreter/intrinsics.rs`
- `codegen/src/interpreter/memory.rs`
- `codegen/src/interpreter/operations.rs`
- `codegen/src/interpreter/reference_holder_tests.rs`
- `codegen/src/interpreter/roots.rs`
- `codegen/src/interpreter/task_group.rs`
- `codegen/src/interpreter/tests.rs`
- `codegen/src/jit/symbols.rs`
- `codegen/src/layout.rs`
- `codegen/src/lir.rs`
- `codegen/src/lir/array_ownership.rs`
- `codegen/src/lir/builder.rs`
- `codegen/src/lir/call.rs`
- `codegen/src/lir/lowering.rs`
- `codegen/src/lir/reference_description_tests.rs`
- `codegen/src/lir/verify.rs`
- `codegen/src/lir/verify_counted_operations.rs`
- `codegen/src/lower/func.rs`
- `codegen/src/lower/func/coroutine.rs`
- `codegen/src/lower/func/expr.rs`
- `codegen/src/lower/func/frame_root_tests.rs`
- `codegen/src/lower/func/intrinsic.rs`
- `codegen/src/lower/func/terminator.rs`
- `codegen/src/lower/func/value.rs`
- `codegen/src/lower/mod.rs`
- `codegen/src/root_storage.rs`
- `codegen/src/root_storage/value_liveness.rs`
- `codegen/tests/cemit.rs`
- `codegen/tests/interpreter_roots.rs`
- `codegen/tests/lir-goldens/corpus.txt`
- `codegen/tests/lir.rs`
- `codegen/tests/lir/verifier.rs`
- `codegen/tests/support/lir_facts_lifetime.rs`
- `compiler/src/check/expr/method.rs`
- `compiler/src/check/rejection_sites.rs`
- `compiler/src/check/rejection_surface_classes.rs`
- `compiler/src/check/rejection_witness_index.rs`
- `compiler/src/check/rejection_witnesses.txt`
- `compiler/src/hir/intrinsics.rs`
- `compiler/src/hir/sites.rs`
- `compiler/src/hir/tests.rs`
- `compiler/src/lir.rs`
- `compiler/src/lir_text.rs`
- `compiler/tests/corpus_reject.rs`
- `corpus/accept/a341-reference-holder-release.expected`
- `corpus/accept/a341-reference-holder-release.ts`
- `corpus/reject/r388-counted-map-for-each.ts`
- `corpus/trap/t100-conditional-await-collect.expected`
- `corpus/trap/t100-conditional-await-collect.ts`
- `corpus/trap/t101-unused-coroutine-parameter.expected`
- `corpus/trap/t101-unused-coroutine-parameter.ts`
- `corpus/trap/t102-generator-parameter.expected`
- `corpus/trap/t102-generator-parameter.ts`
- `corpus/trap/t103-unstarted-generator-parameter.expected`
- `corpus/trap/t103-unstarted-generator-parameter.ts`
- `corpus/trap/t104-coroutine-closure-environment.expected`
- `corpus/trap/t104-coroutine-closure-environment.ts`
- `corpus/trap/t94-counted-map-delete.expected`
- `corpus/trap/t94-counted-map-delete.ts`
- `corpus/trap/t95-counted-map-clear.expected`
- `corpus/trap/t95-counted-map-clear.ts`
- `corpus/trap/t96-counted-map-replace.expected`
- `corpus/trap/t96-counted-map-replace.ts`
- `corpus/trap/t97-counted-field-collect.expected`
- `corpus/trap/t97-counted-field-collect.ts`
- `corpus/trap/t98-counted-field-task-order.expected`
- `corpus/trap/t98-counted-field-task-order.ts`
- `corpus/trap/t99-loop-result-collect.expected`
- `corpus/trap/t99-loop-result-collect.ts`
- `generated-docs/corpus-index.md`
- `runtime/src/assoc_copy.rs`
- `runtime/src/assoc_counted.rs`
- `runtime/src/assocops.rs`
- `runtime/src/context.rs`
- `runtime/src/context/async_scheduler.rs`
- `runtime/src/context/counted.rs`
- `runtime/src/context/lifecycle.rs`
- `runtime/src/context/memory.rs`
- `runtime/src/context/tests_0.rs`
- `runtime/src/ffi/associations.rs`
- `runtime/src/ffi/memory.rs`
- `runtime/tests/async_inspection.rs`
- `runtime/tests/counted_collection.rs`
- `runtime/tests/counted_completion_form.rs`
- `runtime/tests/counted_map.rs`
- `runtime/tests/counted_object.rs`
- `specs/tracking/s172-reference-holders.md`

## 15. Frame-lifetime storage under amended rule 7

The section 14 measurements describe the earlier interval-clear rule.
The amended rule retains coroutine parameters, function environments, stable targets, and Frame locals until completion.
The shared live set remains the other part of the root set.

### Callback measurements

The round 9 working tree prints `8` twice for a342 in JIT, C AOT, and the interpreter.
The first callback captures an integer through another closure.
The second callback captures a reference-class Box through another closure.
A control adds `Context.suspend()` to tick; all three tiers still print `8` twice.
Thus, these supplied shapes do not reproduce the handoff's reported native failure on this tree.
Node v24.18.0 with TypeScript 5.9.2 prints the same bytes, `380a380a` in hexadecimal.
The a342 header declares JavaScript comparison.

### Storage facts

The native tiers no longer clear parameters, function environments, or stable targets at interval ends or suspension.
Both factories store all parameters, including unused parameters.
Initial resume reads parameter storage and preserves it.
Suspension arguments and child handles still clear at restore.
Normal return and unwind clear the complete payload; completion storage holds the result separately.

Both native tiers use `root_storage::stable_values`.
Its LIR fact is an AddressOfValue or AllocateClass address result that occurs as a suspension argument.
Reference-class handle allocations need no stable target field.
The interpreter derives the same stable target IDs from this shared fact.

Each interpreter function caches its frame-lifetime value IDs once.
An unfinished frame retains its parameter values, function captures, stable targets, and valid Frame locals across suspension.
The live-value walk supplies the other roots at the current LIR point.
Return or raise clears the frame values and locals; a finished frame contributes no roots.

A coroutine function value retains its environment field even when its block parameter has no later use.
C declares those values before it assigns their environments.
This preserves the function-replacement control without an undeclared C identifier.

### Changed probe results

The original t101, t102, t103, and t104 bodies now pass their collection points in all three tiers.
Their parameter or environment storage belongs to an unfinished coroutine under amended rule 7.
Each entry adds collection after completion so that it still tests the trap corpus contract.

| Entry | Stdout before the final trap | Final trap position | Golden change |
|---|---|---|---|
| t101 | start, after | 19:79 | Adds after |
| t102 | after | 24:5 | None |
| t103 | after | 22:5 | Adds after |
| t104 | after | 21:61 | Adds after |

Each final collection reports `Error: lost` in all three tiers.
Each survival control omits the final collection and completes without a trap.
The nested closure control also retains its captures until completion.
The replacement control prints `value=1` and `value=2`; the original closure definition still retains its capture.
The retained replacement control prints `value=1` twice.
The mutable parameter control still prints kept and found, then traps after generator completion.
The t99 and t100 outputs and trap positions remain unchanged.
No other existing output golden changes.

### Total native check

The check derives the complete field inventory, reference-bearing lifetime set, and suspension live fields directly from LIR types and operations.
This derivation does not read the emitter's field plan or call the shared stable-value selector.
The C declaration inventory and JIT field plan each equal the independently derived inventory as sets.
The C and JIT inventories also equal each other as sets.

The check reads C assignments and Cranelift stores at each LIR point.
It propagates occupied storage through control-flow edges and exception handlers.
A resume removes the source suspension's spill and child fields after the check proves their native restore clears.
At each suspension, each native field set equals the frame-lifetime set plus the shared live spill set.
A missing field and an outside field both fail.
The check also rejects premature lifetime clears at earlier LIR points.
Test-only source coordinates distinguish completion clears from resume clears.

Hand-built C declarations and JIT stores exercise missing and extra roots.
Their expected parameter root comes from a separately checked and lowered LIR function.
The check covers 322 coroutine functions and 517 suspension points in both native tiers.

The factory check requires every reference-bearing parameter field to receive its argument before the first resume.
The completion check requires every JIT payload byte to clear and checks the C payload clear.
A frame with only a header has no payload bytes to clear.
The store reader follows zero extension and hexadecimal constants when it reads a native memset.

### Performance

The release async-cost binary runs three times alone after all builds and tests stop.
Each workload retains the three-iteration and 200 ms warm-up floors and eleven timed samples.
The table reports each run's median; the best median supplies the comparison.
The pin values are the section 12 baseline.

| Workload | Run 1 ms | Run 2 ms | Run 3 ms | Best ms | Pin ms | Best / pin |
|---|---:|---:|---:|---:|---:|---:|
| settled-awaits | 14.657 | 15.512 | 15.557 | 14.657 | 14.929 | 0.9818 |
| held-handles | 4.514 | 4.524 | 4.669 | 4.514 | 4.329 | 1.0427 |
| deep-chains | 10.478 | 10.916 | 10.914 | 10.478 | 10.583 | 0.9901 |

All three ratios pass the 1.05 limit.
Every workload reports stable output and zero unfinished tasks in every run.
C compilation remains outside the measured execution span.

The debug interpreter corpus binary also runs three times alone, without compilation in the measured span.
Each run checks 266 outputs, omits the benchmark, and retains 64 declared exclusions.
Every selected output matches its golden.
The added a342 accounts for the extra output against section 14.
The table records process wall time, including test startup.

| Tree | Run 1 seconds | Run 2 seconds | Run 3 seconds | Best seconds |
|---|---:|---:|---:|---:|
| 9647c72a, section 11 | 9.045 | 8.964 | 9.033 | 8.964 |
| Amended rule 7 tree | 9.612 | 9.307 | 9.292 | 9.292 |

The best-of-three ratio is 1.0366 and passes the 1.05 limit.

### Verification

`cargo build --offline --locked --workspace --all-targets`: passes.
`cargo test --offline --locked -p subscript-compiler`: 1063 passed, 1 ignored, zero failures.
`cargo test --offline --locked -p subscript-codegen`: 828 passed, 1 ignored, zero failures.
`cargo test --offline --locked -p subscript-runtime`: 426 passed, 1 ignored, zero failures.
The final native field tests also pass with factory, completion, and control-flow checks.
`cargo fmt --check`: passes.
`cargo clippy --offline --locked --workspace --all-targets`: passes with no new warning; existing target warning counts stay unchanged.
All changed Rust files have at most 1,971 lines.
The corpus index comes only from `generate-api-reference`.
The LIR text golden includes a342 through the existing capture helper.

`tools/hygiene.sh`: passes.
`git diff --check`: passes.

### Complete changed-file inventory

The complete working tree differs from `f44126aa` in 108 files.

- `codegen/src/cemit.rs`
- `codegen/src/cemit/access.rs`
- `codegen/src/cemit/arith.rs`
- `codegen/src/cemit/body.rs`
- `codegen/src/cemit/collection.rs`
- `codegen/src/cemit/emitter.rs`
- `codegen/src/cemit/frame_roots.rs`
- `codegen/src/cemit/graph.rs`
- `codegen/src/cemit/intrinsic.rs`
- `codegen/src/cemit/suspend.rs`
- `codegen/src/cemit/terminator.rs`
- `codegen/src/counted.rs`
- `codegen/src/interpreter.rs`
- `codegen/src/interpreter/async_all.rs`
- `codegen/src/interpreter/collection_tests.rs`
- `codegen/src/interpreter/collections.rs`
- `codegen/src/interpreter/counted.rs`
- `codegen/src/interpreter/dispatch.rs`
- `codegen/src/interpreter/instruction.rs`
- `codegen/src/interpreter/intrinsics.rs`
- `codegen/src/interpreter/memory.rs`
- `codegen/src/interpreter/operations.rs`
- `codegen/src/interpreter/reference_holder_tests.rs`
- `codegen/src/interpreter/roots.rs`
- `codegen/src/interpreter/task_group.rs`
- `codegen/src/interpreter/tests.rs`
- `codegen/src/jit/symbols.rs`
- `codegen/src/layout.rs`
- `codegen/src/lir.rs`
- `codegen/src/lir/array_ownership.rs`
- `codegen/src/lir/builder.rs`
- `codegen/src/lir/call.rs`
- `codegen/src/lir/lowering.rs`
- `codegen/src/lir/reference_description_tests.rs`
- `codegen/src/lir/verify.rs`
- `codegen/src/lir/verify_counted_operations.rs`
- `codegen/src/lower/func.rs`
- `codegen/src/lower/func/coroutine.rs`
- `codegen/src/lower/func/expr.rs`
- `codegen/src/lower/func/frame_root_tests.rs`
- `codegen/src/lower/func/intrinsic.rs`
- `codegen/src/lower/func/terminator.rs`
- `codegen/src/lower/func/value.rs`
- `codegen/src/lower/mod.rs`
- `codegen/src/root_storage.rs`
- `codegen/src/root_storage/value_liveness.rs`
- `codegen/tests/cemit.rs`
- `codegen/tests/interpreter_roots.rs`
- `codegen/tests/lir-goldens/corpus.txt`
- `codegen/tests/lir.rs`
- `codegen/tests/lir/verifier.rs`
- `codegen/tests/support/lir_facts_lifetime.rs`
- `compiler/src/check/expr/method.rs`
- `compiler/src/check/rejection_sites.rs`
- `compiler/src/check/rejection_surface_classes.rs`
- `compiler/src/check/rejection_witness_index.rs`
- `compiler/src/check/rejection_witnesses.txt`
- `compiler/src/hir/intrinsics.rs`
- `compiler/src/hir/sites.rs`
- `compiler/src/hir/tests.rs`
- `compiler/src/lir.rs`
- `compiler/src/lir_text.rs`
- `compiler/tests/corpus_reject.rs`
- `corpus/accept/a341-reference-holder-release.expected`
- `corpus/accept/a341-reference-holder-release.ts`
- `corpus/accept/a342-awaited-nested-closure.expected`
- `corpus/accept/a342-awaited-nested-closure.ts`
- `corpus/reject/r388-counted-map-for-each.ts`
- `corpus/trap/t100-conditional-await-collect.expected`
- `corpus/trap/t100-conditional-await-collect.ts`
- `corpus/trap/t101-unused-coroutine-parameter.expected`
- `corpus/trap/t101-unused-coroutine-parameter.ts`
- `corpus/trap/t102-generator-parameter.expected`
- `corpus/trap/t102-generator-parameter.ts`
- `corpus/trap/t103-unstarted-generator-parameter.expected`
- `corpus/trap/t103-unstarted-generator-parameter.ts`
- `corpus/trap/t104-coroutine-closure-environment.expected`
- `corpus/trap/t104-coroutine-closure-environment.ts`
- `corpus/trap/t94-counted-map-delete.expected`
- `corpus/trap/t94-counted-map-delete.ts`
- `corpus/trap/t95-counted-map-clear.expected`
- `corpus/trap/t95-counted-map-clear.ts`
- `corpus/trap/t96-counted-map-replace.expected`
- `corpus/trap/t96-counted-map-replace.ts`
- `corpus/trap/t97-counted-field-collect.expected`
- `corpus/trap/t97-counted-field-collect.ts`
- `corpus/trap/t98-counted-field-task-order.expected`
- `corpus/trap/t98-counted-field-task-order.ts`
- `corpus/trap/t99-loop-result-collect.expected`
- `corpus/trap/t99-loop-result-collect.ts`
- `generated-docs/corpus-index.md`
- `runtime/src/assoc_copy.rs`
- `runtime/src/assoc_counted.rs`
- `runtime/src/assocops.rs`
- `runtime/src/context.rs`
- `runtime/src/context/async_scheduler.rs`
- `runtime/src/context/counted.rs`
- `runtime/src/context/lifecycle.rs`
- `runtime/src/context/memory.rs`
- `runtime/src/context/tests_0.rs`
- `runtime/src/ffi/associations.rs`
- `runtime/src/ffi/memory.rs`
- `runtime/tests/async_inspection.rs`
- `runtime/tests/counted_collection.rs`
- `runtime/tests/counted_completion_form.rs`
- `runtime/tests/counted_map.rs`
- `runtime/tests/counted_object.rs`
- `specs/tracking/s172-reference-holders.md`
