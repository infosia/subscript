<!-- §166 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 166. `Promise.all` over an array of handles

*(Added 2026-10-06.)* Origin: the owner's async usability proposals of
2026-10-06, item 2. The owner selected `Promise.all` on 2026-10-06.
`Promise.allSettled` is not in this section: its `tsc` result type is a
union with an `any` member (`PromiseSettledResult<T>`), which C7 and the
subset do not admit.

Problem: a program that waits for several handles has no form with one
failure point. Each handle needs its own `await`, and a failure that
leaves the function before a later `await` releases the later handle
unobserved. Measured at `05ff5f01` (`fails` throws after one `await`):

```ts
async function load(): Promise<i32> {
  const a: Promise<i32> = fails("a");
  const b: Promise<i32> = fails("b");
  const x: i32 = await a;
  const y: i32 = await b;
  return x + y;
}
// main: try { await load(); } catch (e) { print(`caught ${e.message}`); } print("end");
```

- This language: trap 29, `Error: b`, before `caught a` (§116.1 rule
  4b), exit 1.
- `node` v24: `caught a`, `end`, then an unhandled rejection of `b`.
- With `await Promise.all([fails("a"), fails("b")])` in `load`, `node`
  prints `caught a` and `end`, and exits 0. This language rejects the
  call (S013, `r98`).

Measured `node` order (`tsc`-emitted CommonJS, `print` bound to
`console.log`; another root prints one line for each queue turn):

- A success resumes the caller at the same turn as two direct `await`s
  of the inputs in input order.
- A failure resumes the caller one turn later than a direct `await` of
  the failed input: the input completion runs the aggregate's reaction,
  and that reaction completes the aggregate.
- The aggregate fails with the first failure in completion order, not
  in input order.

### 166.1 Rules

1. `Promise.all(jobs)` is accepted where `jobs` has the type
   `Promise<T>[]`. Its type is `Promise<T[]>`. The call creates an
   aggregate handle. The aggregate is an async handle (§70): the §70 and
   §116 rules apply to it, including "every created handle has an
   awaited completion" (`r100`) and the unobserved-exception trap
   (§116.1 rule 4).
2. For `Promise<void>[]`, the aggregate has the type `Promise<void[]>`
   in `tsc`. This language has no `void[]` value: the aggregate is
   accepted as the operand of an `await` statement or of a stored
   handle that only `await` statements read. A use of the `void[]`
   value is rejected with S013 (`collisions.md` C24 row 34).
3. The call takes a snapshot of the array: it holds one count on each
   element (§70.3), duplicates included. A later change of the array
   does not change the inputs.
4. The aggregate registers on each input in input order, at the call.
   An input that is complete at the call queues the aggregate's job for
   that input at the call (as §94.1 rule 6 does for an `await`).
5. An input completion queues the aggregate's job for that input (§94.1
   rule 5). The job reads the input's completion and releases the
   input's count. That read observes an exception (§116.1 rule 4).
6. When the job of the last input runs and every input succeeded, the
   aggregate completes with a new `T[]` that holds the results in input
   order. `T` holds no count (rule 12), so a byte copy stores each
   result.
7. When a job reads the first exception, the aggregate completes with
   that exception (§116.1 rule 1). The remaining jobs stay registered;
   each reads its input's completion when it runs, so a later exception
   is observed and does not trap. This holds after the caller catches
   the aggregate's exception, and after the last holder releases the
   aggregate.
8. An empty array completes the aggregate at the call with an empty
   `T[]`. An `await` of it suspends its caller (§94.1 rule 6).
9. Only a host checkpoint runs the aggregate's jobs (§94). An aggregate
   is not an invocation: `subscript_rt_ctx_async_unfinished` does not
   count it. `subscript_rt_ctx_async_pending` counts its queued jobs.
10. A live aggregate keeps its unread inputs and its partial state
    alive for explicit collection (invariant 2). An input that never
    completes keeps the aggregate alive.
11. The three tiers (dev JIT, C AOT, interpreter) give the same order
    and output. The order follows the measured `node` order above.
12. Every other `Promise` static and member stays rejected (S013,
    `r96`–`r98`). These forms of `Promise.all` are rejected with S013:
    an argument whose type is not `Promise<T>[]`, an explicit type
    argument, and a `T` that holds a count (an async handle, or an array
    of async handles; `collisions.md` C24 row 35). The runtime
    completion carries bytes and a size only, so the aggregate cannot
    store a counted result through the §70.3 rule 2a path (measured in
    round 2).
13. `collisions.md` C8 states that `Promise.all` over `Promise<T>[]` is
    the one admitted combinator.
14. A waiter and a ready job have a tagged form: an invocation
    continuation (a frame), or an aggregate reaction (the aggregate and
    the input index). The async registry records the kind of each
    entry, so `subscript_rt_ctx_async_unfinished` counts invocations
    only. A checkpoint dispatches each job by its kind. The runtime form
    serves the dev JIT and C AOT. The interpreter keeps its own queue
    with the same tagged form and the same order. The differential gate
    compares the three tiers. Round 1 measured that the frame-only
    waiter (`Vec<*mut u8>`, `VecDeque<*mut u8>`) and the interpreter's
    `Rc<RefCell<Coroutine>>` lists carry no aggregate reaction.

### 166.2 Acceptance

1. Red first. At the contract pin, record each result:
   - Accept entry `a335`, `js-comparable: yes`, golden from `node`. It
     covers: success with another root that prints each turn, the
     first failure in completion order, a later failure that is
     observed, a caught failure followed by a later input failure, an
     empty array, a duplicate handle, a change of the array after the
     call, an input complete at the call, an input that another
     `await` also reads, and a `void` aggregate. At the pin, the checker
     rejects it.
   - Reject entries `r377` (a use of the `void[]` value) and `r378` (an
     argument that is not `Promise<T>[]`), each with its measured `tsc`
     header.
   - Trap entry `t83`: a failed aggregate that no `await` observes
     traps 29 when its last holder releases it.
2. Unit tests for rules 3 to 10, each with a same-shape control. The
   counts of each input and of the aggregate are zero after success and
   after failure (§70 count tests). An explicit collection between
   checkpoints keeps every unread input and the aggregate result.
3. The §154 total test passes for each new rejection site.
4. The async-cost benchmark (`benchmarks/src/bin/async-cost.rs`) on
   the existing workloads: the median is at most 1.05 times the pin
   median (release, best of three).
5. No existing `.expected` golden moves.

### 166.3 Open

The Phase Review found these. None is CRITICAL or MAJOR.

1. A `Promise` static in an `await` operand (`await Promise.race(ps)`)
   gives S016 "unknown name `Promise`" and a follow-on S013 for the
   dropped handle, not the S013 of rule 12. Present before §166.
2. A container of the `void[]` value is accepted:
   `await Promise.all(vs)` with `vs: Promise<void[]>[]` gives a
   `void[][]` whose length a program reads. Each element read is
   rejected, and the three tiers print what `node` prints. Rule 2
   names the `void[]` value only.
3. The doc comment of `subscript_rt_ctx_async_pending`
   (`runtime/src/ffi.rs`) does not name the aggregate jobs that rule 9
   counts.
4. A match arm in `codegen/tests/cemit.rs` (the `t82` arm) lost its
   indent; rustfmt skips that match.

