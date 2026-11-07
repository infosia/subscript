<!-- §236 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 236. An async return adopts a handle

*(Added 2026-11-07.)* Origin: §233.4 item 1 and the divergence
`AsyncReturnHandle` (§167 rule 3). Under the owner's standing approval
of 2026-11-06 for `tsc` divergences with a known fix.

Problem, measured at `3aa70ee7` with `tsc` 5.9.2, the libraries of the
project, `node` v24, and the debug CLI. `v(): Promise<i32>` is async,
and `type Mode = "a" | "b"`.

| Form | subscript | `tsc` |
|---|---|---|
| `async function h(): Promise<i32> { return v(); }` | S100 "the return value expects `i32`, got `Promise<i32>`" | accepts |
| `async function f(): Promise<Mode> { return v().then((x: i32): Mode => "a"); }` | S100 | accepts |
| `async (p: Promise<i32>): Promise<i32> => p` (`r382`) | S100 | accepts |

The divergence states its reason: "the language has no implicit handle
adoption". §167 measured that a rewrite to `return await h` completes
one turn earlier than `node` adoption. The order of turns is
observable, so that rewrite is not the meaning of `return h`.

Measured with `node`, a ticker task that prints one line per turn, and
`v` as the handle:

| Form | `v` settled at the return | `v` pending for two turns |
|---|---|---|
| `return await v()` | result after `turn3` | result after `turn5` |
| `return v()` (adoption) | result after `turn4` | result after `turn5` |
| `const h = v(); await h; return await h;` | result after `turn4` | result after `turn6` |

The development tier gives the same order as `node` for the first and
the third rows. Adoption is one turn of the queue, then an `await` of
the handle: the job that reads a returned handle runs one turn after
the return, and the handle completes the function after that job and
after its own completion. This is a form with a known lowering.

`node` with a `try` statement:

| Form | Output |
|---|---|
| `try { return v(false); } catch { … } finally { log("finally") }`, `v` rejects | `finally`, then the caller sees the rejection; `catch` does not run |
| the same with `return await v(false)` | `catch`, then `finally`, then the caller sees the value of `catch` |

In a `try` statement, `return h` and an `await` of `h` differ in which
handler runs and in the order of `finally`. In `node`, the function
leaves each scope first: `finally`, a `using` dispose, and the close of
a `for…of` iterator run at the `return`. The adoption starts after
that, outside each handler, so a `catch` of the function does not see
a rejection of the handle. This is also a form with a known lowering.

### 236.1 Rules

1. In an async function or an async arrow with the result
   `Promise<T>`, `return e` is accepted when `e` has the type
   `Promise<S>` and `S` is assignable to `T`. An expression body of an
   async arrow is such a return. `T` is not `void`: a `void` result
   keeps its present diagnostic (§236.4).
2. The meaning of rule 1 is: evaluate `e` and hold its handle; leave
   each scope of the function as a `return` does, with each exit action
   (`finally`, a `using` dispose, the close of a `for…of` iterator);
   then, outside each handler of the function, wait one turn of the
   queue, `await` the handle, and complete the function with its value
   or its exception. The order of turns and of exit actions equals
   `node` adoption.
2a. The last completion of the function wins, as in `node`. After the
   return of rule 1, an exit action can complete the function again: a
   `finally` that returns or throws, or a dispose that throws. (A throw
   that leaves an iterator close traps, `collisions.md` C6, for each
   kind of return.) That later completion replaces the
   pending adoption, and the held handle is released with no `await`.
   The function holds one completion state and one epilogue.
3. A leaf of the operand is the operand, or a branch of a conditional
   or of parentheses in it. A leaf whose type, read without a context,
   is `Promise<…>` takes the context `Promise<T>`: the declared result
   of its callee, or the declared type of a local, a parameter, or a
   field. A `then`, `catch`, or `finally` reaction is such a leaf, so
   its callback takes `T` as its result context (§233). Each other leaf
   takes the context `T`. If the leaves mix a handle and a value, the
   return has the meaning of one return per leaf: `return b ? x : h` is
   `if (b) return x; else return h;`, and the condition runs once.
4. Rule 1 applies in each part of a `try` statement. Rule 2 gives the
   meaning of `node` there. If no form remains that the site
   `AsyncReturnHandle` rejects, the site and its divergence are
   removed.
5. A return of a value of type `T` is unchanged, and its lowering is
   unchanged. S013 does not fire for
   the handle of rule 1: the return reads it.
6. The development tier, the ship tier, and the interpreter give the
   same output. An accept entry that compares the order of turns
   carries `js-comparable` and equals `node`.

### 236.2 Acceptance

1. Red first, at the pin: the accept entry
   `a423-async-return-adopts-a-handle` is rejected. The tracking note
   records the output.
2. `a423` passes `tsc`. Its output under `node` equals its golden. It
   has the three rows of the first table, a ticker task that shows the
   order of turns for a settled and a pending handle, a return of an
   exception that the caller catches, and a value return as a control.
   It also has, each with the order of its output lines equal to
   `node`: a return of a handle in `try`/`catch` (the handle rejects,
   and the caller catches), in `try`/`finally`, after a `using`
   declaration, and in a `for…of` over a generator with a `finally`; and
   a conditional `return b ? v() : w()`. It also has the forms of rule
   2a: a `finally` that throws, a `finally` that returns a value, a
   `finally` that returns another handle, and a dispose that throws,
   each after an adopting return, with the order of output equal to
   `node`.
3. If the site `AsyncReturnHandle` keeps a form, the reject entry
   `r382` has that form. If the site goes, `r382` is retired and its
   form moves to `a423`.
4. A unit test pins rules 1, 3, 4, and 5, with a mixed leaf. `a423` has
   a mixed leaf with the order of its output equal to `node`.
5. The rejection-site total check passes. Each site keeps a witness of
   a form that it still rejects.
6. If a corpus entry or golden changes its type or output, report it
   and stop. `a423` has an async entry, so the LIR text snapshot grows
   by its functions. No other golden moves.

### 236.3 Phase Review

*(Added 2026-11-07.)* Three fresh reviews of the implementation. Each
fix changed the form; the contract gained rules 2a and the present
rules 2 to 4.

1. MAJOR, realistic, fixed: a `using` dispose and the close of a
   `for…of` iterator ran after the `await` of the adopted handle, and
   `node` runs them at the `return`. The first form counted `try`
   statements only. Rule 2: the return leaves each scope first, and
   one epilogue outside each handler does the turn and the `await`.
   Adoption in a `try` statement then has the meaning of `node`, so the
   divergence `AsyncReturnHandle`, its site, and `r382` are removed.
2. MAJOR, realistic, fixed: a conditional `return b ? v() : w()` was
   rejected, because the context came from a list of callee kinds.
   Rule 3 reads each leaf; a mixed leaf splits into one return per
   leaf.
3. MAJOR, realistic, fixed: a later completion (a `finally` that
   returns or throws, a dispose that throws) did not replace a pending
   adoption. Rule 2a: one completion state per function; the last
   completion wins.
4. MINOR, fixed: a leaf through a nullable receiver, an unannotated
   async arrow with a mixed body, stale comments and docs, the
   `collision` text of the removed divergence, the amend notes of §167
   and §215, one word for the completion state, and a firing control
   for the `finally` placement of the release model.

### 236.4 Open

1. `async function f(): Promise<void> { return work(); }` with
   `work(): Promise<void>` stays rejected: a `void` expression is only
   a statement. `tsc` accepts it.
2. An adopted handle that a later completion abandons, and that then
   rejects, traps (`collisions.md` C8, §116.1 rule 4b). `node` reports
   an unhandled rejection and continues.
3. The epilogue of an adopting function is lowered at each exit, as a
   `finally` is. A function with four value returns and one adopting
   return has 739 lines of C `resume` against 168 for the form with
   `return await`. One shared epilogue needs a new HIR form.
4. A call with two arguments that each make a handle, such as
   `two(v(), v())`, fails the release fact of the LIR test model at
   `3aa70ee7`. The output of the program is correct. `a423` holds such
   handles in locals.
