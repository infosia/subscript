# §171 — An array owns its counted elements

Contract pin: `96f46f35`. Host: macOS, AArch64. Rust: `1.95.0`.
The amended contract pins have the same production code as this pin.
`J/C/I` means dev JIT, C AOT, and interpreter, in that order.

## Result and form

The three tiers implement recursive counted holders for handles, arrays, FixedArray values, and IterResult values.
An array alias owns the array, rather than a count for each current element.
The last holder releases the elements and frees the array storage.
An element copy acquires its own count. A fresh element transfers its count.
Replacement releases the previous element. Removal transfers the element count to its result; discard releases that result.
Completion caches own counted payloads. Each completion read acquires a separate result owner before frame release.
Generator yield copies the counted value into an IterResult holder that survives normal generator completion.

HIR types carry recursive counted shapes. LIR carries explicit count actions, operand roles, and immediate trap sites.
The verifier derives each obligation from the opcode and static types, then compares it with the emitted action.
Hand-built forms omit required actions or trap sites and fail verification.
The native emitters and interpreter consume the same count facts.
Bulk mutations, counted completion reads, and explicit releases carry an immediate `Call` trap check.
Quiet uncounted completion reads emit no Context pending-word check.

Counted callback and equality/join positions receive S014 under rule 5a, at every nesting depth.
The checker covers receiver elements, callback parameters, callback results, accumulators, and method results.
A counted lambda capture cannot outlive its source binding.
Reference-class fields and Map values retain their separate §172 lifetimes.
A dropped suspended generator retains its frame; exhaustion releases its holders.

## Operation holds

A lexical binding holds each counted loop subject until normal exit, break, return, or exception cleanup.
Expression bindings hold receivers and earlier operands across later user code.
This includes calls, array elements and spreads, indexed reads, and indexed stores.
An immutable local already owns its input through the expression.
`Promise.all` acquires its input elements before it returns and retains no array reference while child tasks run.
Generator `next()` retains a generator reference, rather than an array subject.

`a340` resets its module-global subject, then allocates another two-element array in each loop body.
The replacement tasks use local awaited handles to satisfy S013.
The golden remains `11\n22\n`.
The test compares output in all three tiers and independently checks the LIR subject retain before `IteratorCreate`.
The no-hold probe suppresses acquisition and release for both subject bindings at the loop position.
Those bindings are the HIR synthetic subject and the LIR lexical subject.
The probe prints `11`, `44` in the interpreter and JIT; C AOT prints `11`, then exits with SIGSEGV.
The output assertion fails before the LIR-shape assertion. The probe command exits 101.
`target/s171-15-loop-red.log` records that failure; `target/s171-15-loop-green.log` records the restored hold.
The production builder and loop lowering match their pre-probe source bytes.

## Corpus evidence

Every §171 corpus header names `96f46f35` and the result for each tier in the same format.
The pin accepts the revised `a340` and prints its golden because it does not free the last array holder.
A release test built from the pin's production code confirms all three outputs.
`target/s171-15-loop-pin.log` records that execution.
That pin result does not establish the loop hold.
The other accept and trap entries expose pin failures, as the evidence tables show.
`r387` requires S014; its pin LIR failure does not satisfy that checker requirement.
Stock `tsc` 5.9.2 accepts the §171 sources, including all six rejected-method calls.
Node matches the accept-entry goldens.
The trap goldens require an unobserved-exception trap and exact stdout.
For `t92`, stdout includes `yielded` and `completed` before the trap.

| Probe | Contract pin | Implementation |
|---|---|---|
| `a340`, reset followed by replacement-array allocation | All tiers print `11`, `22`; no last-holder storage free | All tiers print `11`, `22`; the subject has a lexical holder |
| `t93`, counted `fill` over an unobserved failure | Interpreter and C AOT print `after`; JIT prints `after`, then reports use-after-delete | All tiers report the unobserved exception with empty stdout and an immediate `Call` site |
| Counted `copyWithin(0, 1)` over `[fail(), a]` | Same output and faults as `fill` | Same immediate exception trap as `fill` |
| Two-element fresh receiver, `pop` | JIT retains one completed task; the named-array control retains none | All tiers retain zero tasks for both forms |
| Two-element fresh receiver, `push` | JIT retains one completed task; the named-array control retains none | All tiers retain zero tasks for both forms |

The earlier loop source contained only the reset and printed `11`, `22` with or without its hold.
The replacement-array allocation makes the output assertion fail when the subject has no hold.
The named `nextBatch().pop()` probe receives S013; the accepted fresh receiver uses `[h, h].pop()`.
The bulk instruction-site assertion fails when the action omits its immediate trap site, even if stdout still agrees.

## JIT reservation evidence

The reload module `a338-counted-array-holders` sets the floor; its measured source contains 3,897 bytes.
The previous reservation was 174,235 bytes. The resume function requested 152,044 bytes after earlier definitions and data allocations.
Page rounding requires 196,608 bytes. The reload test now fits and leaves zero retained tasks.
`codegen/src/jit/memory.rs` contains the single slope derivation for the same 989-module measurement set.
The floor is 196,608 bytes, the slope is 3, and the margin is 1.5.

## Measurement method

Each task number counts registered tasks after roots finish and the ready queue becomes empty.
The JIT reads `ReloadSession::async_tasks`. C uses a non-null `subscript_rt_ctx_visit_async_tasks` visitor.
The interpreter excludes async registry entries whose owner count is zero.
A trap row counts the registry at the stop, before Context destruction.
`—` means checker rejection or a C build failure. Zero tasks does not imply zero Context allocations.
No measurement invokes an implicit collector.

Successful probes use `work(): Promise<void>` and `main` that awaits `use`.
A failed task throws `Error("inner")`. An `if (false)` await satisfies must-await without observation.
Alias probes await their initial elements before mutation.
Global aliases reset to `[]`; field aliases free their objects.
Numeric alias-matrix cells give the same count in J/C/I, with no trap.
`S` means a same-element index store. `D` means a distinct-element index store.
Completion probes read the cached result zero, one, or two times.
Their synchronous controls use the same result shape; observed-failure controls catch the inner exception.

## Baseline measurements

The tables below describe the pin or the explicitly named pre-implementation probe revision.
They retain the measured failures and zero controls; they do not describe the final implementation.
The layout prototype adds a u32 holder field at offset 32, with four alignment bytes.
The array payload grows from 32 to 40 bytes. Both ship requests use the same 64-byte block.
Metadata offsets describe this Rust build and do not promise a portable ABI.
Form A uses array holder counts. Form B restricts the array to a unique owner.
Operation counts describe these candidate forms; they are not execution-time ratios.

### Holder positions

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

### Array aliases and mutations

| Alias form | No mutation | `pop` | `shift` | `splice` | `push` | Index S / D | `unshift` | `fill` | `reverse` | `copyWithin` |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| Synchronous parameter | 0 | 1 | 1 | 1 | 0 | 0 / 1 | 0 | 1 | 0 | 1 |
| Async parameter | 0 | 1 | 1 | 1 | 0 | — / 1 | 1 | 1 | 0 | 1 |
| `const b=a` | 0 | 1 | 1 | 1 | 0 | — / 1 | 1 | 1 | 0 | 1 |
| Module global | 0 | 1 | 1 | 1 | 0 | — / 1 | 1 | 1 | 0 | 1 |
| Class field, explicitly freed | 0 | 1 | 1 | 1 | 0 | — / 1 | 1 | 1 | 0 | 1 |
| Synchronous return value | 0 | 1 | 1 | 1 | 0 | — / 1 | 1 | 1 | 0 | 1 |
| Nested array element | 1 | 1 | 1 | 1 | 1 | — / 1 | 1 | 1 | 1 | 1 |


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

### Counted completions

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

### Layout and operation cost

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


| Measured layout or allocation | Current | With proposed field | Cost |
|---|---:|---:|---|
| `repr(C)` array payload | 32 bytes | 40 bytes; `u32` count at 32 | 4 count bytes plus 4 alignment bytes |
| Dev exact-size allocation, header included | 48 bytes | 56 bytes | 8 bytes per array header |
| Ship arena payload accounting | 48 bytes | 48 bytes | Both requests use the same 64-byte block |
| Fresh ship arena reservation | 65,536 bytes | 65,536 bytes | Same initial chunk reservation |
| `AsyncFrameMeta` | 88 bytes, alignment 8 | 88 bytes with `u8 release_kind` | Tag uses offset 85 on this host |
| Metadata tail | `host_root` at 84; bytes 85–87 unused | Tag at 85; bytes 86–87 unused | No field moves in the measured tag prototype |


| Operation, for N elements | Current count work | Form A count work | Form B count work |
|---|---|---|---|
| Array alias acquisition and exit | N handle retains and N current-element releases | One array retain and one array release; the last exit releases N elements | Synchronous borrow: zero retains and releases |
| Store one copied handle | One element retain through the counted-store path | One element retain; release the previous element on replacement | Same element work in the unique owner |
| Store a fresh handle | Transfer where the operation carries the fresh-owner fact | Transfer one count | Transfer one count |
| Remove an element | `pop` transfers; other measured removals omit complete ownership handling | Transfer one count to the result; discard releases it | Same transfer and discard requirement |
| Final array holder exit | Each alias releases the array's current elements | One zero-count branch plus N element releases | N element releases at the unique owner's exit |
| Cached completion read | Byte copy; later local binding retains | Acquire the result's own count before the source frame releases | Handle-array result must transfer or satisfy the chosen single-owner restriction |
| Frame free with counted completion | No payload release | One typed payload release | One typed payload release for admitted result shapes |

### Corpus reach and candidate forms

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


| Form | Evidence and required facts | Accepted programs that it changes or rejects | Existing tests and witness ids |
|---|---|---|---|
| A. The array owns its element counts | An array needs its own count. Payload offset 32 gives a 40-byte header. Aliases change that count only. Last release releases the elements. | Admits the measured alias forms. Changes removal, replacement, insertion, and nested-array lifetime. Does not by itself release Map values or fixed-array elements. | `codegen/tests/task_group_array_count_form.rs::synchronous_array_parameter_pop_keeps_an_orphan_count`; `runtime/tests/task_group_array_count_form.rs::empty_array_aliases_cannot_release_the_count_of_a_removed_element`; `codegen/tests/lir.rs::counted_store_verifier_reports_a_missing_retain`; `counted_store_corpus_matches_the_interpreter`; `compiler/src/lir.rs::tests::fresh_async_owner_instruction_table_names_allocations_and_calls` requires the corresponding fresh-result facts |
| B. A handle array has one owner | A synchronous parameter borrows. A borrow cannot escape into a field, global, result, nested element, or escaping closure. Moves need a consumed-source fact. | Rejects `a162` as written. Rejects the measured live `const`, `let`, return, global, field, nested, and conditional alias uses. A move can admit a source that is never read afterward. Async parameters need moves or rejection. | Both array count-form tests above change their expected count to zero for a synchronous borrow. `counted_store_corpus_matches_the_interpreter` loses `a162` acceptance. Constructor/field ownership cases need rejection witnesses. |
| Completion release | Registration carries a result-release kind. Frame free releases the cached owner. Each successful result read acquires one independent owner before frame release. | Repairs the measured handle and immediate handle-array completions. Does not require new acceptance. Does not remove §166's separate counted-result restriction without an explicit contract change. | `codegen/tests/task_group_count_form.rs::async_return_keeps_a_count_after_the_last_script_holder_exits`; `runtime/tests/counted_completion_form.rs::completion_release_does_not_release_a_counted_payload`; `PromiseAllCountedResult`, witness `s166-PromiseAllCountedResult`, variant `Divergence::PromiseAllCountedResult` |


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

### Corpus evidence at `96f46f35`

| Entry | Pin checker | Pin dev JIT | Pin C AOT | Pin interpreter | Required result |
|---|---|---|---|---|---|
| `a338-counted-array-holders` | Accepts | Exit 1; internal trap after `reuse 99` | Exit 3; trap 11 after the same line | Complete golden stdout | Golden stdout in each tier |
| `a339-counted-generator-holders` | Accepts | Exit 1; internal trap after `handle before 7`, `handle done true`, `reuse 99` | Exit 3; trap 11 after the same lines | `expected async handle, found Null` after `handle before 7` | Complete golden stdout |
| `t87-discarded-counted-shift` | Accepts | Exit 0; no trap | Exit 0; no trap | Empty stdout; no trap | Trap 29 |
| `t88-parameter-counted-pop` | Accepts | Exit 0; no trap | Exit 0; no trap | Empty stdout; no trap | Trap 29 |
| `t89-counted-array-completion` | Accepts | Exit 0; no trap | Exit 0; no trap | Empty stdout; no trap | Trap 29 |
| `t90-nested-counted-array` | Accepts | Exit 0; no trap | Exit 0; no trap | Empty stdout; no trap | Trap 29 |
| `t91-fixed-counted-array` | Accepts | Exit 0; no trap | Exit 0; no trap | Empty stdout; no trap | Trap 29 |
| `t92-counted-generator-holder` | Accepts | Exit 1; trap 29 before `completed` | Exit 3; trap 29 before `completed` | Trap 29 before `completed` | `yielded`, `completed`, then trap 29 |
| `r387-nested-counted-array-methods` | Accepts all six calls | Exit 2; counted-store LIR failure | Exit 1; the same LIR failure | The same LIR failure before execution | S014 at all six calls |


| Entry | Rule-derived release that reaches zero |
|---|---|
| `t87` | `shift` transfers the element owner; the statement discards and releases it. |
| `t88` | The returned local keeps the removed element until the caller exits. |
| `t89` | Frame free releases the cached array owner; result-holder exit releases the other owner and its failed element. |
| `t90` | The outer array releases the inner array, which releases its failed element. |
| `t91` | The fixed-array holder exits and releases its failed element. |
| `t92` | Generator completion releases its local; the caller's later `IterResult` exit releases the yielded failed handle. |

### Operation classes

| Checker operation | Class | Count rule |
|---|---|---|
| Index store; `push`; one-value `unshift`; `fill`; `copyWithin` | 5 | Acquire each stored element; release each replaced element. |
| `pop`; `shift`; delete-only `splice` | 5 | Transfer removed elements to the result; release a discarded result. |
| `reverse` | 5 | Reorder without an element-count change. |
| Array literal; `Array.of`; spread; `Array.from`; `slice`; `concat` | 5 | Give the new array one count of each element. |
| Index read; `at`; array `for…of` value binding | 5 | Borrow an element; acquire a count for a stored read. |
| `fill`; `reverse`; `copyWithin` result | 5 | Borrow the receiver; acquire its holder count for a store. |
| `length` | 5 | Read no element. |
| Fused `for…of jobs.values()` | 5 | Rewrite to a value traversal. |
| Fused `for…of jobs.keys()` | 5 | Rewrite to an index traversal; the integer binding holds no count. |
| Array binding pattern `[job] = jobs` | 5 | Rewrite each binding to an index read. |
| `Promise.all(jobs)` | 5 | Borrow the array; the aggregate owns its independent input snapshot. |
| `forEach`; `map`; `filter`; `reduce`; `reduceRight`; `some`; `every` | 5a | Reject each counted callback position with S014. |
| `find`; `findLast`; `findIndex`; `findLastIndex`; `flatMap`; `sort` | 5a | Reject each counted callback position with S014. |
| `indexOf`; `lastIndexOf`; `includes`; `join` | 5a | Reject a counted receiver element with S014. |
| `toString` | 5a | Rewrite to `join`. |
| `Map.groupBy(jobs, callback)` | 5a | Reject the counted input, callback position, or result with S014. |
| Counted `yield`; counted `source.next()` result; generator `for…of` binding | 5b | Acquire the output owner; release the result holder or binding at exit. |

### The missing lexical-holder fact

| LIR probe at `63f1bbf1` | Verifier result |
|---|---|
| Lowered control without the alias | `Ok(())` |
| Lowered source with the alias | `Ok(())` |
| Alias source without both alias count instructions | `Ok(())` |
| Hand-built counted-array parameter followed by `Return`, without its exit release | `Ok(())` |

### Counted callback outputs from an uncounted receiver at `643fa6c4`

| Method | Expression | Result type |
|---|---|---|
| `map` | `numbers.map((n: i32): Promise<i32>[] => jobs)` | `Promise<i32>[][]` |
| `flatMap` | `numbers.flatMap((n: i32): Promise<i32>[] => jobs)` | `Promise<i32>[]` |
| `reduce` | `numbers.reduce((acc: Promise<i32>[], n: i32): Promise<i32>[] => acc, jobs)` | `Promise<i32>[]` |
| `reduceRight` | `numbers.reduceRight((acc: Promise<i32>[], n: i32): Promise<i32>[] => acc, jobs)` | `Promise<i32>[]` |


| Receiver | Method | Pin JIT | Pin C AOT | Pin interpreter | JIT tasks after drain | Control JIT tasks |
|---|---|---|---|---|---:|---:|
| `i32[]` | `map` | Exit 0; two lines | Exit 0; same stdout | Same stdout | 1 | 0 |
| `i32[]` | `flatMap` | Exit 0; two lines | Exit 0; same stdout | Same stdout | 0 | 0 |
| `i32[]` | `reduce` | Exit 2; LIR failure | Exit 1; LIR failure | Same LIR failure | — | 0 |
| `i32[]` | `reduceRight` | Exit 2; LIR failure | Exit 1; LIR failure | Same LIR failure | — | 0 |
| `FixedArray<i32, 1>` | `map` | Exit 0; two lines | Exit 0; same stdout | Same stdout | 1 | 0 |
| `FixedArray<i32, 1>` | `reduce` | Exit 0; two lines | Exit 0; same stdout | Same stdout | 0 | 0 |
| `FixedArray<i32, 1>` | `reduceRight` | Exit 0; two lines | Exit 0; same stdout | Same stdout | 0 | 0 |

### Counted captures that outlive their source block

| Captured type | Pin JIT | Pin C AOT | Pin interpreter |
|---|---|---|---|
| `Promise<i32>` | `before 7`, `reuse 99`, then use-after-delete at the lambda's captured read | Same prefix, then trap 11 at the later await | All three lines |
| `Promise<i32>[]` | Same prefix, then internal trap at the later await | Same prefix, then trap 11 | Same prefix, then `unknown packed async handle` |
| `Promise<i32>[][]` | All three lines | All three lines | All three lines |
| `FixedArray<Promise<i32>, 1>` | All three lines | All three lines | All three lines |
| `IterResult<Promise<i32>[]>` | Same prefix, then internal trap at the later await | Same prefix, then trap 11 | Same prefix, then `unknown packed async handle` |

## Final holder evidence

The alias matrix checks ten source and alias groups against twelve operations and their omitted-operation controls.
It includes 260 cases and same-element index stores.
The completion matrix covers four counted shapes with zero, one, and two reads, synchronous controls, and discarded results.
Direct runtime tests check recursive holder release and Rust/C layouts.
Three-tier tests require zero retained tasks on all four loop exits and after later-operand resets or exceptions.
Two-element temporary `pop` and `push` receivers leave zero completed tasks, as their named-array controls do.

### Measurement coverage

| Measurement rows | Added or existing coverage | Result or scope |
|---|---|---|
| Item 1: dynamic array; let and conditional aliases | Array alias matrix | Zero after holder exit; omitted-operation controls also give zero |
| Item 1: nested depths 2, 3, and 4 | Exact holder rows plus nested alias matrix | Zero, with non-executed holder controls |
| Item 1: for-of over an inner array; FixedArray | Exact holder rows | Zero, with non-executed holder controls |
| Item 1: synchronous array parameter and return; async array parameter | Alias matrix and exact holder rows | Zero |
| Item 1: method parameters and getter array result | Exact holder rows | Zero; explicit object free ends the field holder |
| Item 1: field array; nested field array | Exact holder rows and field alias matrix | Zero after explicit free; a live field's separate lifetime is below |
| Item 1: module-global array; static array field | Global alias matrix and exact holder rows | Zero after reset; an inhabited global has no program scope exit |
| Item 1: synchronous lambda capture of an array | Exact holder rows | Zero; the captured local stays live through the call |
| Item 1: exhausted generator array parameter and local | Exact holder rows | Zero; dropped suspension is below |
| Item 1: fulfilled handle, array, nested array, and FixedArray | Completion matrix | Zero for zero, one, and two reads, with synchronous controls |
| Item 2: each alias, each removal, stored removal result, and omitted mutation | Array alias matrix | Zero; 260 cases include the same-element index store |
| Item 2: fresh push, fresh unshift, and local-alias fresh index stores | Fresh mutation rows | Zero across six alias groups and their omitted-operation controls |
| Item 2: synchronous same-element index store | Fresh mutation rows and synchronous alias matrix | Zero; the helper returns the saved element |
| Item 2: live local across copyWithin and aliased push | Exact holder rows and operation tests | Zero; the later await still reads the held value |
| Item 2: spread, slice, and concat | Exact holder rows and operation tests | Zero after both source and copied holders exit |
| Item 2: direct fresh index stores through global, field, or nested storage | Measured fresh-index position test | S013; observed-local controls accept and give zero in each tier |
| Item 2: failed task removal through a parameter, direct pop, and observed failure | `t88`, existing trap corpus, and differential trap tests | Trap agreement; observed controls give zero |
| Item 3: failed inner handle or array, async or synchronous return | Failed completion rows | Unobserved: trap 29 and two enclosing tasks; observed control: zero |
| Item 3: discarded counted await result | Completion matrix and counted operation tests | Zero; synchronous held-result controls give zero |
| Rejected item 1 forms | Existing rejection corpus and checker witnesses | Object literals, nullable non-reference owners, tuples, Set handles, and value-class fields have no accepted form |
| Rejected item 2 methods | `r387`, S014 witnesses, and existing checker tests | Rule 5a rejects counted callback and equality/join positions; fixed-array pop and free also reject |

### Separate holder lifetimes

| Accepted deferred row | Tasks J/C/I | Zero control J/C/I | Boundary |
|---|---|---|---|
| Reference-class array field; only its local object reference ends | 1/1/1 | Explicit object free: 0/0/0 | Rule 1 reserves the reference-object holder for §172 |
| Map value that holds a handle array | 1/1/1 | Non-executed store: 0/0/0 | Rule 1 reserves the Map value holder for §172 |
| Generator array parameter and local; iterator drops after one yield | 1/1/1 | Exhaust the iterator: 0/0/0 | Rules 12 and 171.3 item 1 preserve this suspension lifetime |

### References across user code

| Operation | Hold and exit |
|---|---|
| `for…of` over a counted subject | The loop declares a counted subject binding. Normal exit, `break`, `return`, and exception cleanup release it. |
| A call receiver or earlier argument, followed by user code in an argument or default | An expression binding holds the input through evaluation and the call. Normal exit and exception cleanup release it. |
| An earlier array-literal element or spread source, followed by user code in another element | The expression holds the counted input until the destination acquires its elements. Both exits release the input. |
| An indexed receiver, followed by user code in the index or assigned value | The expression holds the receiver through the read or store. Both exits release it. |

### Deep-chain generated work

| Pin-to-tree item in `deep-chains` | Emitted work | Rule attribution |
|---|---|---|
| `down` block 4 adds `AwaitRaise count=Uncounted` at source line 6 | No trap, runtime call, state read, or C statement | Completion count fact from rule 8 |
| `main` block 5 adds `AwaitRaise count=Uncounted` at source line 13 | No trap, runtime call, state read, or C statement | Completion count fact from rule 8 |
| `sub_f1_create` calls `async_register_uncounted` instead of `async_register` | Same three arguments and registration body | Rule 8 uncounted path |
| `sub_f2_create` calls `async_register_uncounted` instead of `async_register` | Same three arguments and registration body | Rule 8 uncounted path |
| `sub_f1_resume`, `resume_b3`, calls `async_result_uncounted` instead of `async_result` | Same four arguments; no description lookup or count acquire | Rule 8 uncounted path |
| `sub_f2_resume`, `resume_b2`, calls `async_result_uncounted` instead of `async_result` | Same four arguments; no description lookup or count acquire | Rule 8 uncounted path |
| Common `SsArrayHeader` declaration adds `uint32_t holders` | No access; `deep-chains` uses no array | Rule 2 holder layout |

The generated `deep-chains` C equals the pin after ABI names and the unused array-header field are normalized.
No other instruction, trap site, runtime call, or emitted statement differs in that workload.
Both completions have type `I32`, so neither gains a counted completion trap.
The isolated and concurrent measurement trees emit the same code.

## Release cost measurements

Each revision uses its matching release driver and runtime archive.
Compilation and linking remain outside the timed span; program execution and Context release remain inside it.
Each run uses three warm-ups with a 200 ms floor, then eleven timed samples.
The comparison divides the best tree median by the best pin median across three runs.
All workload runs report stable expected output and zero unfinished tasks.
Within-run spreads range from 0.2% to 6.1%, below the 20% noise limit.

### Completion-ownership prototype, nanoseconds

| Workload | Pin run 1 | Pin run 2 | Pin run 3 | Tree run 1 | Tree run 2 | Tree run 3 | Best ratio | 5% bound |
|---|---:|---:|---:|---:|---:|---:|---:|---|
| settled-awaits | 15898000 | 15685000 | 15545000 | 16243000 | 16112000 | 16344000 | 1.03647 | Pass |
| held-handles | 4531000 | 4532000 | 4388000 | 4783000 | 4765000 | 4798000 | 1.08592 | Fail |
| deep-chains | 10787000 | 10717000 | 10725000 | 11453000 | 11575000 | 11323000 | 1.05655 | Fail |

### Explicit-count-action prototype, nanoseconds

| Workload | Pin run 1 | Pin run 2 | Pin run 3 | Final tree run 1 | Final tree run 2 | Final tree run 3 | Best ratio | 5% bound |
|---|---:|---:|---:|---:|---:|---:|---:|---|
| settled-awaits | 15360000 | 15526000 | 15065000 | 16793000 | 16875000 | 16480000 | 1.09393 | Fail |
| held-handles | 4405000 | 4400000 | 4367000 | 4849000 | 4883000 | 4909000 | 1.11037 | Fail |
| deep-chains | 10506000 | 10676000 | 10713000 | 11099000 | 11186000 | 11149000 | 1.05644 | Fail |

### Runtime-phase probe, nanoseconds

| Runtime phase | Pin | Final tree | Ratio |
|---|---:|---:|---:|
| Register uncounted frames | 2394250 | 2210625 | 0.92331 |
| Cache uncounted completions | 2816875 | 2615208 | 0.92841 |
| Read uncounted completions | 749000 | 791375 | 1.05658 |
| Release individual frames | 11512959 | 10543333 | 0.91578 |
| Release one array of 100000 completed handles | 10123083 | 11133375 | 1.09980 |

### Direct-release implementation, nanoseconds

| Workload | Pin 1 ns | Pin 2 ns | Pin 3 ns | Tree 1 ns | Tree 2 ns | Tree 3 ns | Best ratio | 1.05 bound |
|---|---:|---:|---:|---:|---:|---:|---:|---|
| settled-awaits | 14991000 | 14803000 | 14786000 | 14387000 | 15016000 | 14970000 | 0.97302 | Pass |
| held-handles | 4179000 | 4329000 | 4315000 | 4188000 | 4279000 | 4316000 | 1.00215 | Pass |
| deep-chains | 10497000 | 10252000 | 10611000 | 10578000 | 10597000 | 10429000 | 1.01726 | Pass |

### Concurrent full-gate measurement, milliseconds

| Workload | Pin medians, ms | Tree medians, ms | Best tree / best pin | Bound |
|---|---|---|---|---|
| `settled-awaits` | 14.998, 15.751, 15.602 | 15.656, 16.308, 16.262 | 1.04387 | Pass, ≤ 1.05 |
| `held-handles` | 4.352, 4.504, 4.536 | 4.486, 4.768, 4.710 | 1.03079 | Pass, ≤ 1.05 |
| `deep-chains` | 10.291, 11.087, 11.109 | 11.054, 11.114, 11.065 | 1.07414 | Fail, > 1.05 |

### Isolated final measurement, milliseconds

| Workload | Pin medians, ms | Tree medians, ms | Best ratio | 1.05 bound |
|---|---|---|---|---|
| `settled-awaits` | 16.551, 17.745, 17.375 | 16.778, 16.268, 15.821 | 0.95589 | Pass |
| `held-handles` | 4.761, 5.074, 5.046 | 4.772, 4.723, 4.478 | 0.94056 | Pass |
| `deep-chains` | 11.647, 12.340, 12.380 | 11.627, 11.174, 11.004 | 0.94479 | Pass |

The concurrent `deep-chains` ratio of 1.07414 exceeded 1.05 because the full gate ran in the same tree.
The overlap ran from about 02:13 to 03:00 JST on 2026-10-07; `target/gate-full-s171.log` records the gate.
The isolated measurement ran alone and gives 0.94479 with the same generated code.
The isolated final measurements meet all three 1.05 bounds. They do not establish a speed improvement.
No additional deep-chain measurement replaces either recorded set.
Release `perf_gate` tests pass at the pin and implementation; recorded run times include 7.38 and 7.16 seconds.

## Verification

| Command or check | Result |
|---|---|
| `cargo build --offline --locked --workspace --all-targets` | Exit 0 |
| `cargo test --offline --locked -p subscript-compiler` | Exit 0 |
| `cargo test --offline --locked -p subscript-codegen` | Exit 0; loop output, LIR-shape assertion, and regenerated golden pass |
| `cargo test --offline --locked -p subscript-runtime` | Exit 0 |
| `cargo fmt --check` | Exit 0 |
| `cargo clippy --offline --locked --workspace --all-targets` | Exit 0; the 69 diagnostic headers match the saved baseline multiset |
| `tools/hygiene.sh` | Exit 0 |
| `git diff --check` | Exit 0 |
| Revised `a340`, no subject hold | Exit 101; output differs from the golden before the shape assertion |
| Revised `a340`, restored subject hold | Exit 0; golden output in all tiers; LIR subject retain exists |
| Revised `a340`, contract-pin production source | Exit 0; golden output in all tiers |
| Stock `tsc` 5.9.2 and Node, revised `a340` | Exit 0; Node output matches the golden bytes |

The LIR capture changes only the `a340` entry.
The document generator reproduces the existing generated documents without content changes.
The verification logs use the `target/s171-15-` prefix.
