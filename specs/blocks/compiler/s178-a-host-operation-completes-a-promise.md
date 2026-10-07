<!-- §178 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 178. A host operation completes a promise

*(Added 2026-10-08.)* Origin: the owner's async usability proposals of
2026-10-06, item 3 (host completion sources). On 2026-10-08 the owner
selected it, the trailing endpoint parameter form (rule 4), and the
result set of rule 6. The measurement round at `77593125` is
`specs/tracking/s178-host-completion-measurement.md`.

Problem: a script cannot `await` an operation that the host completes
later. The accepted forms poll or use a callback, and the measurement
found three defects in them:

| Form | Defect | Evidence |
|---|---|---|
| Poll with `Context.suspend()` | `k − 1` of `k + 1` dispatches per operation read no result; at k = 10 the poll takes 1.5–1.8 times the time of a completion source | tracking note, A1 and C2 |
| Callback (§14, §111) | A host failure cannot reach the `await`: an exception that escapes the callback becomes a Context trap | `runtime/src/ffi/callbacks.rs` `fire_callback` |
| A mirror `declare function f(): Promise<i32>` | Accepted: the host return value is used as a handle; `f()` with no `await` gets no §70 diagnostic; `await f()` gets S100 | tracking note, C3 |

A batch poll in one task is faster for a cheap host (A1), but it gives
no handle per operation, so `Promise.all` (§166), a task group (§170),
and §116 exception delivery do not apply to it.

### 178.1 Rules

1. **The script type is `Promise<T>`.** A completion function returns
   an ordinary handle. §70 counts, §116 exception delivery, §166,
   §170, and §171 apply to it with no change. A call is an async
   origin: `await f()` is accepted, and a call with no `await` or
   holder gets the §70 diagnostic.
2. **A source.** The call creates a runtime task with no frame (runtime
   kind 4, *host operation*). It records its result size, its creation
   position (the call), its completion, and its waiters. It starts with
   two counts: the script holder and the producer. A completion
   releases the producer count.
3. **The endpoint.** `subscript_rt_completion` is a C struct of two
   `uint64_t`: the Context id and the operation id. Each Context gets a
   Context id that the process never reuses. Operation ids increase in
   one Context and never wrap; exhaustion is a trap at the call.
4. **The C declaration.** A host function that completes later takes a
   `subscript_rt_completion` as its last parameter and returns `void`.
   The generated adapter creates the source, calls the host function
   with the endpoint, and returns the handle. The host function returns
   at once; it stores the endpoint.
5. **The binder.** `subscript bind --completion <function>=<result>`
   names each such function and its result type. The binder rejects a
   function with an endpoint parameter that no option names, an option
   whose function has no trailing endpoint parameter or does not
   return `void`, an endpoint parameter that is not last, and a result
   outside rule 6. The mirror declares the function with the endpoint
   parameter removed and the result `Promise<T>`, and carries one
   directive line `// @subscript-c-completion function=… result=…`.
   The checker reads the directive (mirror provenance, as the other
   `@subscript-c-*` directives).
6. **Results.** A result is a boundary scalar, a boundary struct whose
   script layout equals its C layout, or `void`. A struct has that
   layout when each field is a boundary scalar or such a struct: a
   string-view field, an absorbed `(pointer, count)` descriptor, a
   pointer, a callback, a `bool` field, and a `@subscript-cenum` alias
   are not (178.3 item 4). A
   boundary struct that another mirror declares (§48) is not in this
   section. The
   completion copies the C bytes, so a struct of another layout cannot
   complete. A C enum is a boundary scalar. The binder and the checker
   each reject another result, and the checker accepts each result
   that the binder emits. Every result can complete
   with an `Error`. Opaque handles, strings, and reference types are
   not in this section.
7. **Completion.** The runtime C API has three functions:
   - `subscript_rt_complete_value(ctx, endpoint, const void* value, size_t size)`
   - `subscript_rt_complete_void(ctx, endpoint)`
   - `subscript_rt_complete_error(ctx, endpoint, const char* message, size_t length)`

   Each returns a `subscript_rt_completion_status`:

   | Status | Condition | Effect |
   |---|---|---|
   | `OK` (0) | live source, not completed | copies the value, or allocates an `Error` with the message; queues the waiters; releases the producer count |
   | `STALE` (1) | another Context id, or an ended source | none |
   | `DUPLICATE` (2) | a live source that is completed | none; the first completion stays |
   | `MISMATCH` (3) | `size` differs from the result size, a value for a `void` source, or `void` for a value source | none |
   | `TRAPPED` (4) | the Context holds a trap | none |

   A completion runs no script code. The host calls it on the Context
   owner thread. It can call it during a host call from the script
   (for example inside the start call, or inside a host pump function);
   the generated trap check after that host call stops the script if
   the completion trapped (rule 8). The runtime does not check the
   thread (invariant 6).
8. **A dropped handle.** The producer count keeps a pending source
   live after the last script holder ends. A value completion then
   frees it. An `Error` completion that no `await` observed traps at
   that release with §116.1 rule 4 (trap 29). The trap position is the
   creation position of the source.
9. **Error position.** The `Error` that a completion creates carries
   the creation position of the source as its throw position.
10. **Collection, reload, and destruction.** A pending source roots
    its waiters; `Context.collect()` does not change it. A source keeps
    its identity across a reload; a waiting frame of an old generation
    follows the stale frame rule when it resumes. Context destruction
    discards each pending source and runs no script code. The host
    must stop delivery before it destroys the Context: the endpoint
    does not make a freed Context pointer safe.
11. **Inspection.** §169 reports a source as kind 4, `WAITING` before
    completion and `COMPLETE` after, with zero function and await
    positions and its creation position. `subscript_rt_ctx_async_unfinished`
    counts a pending source. `subscript_rt_ctx_async_pending` does not.
12. **A `Promise` result needs a source.** A foreign function whose
    mirror result is `Promise<T>` and that has no completion directive
    is rejected. A completion directive whose declaration does not
    match rules 4–6 is rejected.
13. **Tiers.** The dev JIT and C AOT give the same output, statuses,
    and traps. The dev JIT passes the endpoint, and each other struct
    argument of 16 bytes or less with no `_Float16` leaf, at the place
    that the platform C ABI gives it for every count of earlier general
    and SIMD register arguments. The size that a tier reads from a
    source is the size of the C result type, as the source records it.
    Each consumer of a completed result reads that size: an `await`, a
    held handle, and a `Promise.all` input, where the input result size
    and the array element size are separate facts. The interpreter has
    no foreign call (§68.7); a corpus entry that calls a host carries
    `interpreter: no`, as the other interop entries do.
14. **Cancellation and deadlines** are host decisions: the host
    completes the source with an `Error`. This section adds no script
    surface for them.

### 178.2 Acceptance

1. Red first, at the contract pin:
   - accept entry `a347`: a synthetic host library in `corpus/interop/`
     with completion functions for an `i32`, a `@ValueType` struct, and
     `void`, and a host pump that the script calls. It covers
     completion after `await`, completion inside the start call,
     shared and repeated `await`, `Promise.all` over completion
     handles, an `Error` caught at the `await`, and `Context.collect()`
     with a waiter;
   - trap entry `t108`: the script drops the handle, and the host
     completes it with an `Error`; trap 29 at the call position;
   - reject entry `r397`: a mirror foreign function with a `Promise`
     result and no directive.
   Record the pin result of each in each tier.
2. Unit tests:
   - the runtime C API: each status of rule 7, with a same-shape `OK`
     control for each; two Contexts give different Context ids; a
     completion before the `await`; Context destruction with a pending
     source; the inspection record and the unfinished count of rule 11;
   - the dev JIT: a waiting frame across a reload;
   - the binder: each rejection of rule 5, and the mirror text of one
     accepted function;
   - the checker: rule 1 (`await f()` accepted, a dropped call
     diagnosed) and rule 12.
3. Cost: the async-cost workloads and the benchmark workloads within
   1.05 of the pin. The tracking note records the rule 7 path cost per
   operation with the workload of the measurement round. State the
   cost of each new gate test (core principle 15).
4. Goldens: the generated API reference and corpus index move. No
   other `.expected` golden moves.

### 178.3 Open

The class-fix review found boundary defects that are older than this
section and are not in its surface (no completion result or endpoint
reaches them). Each is realistic. They need their own section, with a
sweep over every boundary scalar kind against the platform C compiler:

1. The dev JIT does not sign- or zero-extend an `int8_t`, `int16_t`,
   `uint8_t`, `uint16_t`, or `bool` register argument; Apple arm64
   requires the caller to extend it. A callee reads `-5` as `251`.
2. A `_Float16` scalar argument, field, or return crosses the boundary
   as a number, not as its bits, in C AOT, and in a general register in
   the dev JIT.
3. The dev JIT passes a struct with `_Float16` leaves (an HFA) in
   general registers, and reads such a struct return from `x0`.
4. C AOT emits a `bool` field of a boundary struct as `int32_t`, so the
   emitted struct does not have the C layout (invariant 1). A struct
   return `{bool a; int32_t b; bool c;}` with nonzero padding bytes
   reads `a` and `c` as `true` in C AOT and `false` in the dev JIT; a
   `{bool a; bool b;}` result has another size.

### 178.4 Sections this one amends

- §169: runtime kind 4 (rule 11).
- §70: a completion function call is an async origin (rule 1).
- §13 and §14: a host function can complete later through an endpoint
  (rules 4–7).
