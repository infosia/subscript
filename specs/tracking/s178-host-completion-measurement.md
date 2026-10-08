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
The B7 harness remains a proposed interface.

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


## Implementation: runtime

A process-wide atomic counter assigns Context identities. Zero identifies no Context.
Counter exhaustion creates a trapped Context. No valid identity repeats.
Each Context uses an operation-id map. Completion never scans registered tasks.
A source starts with a script count and a producer count.
Completion caches its value or Error, queues its waiters, and releases the producer count.
No completion checks script depth or runs script code.
The existing collector roots sources, waiters, and cached Error objects.
The existing destructor discards pending state without a resume.

### Generated-code entry

```c
uint8_t* subscript_rt_async_host_operation(
    subscript_rt_context* ctx,
    uint64_t result_size,
    uint32_t is_void,
    uint32_t pos_id,
    const uint64_t* error_metadata,
    subscript_rt_completion* endpoint);
```

`is_void` uses zero for a value and a nonzero value for void.
A void source requires a zero result size.
The entry copies six aligned native-endian metadata words:
payload size, class id, kind-field offset, name-field offset, message-field offset, and Error kind tag.
The generated caller must derive these words from the verified Error class.
The runtime checks field bounds, field alignment, field overlap, and integer ranges.
Each source keeps its metadata copy, including across reload.
The entry traps on operation-id exhaustion or source allocation failure.
A trapping entry returns null and leaves the endpoint unchanged.

The runtime had no Error layout metadata input at the pin.
The Error allocator uses these words; it assumes no fixed class id or object layout.
The tests use a separate C-layout Error fixture with a nonzero class id and nonstandard field offsets.

### Changed files

| File | Change |
| --- | --- |
| `runtime/src/context.rs` | Context identity and operation registry; new module exports. |
| `runtime/src/context/lifecycle.rs` | Process-unique identity at Context creation. |
| `runtime/src/context/host_operation.rs` | Endpoint, status, source creation, lookup, and completion. |
| `runtime/src/context/async_scheduler.rs` | Kind 4, producer release cleanup, and unfinished count. |
| `runtime/src/context/async_inspection.rs` | Kind 4 and zero function/await positions. |
| `runtime/src/exception.rs` | Error allocation module. |
| `runtime/src/exception/host_error.rs` | Error allocation from verified metadata, with creation position. |
| `runtime/src/ffi.rs` | Three public host completion entries. |
| `runtime/src/ffi/async_frames.rs` | Internal source creation entry. |
| `runtime/src/host_header.rs` | Endpoint, status, size types, completion declarations, and C documentation. |
| `runtime/include/subscript_runtime.h` | Generated host header. |
| `runtime/tests/host_operation.rs` | Eleven runtime tests. |
| `specs/tracking/s178-host-completion-measurement.md` | Implementation results and costs. |

No additional file needs a change.
The host-header generator produced the header; no manual header edit occurred.
`cargo run -p subscript-compiler --bin generate-api-reference` regenerated all three documents with byte-identical results.

### Verification

The tests cover each status with a matching successful control.
They also cover before/after await, shared/repeated await, Error observation and position, producer lifetime, collection, destruction, and inspection.
Additional controls cover active script depth, an existing pending exception, boundary structs, ship allocation, and identity across reload.
All new Rust files have fewer than 2,000 lines.

### New test costs

Each test creates at most 24 sources and dispatches at most nine waiter resumes.
No test invokes a compiler, native build, or external process.
The table gives the median of three separate executions of each compiled debug test.
Times include process startup and the test harness. They are not scheduler timings.

| Test | Median ms |
| --- | ---: |
| `active_script_completion_only_queues_and_collection_preserves_waiters` | 3.490 |
| `before_after_shared_and_repeated_awaits_copy_cached_values` | 3.548 |
| `boundary_struct_bytes_and_ship_collection_controls` | 3.621 |
| `completion_during_script_preserves_a_pending_exception` | 3.532 |
| `destruction_with_a_pending_waiter_runs_no_script` | 3.485 |
| `each_mismatch_keeps_the_source_pending_with_ok_controls` | 3.495 |
| `error_metadata_message_position_and_observed_drop_control` | 3.512 |
| `inspection_waiting_complete_and_pending_destruction_control` | 6.558 |
| `producer_keeps_a_dropped_handle_until_value_or_unobserved_error` | 3.410 |
| `source_and_error_allocation_failure_have_same_shape_ok_controls` | 3.497 |
| `statuses_context_identity_and_check_order` | 14.167 |

### Completion path cost

A temporary release harness creates 10,000 i32 sources, completes them, and releases their script holders.
It runs three samples for each Context allocation tier.
The creation column includes endpoint storage. The completion column includes the value copy and producer release.
These sources have no waiters. Each sample ends with zero live allocations and no trap.

| Allocation tier | Creation ns/op, three samples | Completion ns/op, three samples | Holder release ns/op, three samples |
| --- | --- | --- | --- |
| Development | 166.62 / 128.62 / 124.66 | 71.19 / 57.85 / 59.59 | 130.03 / 125.95 / 129.60 |
| Ship | 73.00 / 72.21 / 70.84 | 58.56 / 56.15 / 56.10 | 72.67 / 73.45 / 73.12 |


## Implementation: binder

`subscript bind --completion <function>=<result>` selects each completion function. The option is repeatable.
`BindOptions::with_completion` supplies the same selection to library callers.
The validator rejects absent functions, absent trailing endpoints, non-void returns, non-trailing endpoints, duplicate selections, and unsupported results.
A function with an endpoint parameter requires a selection. Each diagnostic names the function.
Only a trailing by-value `subscript_rt_completion` parameter qualifies as an endpoint.

The result accepts existing mapped C scalar spellings, numeric typedef aliases, enum typedefs, boundary-class structs, and `void`.
Opaque handles, pointers, strings, absorbed descriptors, external types, unknown names, and the runtime endpoint fail rule 6.
Existing boundary validation still checks the emitted classes and the remaining parameters.
The mirror removes the endpoint and returns `Promise<T>`.
It emits one directive per selected function, in C declaration order:

```text
// @subscript-c-completion function="read" result="int32_t"
```

Both quoted fields use the existing JSON-compatible provenance escaping.
The result field preserves the selected C spelling for the size check and C adapter.
Selection order does not change mirror order.

### Type resolution

An undeclared host type produces a libclang parse error unless an external directive supplies its boundary identity.
The runtime endpoint uses its exact ABI name; no host header filename or function suffix selects completion behavior.
If libclang reports an unknown `subscript_rt_completion`, the parser retries with a private forward typedef.
The parser removes this synthetic declaration before emission. It does not infer or reproduce the runtime layout.
Other undeclared host types still fail. Headers that already declare the runtime type keep their original parse.
The emitter does not expose the endpoint as a script class.

### Changed files

| File | Change |
| --- | --- |
| `cli/src/lib.rs` | Repeatable completion option and binder inputs. |
| `bindgen/src/lib.rs` | Completion options, builder, and public documentation. |
| `bindgen/src/completion.rs` | Function, endpoint, duplicate, and result validation. |
| `bindgen/src/emit.rs` | Promise signatures and completion provenance. |
| `bindgen/src/clangfe.rs` | Private resolution of an undeclared runtime endpoint. |
| `bindgen/src/emit/tests.rs` | Existing emitter tests moved without behavior changes. |
| `bindgen/tests/completion.rs` | Thirteen binder gates, including stock TypeScript acceptance. |
| `cli/tests/bind_completion.rs` | Three CLI gates for output, rejection, and usage errors. |
| `specs/tracking/s178-host-completion-measurement.md` | Binder evidence and test costs. |

### Verification

### New gate costs

Each row is one warm test-binary invocation with `--exact <test>`, measured on 2026-10-08.
Wall time includes process startup and excludes compilation. Each invocation passed.
All header parses use in-memory libclang input. Only the TypeScript test starts tsc.
That process checks invariant 5 against an independent type checker.

| Suite | Test | Work | Wall time (s) |
| --- | --- | --- | ---: |
| `completion` | `absent_function_is_rejected` | 2 parses | 0.017 |
| `completion` | `duplicate_selection_is_rejected` | 2 parses | 0.017 |
| `completion` | `emitted_mirror_type_checks_with_stock_tsc_and_project_prelude` | 1 parse; 1 tsc process | 0.216 |
| `completion` | `endpoint_without_selection_is_rejected` | 2 parses | 0.018 |
| `completion` | `external_result_does_not_map_to_a_boundary_class` | 4 parses; external preamble fallback | 0.023 |
| `completion` | `invalid_results_are_rejected` | 10 parses; 9 rejected result forms | 0.043 |
| `completion` | `missing_endpoint_is_rejected` | 2 parses | 0.017 |
| `completion` | `non_trailing_endpoint_is_rejected` | 4 parses; selected and unselected endpoints | 0.023 |
| `completion` | `non_void_return_is_rejected` | 2 parses | 0.017 |
| `completion` | `pointer_endpoint_is_rejected` | 2 parses | 0.016 |
| `completion` | `scalar_struct_and_void_mirrors_carry_exact_provenance` | 6 parses; 6 result spellings | 0.030 |
| `completion` | `two_endpoints_are_rejected` | 2 parses | 0.017 |
| `completion` | `unknown_runtime_endpoint_resolves_but_other_unknown_types_fail` | 5 parses; 2 runtime preamble fallbacks | 0.018 |
| `bind_completion` | `invalid_selections_exit_one_without_output` | 3 CLI processes; 6 parses | 0.134 |
| `bind_completion` | `missing_or_malformed_value_exits_two` | 4 CLI processes; no parse | 0.015 |
| `bind_completion` | `repeated_options_reach_stdout_and_file` | 2 CLI processes; 4 parses | 0.028 |


## Implementation: checker and LIR

The provenance parser reads one completion directive per foreign function.
The checker rejects absent functions, repeated directives, non-Promise results, mismatched results, and results outside rule 6.
The accepted results include scalar typedef aliases, boundary value classes, and void.

Rule 12 uses the existing unsupported boundary-return site `ForeignReturnProvenance`, with code S100.
Its message for `read` is:

```text
foreign function `read` returns Promise<T> without a `@subscript-c-completion` directive
```

A completion call uses the existing `AsyncHandleTransfer` HIR wrapper to carry its async origin.
`ExprKind::Call` and `AsyncHandleTransfer` already report a fresh async owner.
Direct `await read()` uses the ordinary handle-await path.
A held handle that an await observes passes; a dropped call receives S013.
A local holder that never reaches an await also receives S013 under the existing §70 rule.

### Shared form

`InstructionKind::HostCompletion` carries these fields:

| Field | Fact |
| --- | --- |
| `function` | The foreign C function id. |
| `result_size` | The result payload size from the target layout. |
| `is_void` | The selected result has no payload. |
| `creation_pos` | The script call position. |
| `error_metadata` | Six words: Error payload size, class id, kind/name/message offsets, and Error kind tag. |

`Instruction::operands` carries the argument values in call order.
The foreign declaration retains the C result spelling and the script result type.
Its C return is void, and its last parameter carries `CompletionEndpoint` provenance for by-value `subscript_rt_completion`.
The new instruction reports a fresh async owner and retains allocation and call trap sites.
The text printer emits the form, completion result provenance, and endpoint parameter.

The verifier compares operands against the foreign declaration and requires the trailing endpoint and void C return.
It compares the payload size against the target layout and the metadata against the Error class fields.
It also checks the result type, void flag, result set.
The tests construct an endpoint-free C declaration without changing the instruction.
Separate invalid forms test the size, void flag, metadata, and operands.

### Changed files

| File | Change |
| --- | --- |
| `compiler/src/provenance.rs` | Completion directive parser and repeated-function check. |
| `compiler/src/check/mirror_provenance.rs` | Directive target and result validation; rule 12. |
| `compiler/src/check/signatures.rs` | Completion provenance on foreign definitions. |
| `compiler/src/check/expr/call.rs` | Completion calls acquire an async origin. |
| `compiler/src/check/expr/entry.rs` | Direct completion awaits use the handle path. |
| `compiler/src/hir.rs` | Selected C result spelling on foreign functions. |
| `compiler/src/hir/sites.rs` | Completion source allocation trap. |
| `compiler/src/lir.rs` | HostCompletion form, endpoint provenance, and fresh ownership. |
| `compiler/src/lir_text.rs` | Completion declaration provenance in review text. |
| `compiler/tests/host_completion.rs` | Six checker and stock tsc gates. |
| `codegen/src/lir.rs` | Completion module and existing test declaration initialization. |
| `codegen/src/lir/completion.rs` | Target layout facts, Error metadata, and verifier checks. |
| `codegen/src/lir/call.rs` | Shared completion instruction emission. |
| `codegen/src/lir/lowering.rs` | Completion C signature and endpoint parameter. |
| `codegen/src/lir/verify_instruction.rs` | Completion contract dispatch. |
| `codegen/src/lir/verify_lifetime.rs` | Foreign call lifetime convention. |
| `codegen/src/lir/verify_narrowing.rs` | Completion form classification. |
| `codegen/src/interpreter/instruction.rs` | Unsupported native-call result. |
| `codegen/src/lower/func/instruction.rs` | Dev JIT completion lowering. |
| `codegen/src/cemit/graph.rs` | C AOT completion emission. |
| `codegen/tests/lir_completion.rs` | Six argument, layout, verifier, interpreter, and stock tsc gates. |
| `codegen/tests/support/lir_facts.rs` | New form arity in the existing exhaustive fact checker. |
| `specs/tracking/s178-host-completion-measurement.md` | Implementation evidence and gate costs. |

### Verification

### New gate costs

Each row gives the median of three warm debug test-binary invocations with `--exact <test>`.
The samples exclude compilation and include process startup. Each invocation passed.
The compiler and native build checks finished before these samples.
Only the two stock tsc tests start an external process; each starts one tsc process.
These processes check the TypeScript invariant with an independent checker.
Other tests use in-memory source checks, lowering, verification, or the interpreter's unsupported path.

| Suite | Test | Work | Median ms |
| --- | --- | --- | ---: |
| `host_completion` | `direct_await_and_held_handle_are_async_origins` | 3 mirror/script checks | 5.368 |
| `host_completion` | `directives_reject_absent_duplicate_and_mismatched_results` | 7 mirror/script checks | 5.688 |
| `host_completion` | `dropped_call_has_s70_diagnostic_with_held_control` | 2 mirror/script checks | 4.682 |
| `host_completion` | `promise_return_requires_completion_directive_with_same_signature_control` | 2 mirror/script checks | 4.202 |
| `host_completion` | `scalar_alias_struct_and_void_results_match_the_directive` | 5 mirror/script checks | 5.746 |
| `host_completion` | `stock_tsc_checks_every_checker_input_with_the_prelude` | 17 inputs; 1 tsc process | 215.291 |
| `lir_completion` | `completion_arguments_keep_foreign_string_and_array_provenance` | 1 check and lowering; 1 additional verification | 6.791 |
| `lir_completion` | `completion_form_carries_scalar_struct_void_layout_and_error_metadata` | 1 check and lowering; 1 additional verification; text output | 6.895 |
| `lir_completion` | `interpreter_rejects_completion_with_the_foreign_unsupported_path` | 2 checks and lowerings; 2 unsupported interpreter runs | 7.544 |
| `lir_completion` | `stock_tsc_checks_lir_inputs_with_the_prelude` | 3 inputs; 1 tsc process | 219.095 |
| `lir_completion` | `verifier_checks_payload_size_void_flag_metadata_and_operands` | 1 check and lowering; 5 additional verifications | 8.576 |
| `lir_completion` | `verifier_rejects_a_foreign_declaration_without_an_endpoint` | 1 check and lowering; 2 additional verifications | 7.630 |

## Implementation: tiers and corpus

Shared interop discovery selects generated mirrors and ambient dependencies for every harness; `a347` and `t108` use valid `C8 Q13` headers.

Both native tiers create a source through `subscript_rt_async_host_operation`.
They supply the verified result size, void flag, creation position, and six Error metadata words.
They check allocation failure before the foreign call.
They pass the endpoint as the last by-value C argument, then check the Context trap.
The instruction result is the ordinary async handle.
The dev JIT uses integer images on AAPCS64 and SysV, with ABI stack fallback, and a pointer to a copy on Win64.
SysV uses a 16-byte stack argument when fewer than two integer argument registers remain.
An ABI unit test verifies each plan and the SysV register-pressure fallback.

### Corpus and mirrors

The new `host-completion.h` and `host-completion.c` form a second synthetic library.
Its generated mirror shares the existing external `SubDevice` type.
The device supplies the Context that its callback registration adopted.
A thread-local table stores endpoints by device and request id.
The pump completes scalar, boundary struct, void, or Error results.
The immediate function completes before its start call returns.

The CLI generated the mirror with this command:

```sh
subscript bind corpus/interop/host-completion.h \
  --completion subCompletionI32=int32_t \
  --completion subCompletionStruct=SubCompletionValue \
  --completion subCompletionVoid=void \
  --completion subCompletionImmediate=int32_t \
  -o corpus/interop/host-completion.generated.d.ts
```

No generated mirror received a manual edit.
A binder test compares the committed completion mirror with independent regeneration.
The existing interop mirror remains byte-identical.

`a347` covers pending, immediate, shared, repeated, aggregate, Error, struct, and void completion.
It collects while two waiters await the same source.
Both native tiers produce its exact golden output.
`t108` drops the script holder before the Error pump.
Both tiers trap with rule 29 at `12:15`, with message `Error: host failure` and no stdout.
`r397` is an ambient mirror probe, selected by `corpus-ambient: yes`.
Its gate supplies an empty script entry and verifies the exact missing-directive diagnostic.
Stock TypeScript 5.9.2 accepts all three entries with the project prelude.
The entries use existing C8 and Q13 collisions. The collision table needs no change.

### Pin verification

| Entry | Dev JIT at 7edb874e | C AOT at 7edb874e | Interpreter at 7edb874e |
| --- | --- | --- | --- |
| a347 | S100: unknown provenance record kind `completion`; no stdout | Same rejection | Excluded: native interop |
| t108 | S100: unknown provenance record kind `completion`; no stdout | Same rejection | Excluded: native interop |
| r397 | Accepted; empty stdout | Accepted; empty stdout | Accepted; empty stdout |

The release CLI independently verifies both completion mirror rejections and the accepted r397 mirror.
The tier driver verifies r397 execution through all three tiers.

### Reload and shared facts

The reload test starts a waiting invocation, changes its body, then completes its original host source.
The pump returns status zero after the reload.
The old waiter traps with `stale coroutine after reload` before any script output.
This verifies rule 10 without a new interpreter path.

### Goldens and additional files

Release `generate-api-reference` regenerated all three documents.
The API reference remains byte-identical.
The corpus index adds the three entries.
The language reference selects r397 as its S100 example.
The LIR text golden adds only a347, with 1,148 lines.
Its async functions select it for the existing text gate.
No existing `.expected` file changes.

These support files connect the tier implementations to the standing gates:

- `codegen/src/lower/mod.rs`: declare the runtime entry signature for the JIT.
- `codegen/tests/native-fixture/build.rs`: compile the second C library for native tests.
- `bindgen/tests/completion.rs`: verify byte-identical regeneration of the new committed mirror.
- `compiler/tests/corpus_accept.rs`, `corpus_reject.rs`, and `corpus/mod.rs`: load the second mirror and verify the ambient rejection.

### Verification

### New gate costs

Each row gives the median of three warm debug test-binary invocations.
The samples include process startup and exclude compilation.
The invocations ran after the release performance suite, one binary at a time.

| Test | Work | Median ms |
| --- | --- | ---: |
| `host_completion_waiter_across_reload_traps_before_script_effects` | Two JIT generations; source creation, completion, and stale resume | 91.299 |
| `committed_host_completion_mirror_matches_regeneration` | One in-memory header parse and mirror comparison | 15.835 |
| `foreign_promise_rejection_names_the_missing_completion_directive` | One ambient mirror check | 4.402 |
| `completion_endpoint_uses_target_abi_and_sysv_stack_fallback` | Four ABI plans; no compiler or native process | 4.046 |

The a347 corpus witness took 1.42 seconds in an isolated debug run through both native tiers.
The standing golden sweep covers it; no duplicate completion test remains in that sweep.
The existing trap sweep covers t108 and took 24.75 seconds for all 105 paired trap entries, with two existing exclusions.
The interpreter exclusion follows the other native interop entries.

### Rule 7 path cost

The direct Part C form starts 10,000 sources before the first await.
Every source completes at host step k. Each operation has one start and one consume call.
The timed span includes source creation, all native value completions, all waiter resumes, and script holder release.
Context creation, setup, JIT compilation, native compilation, and Context destruction remain outside the timed span.
Each run verifies checksum 49,995,000 and status zero for all completions.
The JIT also verifies zero unfinished tasks. Neither tier reports a trap.
Each binary runs alone. The table gives all three release samples and their minimum.

| Tier | N | k | Samples ns/op | Best ns/op |
| --- | ---: | ---: | --- | ---: |
| Dev JIT | 10,000 | 1 | 453.57 / 395.80 / 393.40 | 393.40 |
| Dev JIT | 10,000 | 10 | 396.73 / 393.72 / 398.40 | 393.72 |
| C AOT | 10,000 | 1 | 296.50 / 262.00 / 256.60 | 256.60 |
| C AOT | 10,000 | 10 | 269.90 / 264.00 / 269.10 | 264.00 |

The temporary host stores endpoints and due steps in an indexed table.
It scans all 10,000 requests at every checkpoint and completes them outside the script.
Each scan remains inside the timed span, as Part C requires.
Results carry the request id; the checksum verifies each value.
It uses the production adapter and completion API, with no polling function or prototype source implementation.
The measurement source follows Part C's direct form.

### Existing workload costs

Release binaries at `77593125` and the current tree each run three times, one binary at a time.
`async-cost` uses its three workloads. `cross-language --only <workload>` covers each of its ten workloads.
Each invocation uses the harness defaults: at least three warm-ups, a 200 ms warm-up floor, and eleven timed samples.
The table selects the smallest reported median from three invocations.
Noise-invalid subject timings remain excluded. Samples gives the number of valid pin/current subject reports.
External detector failures do not replace a native tier result.

| Workload | Tier | Pin best ms | Current best ms | Ratio | Samples |
| --- | --- | ---: | ---: | ---: | --- |
| settled-awaits | C AOT | 17.223 | 17.276 | 1.003 | 3/3 |
| held-handles | C AOT | 4.758 | 4.824 | 1.014 | 3/3 |
| deep-chains | C AOT | 12.032 | 10.886 | 0.905 | 3/3 |
| fib-recursive | C AOT | 4.411 | 4.247 | 0.963 | 3/3 |
| fib-recursive | Dev JIT | 9.256 | 10.000 | 1.080 | 2/3 |
| fib-loop | C AOT | 34.196 | 36.028 | 1.054 | 3/1 |
| fib-loop | Dev JIT | 80.024 | 87.876 | 1.098 | 2/2 |
| mandelbrot | C AOT | 164.791 | 146.587 | 0.890 | 3/3 |
| mandelbrot | Dev JIT | 178.141 | 153.251 | 0.860 | 3/3 |
| primes | C AOT | 23.949 | 23.447 | 0.979 | 3/2 |
| primes | Dev JIT | 35.961 | 35.888 | 0.998 | 3/2 |
| sort | C AOT | 19.788 | 19.797 | 1.000 | 3/3 |
| sort | Dev JIT | 37.793 | 37.910 | 1.003 | 3/3 |
| tree | C AOT | 116.486 | 119.883 | 1.029 | 3/3 |
| tree | Dev JIT | 464.018 | 464.572 | 1.001 | 3/3 |
| queen | C AOT | 28.665 | 28.264 | 0.986 | 3/3 |
| queen | Dev JIT | 39.560 | 40.062 | 1.013 | 3/3 |
| particles | C AOT | 88.206 | 87.941 | 0.997 | 3/3 |
| particles | Dev JIT | 522.302 | 515.247 | 0.986 | 3/2 |
| callbacks | C AOT | 41.765 | 41.767 | 1.000 | 2/3 |
| callbacks | Dev JIT | 301.897 | 306.411 | 1.015 | 3/3 |
| collect | C AOT | 38.715 | 38.965 | 1.006 | 3/3 |
| collect | Dev JIT | 126.482 | 125.796 | 0.995 | 3/3 |

| Follow-up workload | Tier | Pin medians ms | Current medians ms | Best ratio |
| --- | --- | --- | --- | ---: |
| fib-recursive | Dev JIT | 9.666 / 9.633 / 9.632 | 9.672 / 9.635 / 9.642 | 1.000 |
| fib-loop | Dev JIT | 85.892 / 83.289 / 82.355 | 84.701 / 82.941 / 79.525 | 0.966 |
| fib-loop | C AOT | 41.029 / 42.326 / 36.649 | noise-invalid / 36.939 / 36.777 | 1.003 |

The initial threshold exceedances remain in the first table; their cause is not established.
`tools/hygiene.sh` and `git diff --check` pass after the implementation.


## Implementation: review fixes

AAPCS64 C.13 sends a composite to the stack when its integer images exceed the remaining GPRs.
The planner inserts one unused I64 argument per remaining GPR, then passes the complete composite as I64 stack images.
Every later integer argument therefore also uses the stack.
The same planner serves completion endpoints and ordinary boundary structs of at most 16 bytes.
SysV retains its MEMORY fallback; Win64 uses a caller-copy pointer for a 16-byte struct.
Unit tests cover counts zero through eight for all three plans.

The native gate compiles C callees with the platform compiler.
Each callee checks every earlier int32 argument, both 64-bit struct words, and the following int32 argument.
Both native tiers pass all eighteen endpoint and ordinary-struct cases, with earlier counts zero through eight.
Two additional callees check a twelve-byte struct after eight and nine int32 arguments.
Apple clang places that composite at stack offset eight after the ninth int32, and its following int32 at offset twenty-four.
Cranelift packs surrounding Apple scalar stack arguments naturally; I64 images supply the required composite alignment and rounding.
The pressure-blind plan fails exactly the seven-earlier-argument endpoint and ordinary-struct cases on this host.
The repaired plan passes them.
A real completion after seven int32 arguments returns 140 in both tiers; its pending-source control returns status zero and value 91.

The binder rejects a struct field unless it is a scalar or a recursively eligible struct.
Its diagnostic names the completion function, struct, and field.
The checker independently derives the same result from resolved class fields and mirror provenance.
Both reject string views, absorbed pointer/count fields, pointers, callbacks, and wire aliases, with scalar-layout controls.
The checker also rejects a direct CEnum alias completion result.

HostCompletion uses the instruction position directly.
Its void flag derives from the C result directive; the verifier compares it against the independently declared Promise element type.
C AOT Error metadata uses sizeof and offsetof on the emitted Error struct.
Error allocation rollback releases the object and name when a later allocation fails.
The rollback test covers all three allocation failures and a successful control.
The reload control completes without a reload and prints status zero and value 91.
The public C header states that the message contains UTF-8 bytes and that the runtime copies them.

A release CLI built from 7edb874e checks the three updated pin entries.
Check and C emission reject a347 and t108 with S100 for the unknown completion directive.
Both accept the r397 ambient mirror with an empty script entry.
Their recorded pin results remain unchanged.

### Verification

- cargo fmt --check: pass.
- cargo clippy --workspace --all-targets: pass with existing warnings.
- Binder, compiler, and runtime suites: 1,691 passed, zero failed, two ignored.
- CLI suite: 65 passed, zero failed.
- Codegen suite: 860 passed, zero failed, one ignored.
- The codegen suite includes 105 C emission gates, 35 golden gates, 57 LIR gates, and 37 reload gates.
- Final ABI unit checks: twelve passed.
- Final native pressure gates: two passed; the twenty native callee cases and the seven-argument completion agree in both tiers.
- Final native interop gates: twenty-two passed.
- The generated LIR text comparison passes after regeneration.

### New gate cost

Each cost is one isolated warm debug test-binary invocation, including startup and excluding the Rust build.
Each invocation passes. Native tests include the C AOT compilation they require.

| Suite | Test | Wall ms |
| --- | --- | ---: |
| abi_pressure | `native_composites_obey_register_pressure_for_zero_through_eight_arguments` | 830.406 |
| abi_pressure | `completion_after_seven_integer_arguments_in_both_tiers` | 704.713 |
| completion | `completion_struct_fields_require_recursive_scalar_layouts` | 47.021 |
| completion | `completion_rejects_callbacks_and_wire_aliases_with_scalar_controls` | 24.353 |
| host_completion | `completion_struct_fields_require_recursive_scalar_layouts` | 27.746 |
| subscript_runtime | `exception::host_error::tests::later_allocation_failure_releases_each_earlier_allocation` | 12.747 |
| subscript_codegen | `lower::func::abi::aggregate_abi_tests::aapcs64_c13_places_the_whole_composite_on_the_stack` | 4.561 |

### Fib cost after review fixes

Each workload and tier uses five interleaved pin/current pairs.
Each revision runs three times per pair, with one binary active at a time.
Each invocation discards at least three warm-ups and at least 200 ms, then records eleven timed samples.
The table records the three invocation medians and selects the smallest median in each pair.
No result causes a repeated measurement.

| Workload | Tier | Pair | Pin medians ms | Current medians ms | Best ratio |
| --- | --- | ---: | --- | --- | ---: |
| fib-recursive | Dev JIT | 1 | 9.452 / 9.543 / 9.443 | 9.346 / 9.408 / 9.629 | 0.990 |
| fib-recursive | Dev JIT | 2 | 9.439 / 9.444 / 9.451 | 9.510 / 9.573 / 9.563 | 1.008 |
| fib-recursive | Dev JIT | 3 | 9.632 / 9.581 / 9.467 | 9.445 / 9.553 / 9.380 | 0.991 |
| fib-recursive | Dev JIT | 4 | 9.412 / 9.360 / 9.387 | 9.319 / 9.314 / 9.257 | 0.989 |
| fib-recursive | Dev JIT | 5 | 9.334 / 9.287 / 9.388 | 9.253 / 9.454 / 9.290 | 0.996 |
| fib-recursive | C AOT | 1 | 4.235 / 4.379 / 4.415 | 4.236 / 4.413 / 4.414 | 1.000 |
| fib-recursive | C AOT | 2 | 4.410 / 4.292 / 4.410 | 4.413 / 4.414 / 4.249 | 0.990 |
| fib-recursive | C AOT | 3 | 4.358 / 4.281 / 4.247 | 4.410 / 4.258 / 4.243 | 0.999 |
| fib-recursive | C AOT | 4 | 4.319 / 4.310 / 4.410 | 4.396 / 4.414 / 4.413 | 1.020 |
| fib-recursive | C AOT | 5 | 4.390 / 4.327 / 4.326 | 4.331 / 4.405 / 4.285 | 0.991 |
| fib-loop | Dev JIT | 1 | 81.649 / 82.352 / 82.727 | 81.636 / 81.096 / 81.329 | 0.993 |
| fib-loop | Dev JIT | 2 | 82.295 / 81.256 / 81.428 | 80.807 / 80.987 / 80.779 | 0.994 |
| fib-loop | Dev JIT | 3 | 80.528 / 80.303 / 88.383 | 84.885 / 86.391 / 81.746 | 1.018 |
| fib-loop | Dev JIT | 4 | 80.492 / 80.061 / 80.055 | 80.157 / 79.878 / 80.076 | 0.998 |
| fib-loop | Dev JIT | 5 | 80.268 / 79.780 / 80.981 | 79.345 / 79.649 / 79.953 | 0.995 |
| fib-loop | C AOT | 1 | 33.508 / 33.495 / 34.002 | 33.967 / 34.454 / 32.803 | 0.979 |
| fib-loop | C AOT | 2 | 32.822 / 34.131 / 33.855 | 33.487 / 33.950 / 33.822 | 1.020 |
| fib-loop | C AOT | 3 | 34.343 / 32.831 / 33.661 | 33.471 / 34.155 / 34.375 | 1.019 |
| fib-loop | C AOT | 4 | 32.842 / 32.809 / 33.373 | 33.743 / 33.885 / 33.638 | 1.025 |
| fib-loop | C AOT | 5 | 33.514 / 33.417 / 33.468 | 33.419 / 33.268 / 33.308 | 0.996 |

All 120 invocations report a stable checksum. Noise-invalid invocations: 0.

### Files changed by the review fixes

| File | Change |
| --- | --- |
| `bindgen/src/completion.rs` | Reject recursive non-scalar result fields. |
| `bindgen/tests/completion.rs` | Add rejection controls and regenerate the completion mirror check. |
| `compiler/src/check/mirror_provenance.rs` | Derive recursive result eligibility from class fields and provenance; reject wire aliases. |
| `compiler/src/lir.rs` | Remove the duplicate creation position. |
| `compiler/tests/host_completion.rs` | Add recursive field and wire alias rejection controls. |
| `codegen/src/lower/func.rs` | Add the AAPCS64 stack-image plan. |
| `codegen/src/lower/func/abi.rs` | Apply composite register pressure to endpoints and ordinary structs. |
| `codegen/src/lower/func/boundary.rs` | Consume unused GPRs before the composite stack images. |
| `codegen/src/lower/func/instruction.rs` | Use the instruction position for source creation. |
| `codegen/src/lir/completion.rs` | Derive the void flag from the C directive and remove the duplicate position. |
| `codegen/src/cemit/graph.rs` | Use emitted Error sizeof/offsetof and the instruction position. |
| `codegen/tests/abi_pressure.rs` | Verify C callees for every earlier register count and actual completion. |
| `codegen/tests/native-fixture/build.rs` | Compile the pressure fixture with the platform C compiler. |
| `codegen/tests/support/native_fixture.rs` | Expose the new native pressure and completion symbols. |
| `codegen/tests/lir_completion.rs` | Remove the duplicate position assertion and update verifier diagnostics. |
| `codegen/tests/lir-goldens/corpus.txt` | Regenerate the LIR text after the position and foreign signature changes. |
| `codegen/tests/reload.rs` | Add a same-shape completion control without reload. |
| `corpus/interop/abi-pressure.c` | Add native endpoint, ordinary, and narrow composite callees. |
| `corpus/interop/abi-pressure.h` | Declare the native pressure callees. |
| `corpus/interop/host-completion.c` | Complete through seven int32 arguments. |
| `corpus/interop/host-completion.h` | Declare the seven-argument completion function. |
| `corpus/interop/host-completion.generated.d.ts` | Regenerate the mirror with the additional completion selection. |
| `corpus/accept/a347-host-operation-completes.ts` | Update only the contract pin hash. |
| `corpus/reject/r397-foreign-promise-without-completion.ts` | Update only the contract pin hash. |
| `corpus/trap/t108-host-operation-dropped-error.ts` | Update only the contract pin hash. |
| `runtime/src/exception/host_error.rs` | Release earlier allocations on later failure; test the rollback. |
| `runtime/src/context/host_operation.rs` | Correct the endpoint identity documentation before C word substitution. |
| `runtime/src/ffi.rs` | Document the UTF-8 message copy. |
| `runtime/include/subscript_runtime.h` | Regenerate the public C header. |
| `generated-docs/corpus-index.md` | Regenerate the corpus index with the final pin. |
| `generated-docs/language-reference.md` | Regenerate the language reference with the final corpus source. |
| `specs/tracking/s178-host-completion-measurement.md` | Remove stale process statements and record review evidence. |


## Implementation: class fixes

The AAPCS64 allocator counts NGRN and NSRN separately across scalars, aggregate images, HFAs, and indirect pointers.
A hidden struct-return pointer uses x8 and consumes neither cursor.
An HFA that exceeds the remaining SIMD registers exhausts NSRN and passes its complete image on the stack.
A small composite that exceeds the remaining general registers exhausts NGRN and passes its complete image on the stack.
Skipped register parameters preserve both cursors for every later argument.
The endpoint and every boundary struct use the same allocator.
Apple stack images use natural scalar sizes; base AAPCS64 images use eightbyte slots.

The native pressure sweep contains 846 C callees.
Four general-only kinds each use nine earlier general-register counts: integer, endpoint, small composite, and indirect large composite.
Ten SIMD kinds each use 81 general/SIMD count pairs: float, double, and one-through-four-leaf HFAs of each float type.
Every count ranges from zero through eight.
Each callee checks every earlier argument, every aggregate field, and the following float, double, and integer arguments.
One generated C library and one JIT module contain the sweep.
The same module runs through C AOT.
The NSRN-blind control fails 108 native callee checks on this arm64 host.
The allocator passes all 846 checks in both tiers.

The completed-source reader uses a C-layout temporary, then converts that value into the script local.
Boolean cache storage is one byte; the script local remains int32_t.
Direct, held, and function-value async producers use the same cache storage type.
The native result sweep covers eight integer types with both signs at each width, bool, _Float16, float, double, a C enum, a scalar alias, and a padded struct.
Each of the 15 types uses both a direct await and a held await: 30 native completions.
Five script boolean controls cover immediate, suspended, held, and function-value producers.
Both tiers produce 35 true results.

The checker accepts C enum completion results; the LIR verifier accepts the same boundary scalar.
The binder/checker gate checks 18 accepted result kinds and 13 rejected result kinds.
Each accepted case binds a header and checks the emitted mirror with an await.
Each rejected case compares binder rejection with checker rejection of the same result in a handwritten mirror.
The checker also rejects the runtime endpoint as a completion result, as the binder does.
The redundant field-provenance branch is removed.
Only callback fields receive that provenance; their resolved function type already rejects them.
String-view fields also fail the resolved-type check.

### Other target rules

*(docs)* [SysV AMD64 parameter rules](https://gitlab.com/x86-psABIs/x86-64-ABI/-/raw/master/x86-64-ABI/low-level-sys-info.tex): an aggregate uses eightbyte classes and reverts all register assignments if any class lacks capacity.
Five floats followed by an f32x4 aggregate use five plus two SSE registers; the following float uses xmm7.
Seven doubles followed by an f64x2 aggregate put the aggregate on the stack; the following double retains xmm7.
The existing SysV MEMORY fallback preserves this register capacity.
A unit gate checks both plans and the following double's capacity.

*(docs)* [Win64 parameter rules](https://learn.microsoft.com/en-us/cpp/build/x64-calling-convention): the first four argument positions select registers; all later positions use the stack.
Both 16-byte aggregates pass by reference, with no HFA register rule.
Five floats or seven doubles precede stack arguments in the two shapes.
The existing Win64 indirect plan applies to both aggregates.
The unit gate checks both plans.
Neither x86 target executes on this arm64 host.

### Open item

The binder rejects an external boundary struct from another mirror (§48) as a completion result or field.
This round records that binder limitation without a change.

### Contract pin

The a347, t108, and r397 entries change only their pin hash to d9a132f0.
A release CLI built from d9a132f0 rejects a347 and t108 with S100 for the unknown completion directive.
Both CLI check and C emission give that result.
The r397 mirror with an empty entry remains accepted.
Its dev JIT and C AOT runs each produce empty stdout.

### Gate cost

The costs below include isolated warm debug test-binary startup and exclude the Rust build.
Native costs include the required C AOT compilation.
The fixture archive builds once per Cargo build.
Separate generated C sources prevent the result gate from compiling the pressure sweep.

| Suite | Test | Work | Wall ms |
| --- | --- | --- | ---: |
| abi_pressure | `native_argument_kind_general_and_simd_pressure_sweep` | 846 native callees; one JIT module and one C AOT compilation. | 7492.092 |
| abi_pressure | `native_completion_result_type_sweep_in_both_tiers` | 30 native completions and five script controls; one module per tier. | 1627.404 |
| bind_completion | `binder_results_and_checker_results_agree` | 31 libclang parses and 31 in-memory checker calls. | 366.377 |
| subscript_codegen | `lower::func::abi::aggregate_abi_tests::float_pressure_shapes_follow_sysv_and_win64_rules` | Two x86 ABI shape plans and one following-register capacity check. | 703.049 |

### Verification

- cargo fmt --check: pass.
- cargo clippy --workspace --all-targets: pass with existing warnings.
- Binder, compiler, CLI, and fixture suites: 1,313 passed, zero failed, one ignored.
- Runtime suite: 445 passed, zero failed, one ignored.
- Codegen suite: 863 passed, zero failed, one ignored.
- Final binder/checker result gate: 18 accepted and 13 rejected cases pass.
- The native and corpus gates compare dev JIT and C AOT against their expected outputs.
- Both generated sweep scripts pass stock tsc.
- The isolated gate-cost invocations each pass.
- git diff --check: pass.

### Files changed in this round

| File | Change |
| --- | --- |
| `compiler/src/check/mirror_provenance.rs` | Accept C enums, reject endpoint results, and remove the redundant field test. |
| `codegen/src/lir/completion.rs` | Accept C enum result facts. |
| `codegen/src/cemit/emitter.rs` | Define cache storage types and record their sizes. |
| `codegen/src/cemit/suspend.rs` | Read C-layout buffers and use cache storage for direct producers. |
| `codegen/src/cemit/async_callable.rs` | Use cache storage for function-value producers. |
| `codegen/src/cemit/graph.rs` | Use cache storage for held producers. |
| `codegen/src/cemit/terminator.rs` | Write coroutine results with the cache storage type. |
| `codegen/src/lower/func.rs` | Represent skipped general or SIMD registers in stack plans. |
| `codegen/src/lower/func/abi.rs` | Allocate both AAPCS64 register banks and check x86 pressure plans. |
| `codegen/src/lower/func/boundary.rs` | Marshal endpoints and structs through the common plan. |
| `cli/tests/bind_completion.rs` | Bind and check every accepted and rejected result kind. |
| `codegen/tests/abi_pressure.rs` | Add native argument and result sweeps. |
| `codegen/tests/native-fixture/cases.rs` | Generate C callees, mirrors, scripts, and symbol addresses. |
| `codegen/tests/native-fixture/build.rs` | Compile the generated callees in the fixture archive. |
| `codegen/tests/native-fixture/lib.rs` | Expose generated fixture inputs and symbol addresses. |
| `corpus/accept/a347-host-operation-completes.ts` | Change only the contract pin hash. |
| `corpus/trap/t108-host-operation-dropped-error.ts` | Change only the contract pin hash. |
| `corpus/reject/r397-foreign-promise-without-completion.ts` | Change only the contract pin hash. |
| `generated-docs/corpus-index.md` | Regenerate the corpus pin references. |
| `generated-docs/language-reference.md` | Regenerate the corpus pin references. |
| `specs/tracking/s178-host-completion-measurement.md` | Record the class rules, sweeps, costs, target rules, and external-struct limitation. |

## Implementation: result-size class

### Completed result sizes

The aggregate carries the input result size and the output array element size separately.
Each reaction compares the input size with its recorded size and completed byte count.
Equal representations copy the bytes.
A one-byte boolean expands into a four-byte C AOT array element with a value conversion.
The dev JIT uses one byte for both boolean representations.
The C emitter and JIT runtime signature pass both sizes.
Direct and held readers keep their C-layout completion buffers.

The result sweep covers 15 types: eight integer types, bool, _Float16, float, double, a C enum, a scalar alias, and a padded struct.
Each type crosses four producers: a completion source, a script function, an async method, and an async closure.
Each producer crosses three readers: direct await, held await, and Promise.all with two elements.
The script producers suspend before completion.
The sweep gives 240 value checks and one true/false boolean array check per tier.
Both tiers print 241 true results.

The control that passes the array element size as the input size fails in C AOT.
It traps with `async resume without completion` at the first boolean aggregate.
The runtime gate checks one-byte arrays, four-byte boolean arrays, and the wrong input size with the same source shape.

### Independent C layout

The completion instruction derives its result size from the C directive.
C scalar spellings select C widths; scalar typedefs use their resolved boundary kinds.
Named C structs select their mirror declarations and use recursive C field layout.
The verifier compares that size against the script result layout.
C bool is one byte, and the script storage layout is also one byte.
The four-byte C SSA local does not determine the recorded completion size.

The scalar mismatch gate changes the C declaration from int32_t to int64_t and leaves the script result unchanged.
The struct mismatch gate changes the C declaration from a padded Pair to an eight-byte ShortPair.
Both forms fail the layout comparison without a change to the instruction's recorded size.
The verifier rejects StringAlias results, including aliases with wire values.

### Argument sweep and archive

The argument sweep contains 306 native callees instead of 846.
Four general-only kinds retain all nine general-register counts from zero through eight.
Ten SIMD kinds retain all nine SIMD-register counts and general-register counts zero, seven, and eight.
These general counts cover an empty bank, the last available register, and an exhausted bank.
Each callee checks all earlier arguments, all value fields, and the following float, double, and integer arguments.

The NGRN-blind control fails two callee checks.
The NSRN-blind control fails 36 callee checks.
The complete allocator passes all 306 checks in both tiers.
C AOT links the fixture archive that build.rs compiles once.
It does not compile class-sweep.c or class-results.c again for each test run.

The sweep contains no _Float16 HFA argument cases.
The _Float16 result cases remain byte-copy completion tests.
The boundary defects outside this surface remain in [the contract, §178.3](../blocks/compiler/s178-a-host-operation-completes-a-promise.md#1783-open).

### Contract pin

The a347, t108, and r397 entries change only their pin hash to e6266a2e.
A release CLI built from e6266a2e rejects a347 and t108 with S100 for the unknown completion directive.
CLI check and C emission give the same pin results.
The r397 mirror with an empty entry remains accepted.
The pin JIT and CLI C AOT runs each produce empty stdout.

### Files changed

| File | Change |
| --- | --- |
| `codegen/src/cemit/graph.rs` | Pass separate completion and array element sizes. |
| `codegen/src/lir/completion.rs` | Derive C result layout separately and reject wire alias results. |
| `codegen/src/lower/func/instruction.rs` | Pass both sizes in the dev JIT. |
| `codegen/src/lower/mod.rs` | Update the aggregate runtime call signature. |
| `codegen/tests/abi_pressure.rs` | Link the existing archive and extend the result readers. |
| `codegen/tests/lir_completion.rs` | Check independent C layouts and wire alias rejection. |
| `codegen/tests/native-fixture/cases.rs` | Reduce argument cases and generate all result producer/reader shapes. |
| `runtime/src/context/async_scheduler.rs` | Check each recorded result size and convert boolean elements. |
| `runtime/src/context/async_all_tests.rs` | Check equal sizes, boolean expansion, and a wrong input size. |
| `runtime/src/ffi/async_frames.rs` | Carry the input size in the aggregate FFI call. |
| `runtime/tests/async_budget.rs` | Supply both aggregate sizes. |
| `runtime/tests/async_inspection.rs` | Supply both aggregate sizes. |
| `corpus/accept/a347-host-operation-completes.ts` | Change only the contract pin hash. |
| `corpus/trap/t108-host-operation-dropped-error.ts` | Change only the contract pin hash. |
| `corpus/reject/r397-foreign-promise-without-completion.ts` | Change only the contract pin hash. |
| `generated-docs/corpus-index.md` | Regenerate the corpus pin references. |
| `generated-docs/language-reference.md` | Regenerate the corpus pin references. |
| `specs/tracking/s178-host-completion-measurement.md` | Record result sizes, controls, sweep costs, and verification. |


### Gate cost

These isolated warm debug costs include test-binary startup and C AOT compilation.
They exclude the Rust build and fixture archive compilation.
The result sweep grows from 35 checks to 241 checks to cover every producer and reader pair.

| Test | Work | Wall ms |
| --- | --- | ---: |
| `native_argument_kind_general_and_simd_pressure_sweep` | 306 callees; one JIT module and one C AOT compilation that links the archive. | 1938.327 |
| `native_completion_result_type_sweep_in_both_tiers` | 15 result types, four producers, three readers; 241 value checks per tier. | 5744.873 |

The previous 846-callee argument sweep cost 7492.092 ms on this host.
The reduced sweep costs 0.259 times that measurement and retains both register-bank failure controls.
The result sweep uses the cross product because each producer and reader has a separate completion path.
Both test comments state their measured costs and coverage reasons.

### Verification

- cargo fmt --check: pass.
- cargo clippy --workspace --all-targets: pass with existing warnings.
- Binder, compiler, runtime, and fixture suites: 1,693 passed, zero failed, two ignored.
- CLI tests outside the gate target: 52 passed, zero failed.
- Codegen suite: 865 passed, zero failed, one ignored.
- Final C-layout and native gates: 12 passed, zero failed.
- The corpus and native gates compare dev JIT and C AOT against their expected outputs.
- Both generated sweep scripts pass stock tsc.
- Both isolated gate-cost invocations pass.
- git diff --check: pass.
- Hygiene scan: pass.

### Implementation: bool fields

Completion result structs exclude bool fields at every depth, including scalar typedefs.
The binder, checker, and LIR verifier derive this restriction from their own fields.
A top-level bool result remains supported.

The agreement gate rejects {bool; bool}, {bool; int32_t; bool}, nested bool leaves, and bool typedef fields.
It accepts {uint8_t; int32_t; uint8_t}, its nested control, and scalar bool.
The LIR test constructs a bool field and rejects direct and nested completion results.
The native result sweep contains no bool-field structs.

Warm standalone wall times include test-binary startup and exclude Rust builds and native compilation.

| Test | Wall ms |
| --- | ---: |
| `binder_results_and_checker_results_agree` | 248.674 |
| `verifier_rejects_completion_struct_bool_fields_at_every_depth` | 7.391 |
