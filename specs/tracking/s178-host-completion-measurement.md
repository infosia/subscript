# §178: Host completion sources

Contract pin: `77593125`. Platform: aarch64 macOS. The runtime and the JIT use release builds. C uses `cc -std=c11 -O2`.

## Part A: Existing forms

The fake host starts N operations at checkpoint 0. Each operation returns 7 at checkpoint k.
The host delivers completions immediately before the checkpoint drain. The script reports each result through `hostConsume`.
Each checksum must equal `7 * N`. Each sample uses a new Context. Each binary runs alone.

The timed span starts before `main` and ends after the last checkpoint. It excludes compilation, Context initialization, and Context destruction.
It includes host delivery, queue dispatch, result consumption, and one live-allocation read after `main`.
The tables use the minimum of three samples. No warmup precedes these three samples.

Each continuation runs at the first checkpoint drain after host delivery. All rows therefore have one drain of completion latency.
The instrumentation reports zero additional checkpoints, because delivery and that drain share the same host-step number.
The tables also state dispatches per operation.
Host-call counts exclude the `hostConsume` measurement call. They include script calls to start, test, and read an operation.
Callback delivery is a host-to-script call. The callback column counts this call separately.

The allocation column counts `Context::alloc` requests over the timed span.
A temporary atomic counter records these requests.
A thread-local counter records each scheduler dispatch. The host uses the ordinary unbudgeted checkpoint API. Rust queue, map, callback-registration, and result-cache allocations are outside this counter.
The observer adds the same atomic operation to every Context allocation. These times are instrumented measurements.

The poll form uses one async task per operation:

```ts
async function work(id: i32): Promise<void> {
  const request = hostStart(id);
  while (hostDone(request) == 0) {
    await Context.suspend();
  }
  hostConsume(id, hostResult(request));
}
```

The callback form uses a non-capturing callback and a separate userdata object.
The C declaration has the §111 callback-info layout. The mirror selects its explicit callback lifetime.
The callback sets `result = message.length` and `done = 1`. The wait task polls `done` with `Context.suspend()`.
The host passes the seven-byte view `1234567`. It releases each registration after the callback returns.
This encoding respects the current callback ABI, which accepts a string view and two userdata pointers, rather than an i32 callback argument.

The batch form uses one async task and an array of request ids.
At each checkpoint, this task checks every unfinished id, consumes completed results, and marks those ids as -1.
This accepted form removes per-operation scheduler wakeups. It still polls the host N times per checkpoint.
It suits a batch owner that can process results in one script segment. It does not supply separate awaitable operations.

### Exception path

The current callback trampoline settles an escaping exception into a Context trap.
It cannot deliver that exception to the wait task's await.
Evidence: `runtime/src/ffi/callbacks.rs::fire_callback` calls `Context::settle_uncaught_exception` after the callback.

A poll adapter can preserve exception delivery if it exposes failure status and message.
The async wrapper must then throw `new Error(message)` before it returns. Its caller observes the exception at `await work(...)`.
The simple value-only poll and callback workloads do not implement this failure protocol.

### A1. Measured costs

| Form | N | k | JIT ns/op | C AOT ns/op | Dispatches/op | Script host calls/op | Callback calls/op | Context requests/op |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| poll | 1 | 1 | 1959.00 | 1000.00 | 2 | 4 | 0 | 4 |
| poll | 1 | 10 | 2583.00 | 2000.00 | 11 | 13 | 0 | 4 |
| poll | 100 | 1 | 241.25 | 130.00 | 2 | 4 | 0 | 1.08 |
| poll | 100 | 10 | 407.92 | 290.00 | 11 | 13 | 0 | 1.08 |
| poll | 10,000 | 1 | 202.56 | 91.90 | 2 | 4 | 0 | 1.0015 |
| poll | 10,000 | 10 | 356.70 | 214.90 | 11 | 13 | 0 | 1.0015 |
| callback | 1 | 1 | 3083.00 | 1000.00 | 2 | 1 | 1 | 6 |
| callback | 1 | 10 | 3125.00 | 2000.00 | 11 | 1 | 1 | 6 |
| callback | 100 | 1 | 437.50 | 220.00 | 2 | 1 | 1 | 3.08 |
| callback | 100 | 10 | 658.34 | 440.00 | 11 | 1 | 1 | 3.08 |
| callback | 10,000 | 1 | 427.02 | 160.80 | 2 | 1 | 1 | 3.0015 |
| callback | 10,000 | 10 | 629.95 | 313.10 | 11 | 1 | 1 | 3.0015 |
| batch | 1 | 1 | 1208.00 | 2000.00 | 1 | 3 | 0 | 3 |
| batch | 1 | 10 | 1958.00 | 2000.00 | 10 | 12 | 0 | 3 |
| batch | 100 | 1 | 34.58 | 40.00 | 0.01 | 3 | 0 | 0.08 |
| batch | 100 | 10 | 84.17 | 70.00 | 0.1 | 12 | 0 | 0.08 |
| batch | 10,000 | 1 | 12.34 | 10.60 | 0.0001 | 3 | 0 | 0.0015 |
| batch | 10,000 | 10 | 53.43 | 36.90 | 0.001 | 12 | 0 | 0.0015 |

The per-operation poll and callback forms dispatch `k + 1` jobs per operation.
Each work task dispatches k times. The main task dispatches once per awaited work handle.
Of the k work dispatches, `k - 1` cannot consume a result.
The batch form dispatches k jobs for the whole batch. Its first `k - 1` jobs cannot consume a result.
The batch form avoids the initial per-request readiness test, because it always waits for the first checkpoint.

The N=1 C samples have one-microsecond granularity. Do not infer a small-operation ranking from those rows.
Several cold samples exceed the best sample by more than 20%. These minimums describe this run; they set no acceptance threshold.

## Part B: Design questions

### B1. Script type and task record

Proposed rule: return an ordinary `Promise<T>` handle. Do not add a script source type.
The handle uses §70 counts, §116 exception observation, and §171 recursive result ownership.
A producer endpoint is a separate native capability. It owns one count until terminal delivery or adapter teardown.
A script cannot use this capability.
Generated host calls also need an async origin for the existing required-await check.
The handwritten foreign declaration alone does not supply that origin.

Evidence: `runtime/src/context/async_scheduler.rs` defines `AsyncFrameMeta`, `AsyncKind`, and `RuntimeTask`.
`Context::async_all` and `runtime/src/context/task_group.rs::Context::task_group_join` already register tasks without a generated resume body.
Their handles still have the count word at offset 4. They never enter the invocation-resume queue.
`Context::async_complete` queues their waiters. `Context::async_result_uncounted` raises their cached exception in the awaiter.

Add a `HostSource` runtime kind. Its record needs identity, result size, result ownership description, creation position, completion, and ordered waiters.
It also needs producer state, owner Context identity, and a terminal-delivery flag.
It needs no program counter, generated resume pointer, or function body.
Retain the common counted-handle storage for existing generated retain and release operations.

Proposed inspection rule: report host sources as kind 4, state `WAITING` before completion, and state `COMPLETE` after completion.
Their function and await positions are zero. Their creation position names the adapter call when generated metadata supplies it.
Include pending sources in `async_unfinished`. Exclude them from `async_pending` until completion queues a waiter.
Evidence: `runtime/src/context/async_inspection.rs::Context::visit_async_tasks` classifies runtime kinds as wait tasks.
At HEAD, `Context::async_unfinished` counts only kind 1. A new kind needs an explicit change there.

### B2. C declarations and generated bindings

Proposed rule: select completion declarations through explicit binder metadata. Do not recognize a header filename or operation-name suffix.
Carry the selected declaration, fulfilled C type, and adapter symbol in mirror provenance.
The generated adapter uses the existing boundary marshaler for its value. It adds no second value ABI.

| Header form | Binder selection and script result | Supported results |
| --- | --- | --- |
| `void read(int32_t id, HostI32Completion completion);` | Select the trailing producer parameter and its fulfilled type. Generate `read(id: i32): Promise<i32>`. | Scalars; a typed endpoint for each supported by-value struct or opaque handle; a separate void endpoint. |
| `HostI32Operation read(int32_t id);` | Select the typed operation result and its completion endpoints. Generate a wrapper that returns `Promise<i32>`. | The same scalar, struct, opaque-handle, and void set. The native token is separate from the fulfilled value. |
| A callback-info parameter with an explicit end | Select a typed C facade that converts this registration into a runtime source. Generate the facade as a Promise function. | The existing callback shape supplies a string view. Other results require a C facade; arbitrary callback signatures remain rejected. |

A struct result must satisfy the current boundary rules and the target ABI restrictions.
A `@ValueType` struct uses its ordinary C layout. An opaque handle uses its existing branded boundary type and ownership convention.
The source retains a managed reference result until the last result owner ends. It does not infer native retain/release functions from names.
Void carries no result bytes. Reject a result shape that the current boundary marshaler cannot represent.

Evidence: `bindgen/src/clangfe.rs` parses headers. `bindgen/src/emit.rs::emit_func` maps each C result through `map_use` today.
It does not derive async behavior from a token or completion parameter today.
`bindgen/src/callback_lifetime.rs::select` validates explicit callback-lifetime selections by aggregate identity.
`bindgen/src/emit.rs::emit_provenance` records boundary facts that the compiler cannot recover from TypeScript types alone.
`compiler/src/check/mirror_provenance.rs::Checker::collect_mirror_provenance` reads and validates these records.
`codegen/src/native.rs::NativeLibrary` supplies JIT symbols, C sources, and include directories as caller inputs.
Section 23 requires this header-independent path.

### B3. Token identity and cost

Proposed rule: use a 128-bit token `{ uint64_t context_id; uint64_t operation_id; }`.
Assign each Context incarnation a non-reused id. Assign operation ids monotonically within that Context.
Do not wrap either counter. Report exhaustion before another source starts.
An operation-id map resolves the live endpoint without a scan. Check the Context id before lookup.

The token occupies 16 bytes with alignment 8 on this host.
Delivery needs a Context-id comparison, one map lookup, and a terminal-state comparison.
The extra map entry is host storage, outside Context payload accounting. No measured total allocator cost supports a byte estimate for this map.

Evidence: `Context::async_register` assigns a non-reused u64 `task_id` and uses checked addition.
`Context::bump_reload_epoch`, in `runtime/src/context/lifecycle.rs`, uses a wrapping u32 epoch.
`Context::async_is_stale` compares that epoch with the creation epoch.
The epoch detects old code, not an operation that replaces another allocation. Do not use it as the producer token identity.

The prototype instead pairs a payload address with its non-reused task id. It occupies 16 bytes and uses the existing address map.
It rejects a changed id at the same address. It does not protect Context-address reuse after destruction.
The proposed Context id closes this limitation.

### B4. Completion functions and statuses

Proposed C surface:

```c
HostCompleteStatus host_complete_value(
    subscript_rt_context *ctx, HostToken token,
    const void *value, size_t size);
HostCompleteStatus host_complete_void(
    subscript_rt_context *ctx, HostToken token);
HostCompleteStatus host_complete_error(
    subscript_rt_context *ctx, HostToken token,
    const char *message, size_t length);
```

Generated typed wrappers supply the value size, boundary conversion, and ownership description.
The raw value entry is adapter infrastructure. Script-visible host functions retain their existing typed C declarations.

| Condition | Return | Effect |
| --- | --- | --- |
| Live pending source, outside script | `OK` | Cache one value or Error. Append waiters to the ready queue. Release producer ownership. |
| Wrong Context identity or ended endpoint | `STALE` | Change no state. Copy no payload. |
| Live terminal endpoint | `DUPLICATE` | Preserve the first completion. Copy no payload. |
| Active script call | `BUSY` | Change no state. The host retries outside the call. |

Check `BUSY` first. An endpoint record can survive completion until the host detaches its native callback.
After endpoint release, another delivery returns `STALE`, including a duplicate of a completed operation.
`OK` can coexist with an unobserved-exception Context trap under B5. The host must read the ordinary trap state.
If the Context already holds a trap, return `CONTEXT_TRAPPED` and change no source state.
An allocation failure reports the ordinary allocation trap. A production API also needs a distinct unsuccessful completion status for that case.

Evidence: `Context::script_depth` exposes the active-call depth.
`Context::async_complete` caches bytes and moves waiters without a script call.
`runtime/src/exception.rs::Context::async_complete_exception` caches an exception and moves the same waiters.
These internal functions do not validate a host token or an active host completion today.

### B5. Last script handle dropped

Proposed rule: producer ownership keeps a pending source alive after the last script holder ends.
A wait registration also owns its handle count. A source with a waiter therefore still has an observer.
Do not cancel native work merely because script holders end.

A later value completion caches the value, releases producer ownership, and frees the source if no holder or waiter remains.
Release any counted result through its existing ownership description.
A later failure does the same, but the last count traps if no await observed the Error.
Do not suppress this failure or invoke script from the completion call.

A producer cleanup inside a script-to-host call can release the last count of an already-failed source.
That release sets the Context trap. The generated host-call trap check stops the script after the C call returns.
A pending source cannot fail through the host completion API during that active call; B4 returns `BUSY`.

Evidence: `Context::async_release` keeps an unfinished task registered at count zero.
It frees a completed task at its last release and traps on `Completion::unobserved`.
`Context::async_release_runtime` releases counted completion bytes for `RuntimeTask::CountedInvocation`.
A host source needs the same result-description path. The i32 prototype has no counted result.

### B6. Destruction, reload, and collection

Proposed destruction rule: stop or detach every native notification before the Context dies.
Discard pending sources and waiter state without script execution.
For adapter teardown without Context destruction, detach notifications and complete the pending source with an Error outside an active script call.
Apply B5 if no script observer remains.
A token alone does not make a freed Context pointer safe. A host with late callbacks needs an adapter control block that outlives those callbacks.
That block marks the Context dead and rejects delivery before it accesses the pointer.

Proposed reload rule: a pending source keeps its identity across a body reload.
Its value and Error contain no generated resume code. A waiting invocation retains its original epoch and code lifetime.
If that invocation resumes after reload, apply the existing stale-frame trap before script effects.
If the host wants to replace that invocation, it must detach or terminate it through an explicit reload policy.
Do not reset the source epoch to make an old waiter executable.

Proposed collection rule: pending source records root their handles and their waiter graph.
Completed sources root their result or Error until their last owner ends.
`Context.collect()` changes neither native completion eligibility nor source identity.

Evidence: `runtime/src/context/memory.rs::Context::collect` roots all registered async handles, blocked jobs, and completion objects.
`Context::async_is_stale` keeps the old epoch test separate from cached completion.
`codegen/src/reload.rs::ReloadSession::reload` retains JIT generations that pending code can use.
`runtime/src/context.rs::Context::drop` destroys scheduler storage without a resume.

### B7. Interpreter host harness

Proposed rule: provide a typed, deterministic host interface to the interpreter.
Bind a host-operation descriptor through the same mirror metadata, rather than a machine address.
Implement the fake host with interpreter values and its independent task registry.
Add independent source creation and terminal delivery to that registry. Reuse no production scheduler.

The harness owns a checkpoint counter. It completes each request before the selected drain and records outputs, dispatches, and task records.
Run the same workload text, delay schedule, errors, repeated awaits, and stale-token cases in all three tiers.
Compare values, output bytes, checkpoint order, and resolved positions. Do not compare native pointers.
Callback tests need an interpreter callable plus rooted userdata and explicit registration release.

Evidence: `codegen/src/interpreter.rs::Interpreter::invoke_target` returns `InterpretError::Unsupported` for `CallTargetKind::Foreign`.
The current interpreter has no supplied native-library path.
`corpus/accept/a35-interop-async.ts` and the §111 callback entries explicitly exclude the interpreter.
The JIT uses `NativeLibrary` symbol addresses. C AOT compiles its declared C source and links the runtime archive.
A differential gate cannot admit these fake-host entries until the interpreter host interface exists.

### B8. Cancellation and deadlines

Proposed rule: a host can finish a pending source with an Error through the same terminal-delivery path.
A deadline uses the host clock. Cancellation uses the host adapter's decision to stop or detach native work.
The first terminal completion wins. Later value, error, cancellation, or deadline delivery returns the B4 status.
This requires no new script surface for basic cancellation failure.

A distinct `CancellationError`, a cancellation request token, and cooperative script checks are separate proposal-4 features.
An ordinary Error cannot provide that subtype test today.
A completion only resolves the source. It does not prove that native callbacks stopped.

Evidence: `Context::async_complete_exception` and `Context::async_result_uncounted` already implement Error delivery and observation.
`runtime/src/context/task_group.rs::Context::task_group_react` observes child failures through the cached completion.
Section 170 joins all group children and retains their observation counts. A cancellation Error can use this path.


## Part C: i32 completion-source prototype

The source exposes one native producer token and an ordinary script handle.
The temporary C declarations are:

```c
typedef struct {
    unsigned char *handle;
    uint64_t id;
} HostToken;
HostToken s178_source_new(void *ctx);
uint32_t s178_source_complete(
    void *ctx, HostToken token, int32_t value,
    const unsigned char *error, size_t length);
unsigned char *hostAsync(int32_t id);
```

The handwritten mirror carries the normal header provenance:

```ts
// @subscript-c-header include="host.h"
declare function hostAsync(id: i32): Promise<i32>;
```

A null error pointer selects the i32 value. A non-null pointer selects an Error with the supplied message.
The status codes are 0 `OK`, 1 `STALE`, 2 `DUPLICATE`, and 3 `BUSY`.
Creation registers a 16-byte counted handle as runtime kind 4. Its initial count is two: script ownership and producer ownership.
Terminal delivery queues waiters and releases producer ownership. Last release uses the existing unobserved-exception trap.
The prototype does not place the source itself in the resume queue.

The wrapper form uses the Part A task structure:

```ts
async function work(id: i32): Promise<void> {
  const handle = hostAsync(id);
  const result: i32 = await handle;
  hostConsume(id, result);
}
```

The direct form uses one main task and a handle array:

```ts
const N: i32 = 10000;

export async function main(): Promise<void> {
  const jobs: Promise<i32>[] = [];
  for (let i: i32 = 0; i < N; i++) {
    jobs.push(hostAsync(i));
  }
  for (let i: i32 = 0; i < N; i++) {
    hostConsume(i, await jobs[i]);
  }
}
```

Every source starts before the first await. All results complete at host step k.
The direct form dispatches the main task N times in one drain. It dispatches no job at the earlier checkpoints.

### C1. Measured costs

| Form | N | k | JIT ns/op | C AOT ns/op | Dispatches/op | Script host calls/op | Callback calls/op | Context requests/op |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| source | 1 | 1 | 2333.00 | 2000.00 | 2 | 1 | 0 | 5 |
| source | 1 | 10 | 2583.00 | 1000.00 | 2 | 1 | 0 | 5 |
| source | 100 | 1 | 492.08 | 300.00 | 2 | 1 | 0 | 2.08 |
| source | 100 | 10 | 527.92 | 310.00 | 2 | 1 | 0 | 2.08 |
| source | 10,000 | 1 | 453.97 | 224.30 | 2 | 1 | 0 | 2.0015 |
| source | 10,000 | 10 | 459.07 | 240.30 | 2 | 1 | 0 | 2.0015 |
| direct | 1 | 1 | 2250.00 | 2000.00 | 1 | 1 | 0 | 4 |
| direct | 1 | 10 | 2084.00 | 2000.00 | 1 | 1 | 0 | 4 |
| direct | 100 | 1 | 267.08 | 170.00 | 1 | 1 | 0 | 1.08 |
| direct | 100 | 10 | 287.08 | 180.00 | 1 | 1 | 0 | 1.08 |
| direct | 10,000 | 1 | 222.83 | 113.90 | 1 | 1 | 0 | 1.0015 |
| direct | 10,000 | 10 | 233.97 | 118.40 | 1 | 1 | 0 | 1.0015 |

All source rows have one drain of completion latency, with zero additional checkpoints.
The source and direct forms make one script-to-host start call per operation. They make no readiness or result-read call.
Each operation also needs one native call to `s178_source_complete`, which the script-host-call column excludes.
The completion calls remain inside the timed span.

### C2. Comparison

| N=10,000 | k | JIT source/best existing | C source/best existing | JIT source/per-task poll | C source/per-task poll |
| --- | ---: | ---: | ---: | ---: | ---: |
| Direct source | 1 | 18.061 | 10.745 | 1.100 | 1.239 |
| Direct source | 10 | 4.379 | 3.209 | 0.656 | 0.551 |

A ratio above one means that the source takes more time. The best existing form is the batch poll at N=10,000.
The direct source removes unsuccessful script wakeups and preserves per-operation await handles.
It remains slower than the batch poll for this cheap fake host.
At k=10, it takes less time than the per-task poll. At k=1, it takes more time.
The wrapper source adds N task allocations and N continuation dispatches over the direct source.
It supplies no measured advantage over the direct source in this workload.

All forms consume the result at the first drain after completion. The workload does not measure a polling latency penalty.
A different pump order or an explicit scheduler budget needs a separate measurement.

### C3. Tier requirements and observed failures

The JIT uses `ReloadSession::new_with_native_libraries` with Rust C-ABI functions for each host symbol.
A temporary `ReloadSession::s178_context` method exposes the Context pointer to the headless host.
The C host sets its current Context before the script call. It links the generated C with the release runtime archive.
Both fake hosts keep one request table. They increment the same step counter and complete requests before each drain.
The host table scans N entries at each of k checkpoints in every form. This scan cost remains inside each sample.

Neither tier needs a checker, HIR, LIR, or marshaler change for the stored-handle form.
The handwritten mirror can already declare an ordinary foreign function with a `Promise<i32>` result.
The compiler marks a call result as a fresh owner through `ExprKind::produces_fresh_async_owner` and `InstructionKind::produces_fresh_async_owner`.

The direct expression `await hostAsync(1)` fails with S100:
`hostAsync` is not a directly declared async function.
The stored form `const h = hostAsync(1); await h` passes the CLI check at HEAD.
This difference requires a compiler change before a generated Promise API can promise the direct-await spelling.
The CLI also accepts `export function main(): void { hostAsync(0); }` with this mirror.
This foreign-call form bypasses the §70 required-await diagnostic.
`compiler/src/check/expr/entry.rs::Checker::expr_async_origins` tracks async origins, rather than every foreign result with a Promise type.
A generated adapter must carry its async origin into that check. The prototype does not establish full §70 acceptance.

The first producer-release prototype used one count for both producer and script ownership.
Completion then freed the source before its await read. The early-await case reported `async resume without completion`.
The ordinary pending-await workload reported `use of a deleted allocation`.
Two initial counts and one producer release at terminal delivery close this prototype defect.

A kind-4 source needs inclusion in `Context::async_unfinished`. The HEAD predicate counts only invocation kind 1.
The prototype adds kind 4 to that predicate. Task inspection already classifies non-invocation kinds as wait tasks.

The Error path allocates the existing Error layout: a u32 tag, a name pointer, and a message pointer.
On this host, its size is 24 bytes and its class id is 0.
The prototype writes tag 0 and raises the Error through `Context::raise_exception` before it caches the exception.
This fixed layout is prototype code. A production adapter must use verified Error metadata and handle allocation failure.
The prototype does not guard an already-trapped Context. Its Error delivery cannot cache a new exception in that state.

The prototype has no void or reference-result endpoint. It has no cross-thread delivery or Context-incarnation token.
It has no separate terminal endpoint tombstone and no native cancellation hook.
An ended source therefore returns `STALE`, even for a repeated delivery of its earlier completion.
A live completed source returns `DUPLICATE`. These limits do not establish production acceptance.

### C4. Semantic observations

| Case | JIT | C AOT or C API | Observation |
| --- | --- | --- | --- |
| Completion after await | Pass, all workloads | Pass, all workloads | Exact checksum `7 * N`. |
| Completion before await | Pass | Pass, three Contexts | Await still defers and reads 7. |
| Shared and repeated await | Pass | Pass, three Contexts | Three reads each return 7. No source task remains after script scopes end. |
| Error completion | Pass | Pass, three Contexts | Catch recognizes `Error` and reads `Error:failure`. |
| Completion runs no continuation | Pass | Code path shared with C API | Result-consumption count stays zero until the checkpoint. |
| Collection with a blocked waiter | Pass | Pass, three Contexts | Source and waiter survive; the result remains 7. |
| Active script call | Pass | Pass | Completion returns 3 and preserves the pending source. |
| Duplicate completion | Pass | Pass | Live completed source returns 2. |
| Stale token | Pass | Pass | Ended source returns 1. |
| Replacement identity | Pass | Pass | An old id at a live replacement address returns 1. |
| Last script count ends before failure | Pass | Pass | Terminal delivery returns 0 and sets uncaught-exception trap 29. |
| Context destruction with pending source | Pass | Runtime destructor shared | Teardown runs no result-consumption callback. |
| Waiting invocation across reload | Pass | C has no reload tier | Completion queues the old waiter; its resume traps before result consumption. |
| Escaping callback failure | Pass | Shared runtime trampoline | Callback failure becomes a Context trap. |
| Interpreter native host call | Unsupported, measured | Not applicable | `hostAsync requires a native library`. |

The C API tests exercise active, duplicate, stale, replacement-id, and dropped-failure cases through the linked C ABI.
The C semantic host also requires zero registered tasks after each successful or caught-error run.
The replacement test supplies an old id with the new handle address. It does not claim that the allocator chose that address twice.
The collection tests use real generated waiters, not synthetic resume functions.
The reload test performs an accepted body reload while the source and its waiter remain pending.

Stock `tsc` accepts all five workload forms with strict ES2022 checks and the project prelude.
The CLI accepts temporary copies of a35 and a258. Their committed sources and goldens remain unchanged.
The interpreter rejects the prototype's foreign call before it can await the source.
The B7 harness remains a proposed interface; this round does not implement it.

### C5. Exact timed samples

Each row lists the three whole-workload times in execution order, in nanoseconds.
The counters match independently between JIT and C AOT for every sample.

| Form | N | k | JIT samples, ns | C AOT samples, ns |
| --- | ---: | ---: | --- | --- |
| poll | 1 | 1 | 10709 / 2542 / 1959 | 100000 / 5000 / 1000 |
| poll | 1 | 10 | 11667 / 3417 / 2583 | 16000 / 5000 / 2000 |
| poll | 100 | 1 | 36083 / 26417 / 24125 | 120000 / 18000 / 13000 |
| poll | 100 | 10 | 54000 / 42084 / 40792 | 56000 / 34000 / 29000 |
| poll | 10,000 | 1 | 2427375 / 2276833 / 2025584 | 1340000 / 931000 / 919000 |
| poll | 10,000 | 10 | 4634291 / 3715042 / 3567000 | 2597000 / 2170000 / 2149000 |
| callback | 1 | 1 | 13167 / 3708 / 3083 | 133000 / 3000 / 1000 |
| callback | 1 | 10 | 12958 / 3666 / 3125 | 20000 / 3000 / 2000 |
| callback | 100 | 1 | 56458 / 45750 / 43750 | 166000 / 29000 / 22000 |
| callback | 100 | 10 | 78666 / 65834 / 79458 | 73000 / 44000 / 52000 |
| callback | 10,000 | 1 | 5154667 / 4493125 / 4270208 | 2135000 / 1665000 / 1608000 |
| callback | 10,000 | 10 | 7220667 / 6299541 / 6373250 | 3650000 / 3193000 / 3131000 |
| batch | 1 | 1 | 9167 / 1792 / 1208 | 100000 / 2000 / 2000 |
| batch | 1 | 10 | 10625 / 2375 / 1958 | 20000 / 3000 / 2000 |
| batch | 100 | 1 | 9083 / 3750 / 3458 | 111000 / 6000 / 4000 |
| batch | 100 | 10 | 16375 / 8625 / 8417 | 23000 / 8000 / 7000 |
| batch | 10,000 | 1 | 137875 / 123375 / 126792 | 253000 / 108000 / 106000 |
| batch | 10,000 | 10 | 607791 / 546458 / 534292 | 411000 / 369000 / 392000 |
| source | 1 | 1 | 9083 / 2667 / 2333 | 110000 / 3000 / 2000 |
| source | 1 | 10 | 11833 / 3250 / 2583 | 18000 / 6000 / 1000 |
| source | 100 | 1 | 64584 / 51334 / 49208 | 152000 / 38000 / 30000 |
| source | 100 | 10 | 66375 / 52792 / 52834 | 65000 / 38000 / 31000 |
| source | 10,000 | 1 | 5929625 / 4740458 / 4539666 | 3437000 / 2371000 / 2243000 |
| source | 10,000 | 10 | 7503125 / 5420667 / 4590708 | 3413000 / 2492000 / 2403000 |
| direct | 1 | 1 | 10125 / 3584 / 2250 | 102000 / 2000 / 2000 |
| direct | 1 | 10 | 11041 / 2959 / 2084 | 16000 / 3000 / 2000 |
| direct | 100 | 1 | 34708 / 26708 / 27792 | 125000 / 21000 / 17000 |
| direct | 100 | 10 | 38000 / 28708 / 30000 | 43000 / 22000 / 18000 |
| direct | 10,000 | 1 | 2744750 / 2297750 / 2228291 | 1734000 / 1225000 / 1139000 |
| direct | 10,000 | 10 | 3197084 / 2445625 / 2339750 | 1658000 / 1259000 / 1184000 |
