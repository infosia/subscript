<!-- §234 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 234. `await` reads the handle of a synchronous call

*(Added 2026-11-07.)* Origin: §167.3 item 2. Under the
owner's standing approval of 2026-11-06 for `tsc` divergences with a
known fix.

Problem, measured at `56798d52` with `tsc` 5.9.2, the libraries of the
project, and the debug CLI. `v(): Promise<i32>` is async.
`function s(): Promise<i32> { return v(); }`, and `class K` has the
method `s(): Promise<i32>` and the static method `t(): Promise<i32>`,
each synchronous.

| Form | subscript | `tsc` |
|---|---|---|
| `await s()` | S100 "`s` is synchronous and cannot be awaited" | accepts |
| `await new K().s()` | S100 "method `s` is synchronous and cannot be awaited" | accepts |
| `await K.t()` | S100 "class `K` used as a value" | accepts |
| `const f = s; await f()` | accepts | accepts |
| `const h = K.t(); await h` | accepts | accepts |
| `const h = k.s(); await h` | accepts | accepts |

The divergence `AwaitSynchronousFunction` states its reason: "a
synchronous function call supplies only its immediate result". That
reason holds for a result that is not a handle. It does not hold for a
synchronous call whose result is a handle: the result is the handle, and
`await` of a handle in a local is accepted. The same program is accepted
or rejected by where the handle is held.

### 234.1 Rules

1. `await` of a direct call of a synchronous function, method, or static
   method whose result type is `Promise<T>` is accepted. It has the
   meaning of `const h = call; await h`: the call runs, and `await` reads
   its handle.
2. `await` of a synchronous call whose result is not a handle keeps its
   diagnostic and its divergence (`AwaitSynchronousFunction`,
   `AwaitSynchronousMethod`).
3. The rules of a handle in a local apply to the handle of rule 1: S013
   and §178 rule 1 treat the call as an async origin, as for the local
   form.
4. §215.1 rule 3a stays: a synchronous callee under `await` takes no
   contextual candidate from the context of the `await`. §215.4 item 3
   stays open: `await saf()` with no type argument keeps its
   no-candidate text, and `await saf<P>()` is accepted by rule 1.
5. No run-time cost beyond the local form. The three tiers give the same
   output as the local form.

### 234.2 Acceptance

1. Red first, at the pin: the accept entry
   `a421-await-a-synchronous-handle-call` is rejected. The tracking note
   records the output.
2. `a421` passes `tsc`. Its output under `node` equals its golden. It
   has the first three rows of the table, an argument, a call in a
   template and in an expression statement, and a generic synchronous
   function with an explicit type argument.
3. A unit test pins rules 1, 2, and 4, with controls: `await` of a
   synchronous call that returns `i32` keeps its text, and the local
   form gives the same type.
4. If a corpus entry or golden changes its type or output, report it and
   stop.
5. The rejection-site total check passes. Each site keeps a witness of a
   form that it still rejects. §167.3 item 2 cites this section. `a421` has an async entry, so the LIR text snapshot grows by
   its functions. No other golden moves.
