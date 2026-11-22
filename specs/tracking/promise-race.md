# `Promise.race`

Status: **measured 2026-11-22 at `7c1130fa`; the contract is
`compiler.md` §350.** The measurement round changed no file of the
tree.

## Owner decisions, 2026-11-22

- `Promise.race` joins the language, with the accepted forms of
  `Promise.all`: an array literal of handles and a `Promise<T>[]`
  value.
- A. The race completes with the completion of the first input that
  completes: its value or its exception. A later failure of an input
  that lost is ignored.
- B. An empty array traps.
- C. If some inputs have a completion at the call, the first of them
  in array sequence wins.
- No cancel: an input that lost runs on. The race gives the value
  only, not the index.

## Measured at `7c1130fa` (dev JIT, `tsc` 5.9.2, node v24.22.1)

1. `tsc` takes `Promise.race` from its lib; the prelude needs no
   change. `Promise.race([])` is `Promise<never>` for `tsc`. A literal
   of `Promise<i32>` and `Promise<string>` is a union for `tsc`; the
   array rule of this language rejects it (S100), as for
   `Promise.all`.
2. Today `await Promise.race([f(), g()])` is S016 "unknown name
   `Promise`" (§166.3 item 1), and the held form is S013 "Promise
   static `Promise.race(...)` is not in the language".
3. `Promise.all` with one input that throws: the await raises the
   first failure in completion sequence, the other input runs to its
   end, and a second failure is observed with no trap.
4. A handle can be awaited again after an aggregate read it, and each
   await gives the same value or raises the same exception.
5. `Promise.all` over an empty `Promise<i32>[]` value completes with
   an empty array. An empty literal is S100 (no element type).
6. node: `race` of two settled inputs gives the first in array
   sequence; a rejection first in the array wins over a value; a
   pending input that settles in the same turn does not pass a settled
   one; an empty array does not settle; a failure of an input that
   lost gives no unhandled rejection.
7. Turn sequence: a direct `await` of a task that completes at turn N
   resumes at turn N. `await Promise.all([task])` and
   `await Promise.race([task])` resume at turn N + 1 in node.
   `Promise.all` in the dev JIT resumes at N + 1 also.
8. The aggregate of `Promise.all` holds one count of each input and
   stays registered until each input completes, also after its last
   holder ends. Each input that it reads is marked as observed.
9. An aggregate is a frame of a runtime kind: a root of a collection
   while it is registered (§348 rule 5), with its completion bytes.
10. A `Worker.receive` that lost a race stays in the sequence of its
    Worker and takes the next message of that Worker.
