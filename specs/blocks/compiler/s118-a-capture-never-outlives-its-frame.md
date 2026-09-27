<!-- §118 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 118. A capture never outlives its frame

*(Added 2026-09-27.)* Origin: the §116 Phase Review found a capturing
lambda that escapes through a callee.

Problem: collision C5 lets a capturing lambda be passed downward and
forbids every other escape. Its environment lives on the stack, or in
the caller's coroutine frame (§67 rule 1k). At `f682926` the checker
enforced C5 with a taint inside one function
(`compiler/src/check/mod.rs` `is_capturing_value`). A parameter carried
no taint, so a callee could store or return a lambda that its caller
passed down, and the lambda then outlived the environment it captured.

Measured at `f682926`:

| Probe | dev tier | ship tier | `node` |
|---|---|---|---|
| a lambda that returns a captured async handle, stored by a callee, called after the defining function returned | `trap [internal]: async resume without completion` | `trap 11` | `late 7` |
| the same escape with an `i32` capture | `late 550780944` | `late 1` | `late 5` |

A path-following fix (taint every function-typed parameter, then close
each further path) went through three reviews, and each review found a
path the previous one did not close:

1. an argument of a generator or async call, a `yield`, and the
   synthetic local of `??` (a generator: `5` then `57394604` on the dev
   JIT);
2. a held async handle whose caller throws before its `await`: the host
   queue resumed the callee, which read `0` on the dev JIT and
   `123456789` on the ship C from a dead environment;
3. a `Generator`-typed parameter, a for-of element, `.next().value`,
   and a loop back-edge (the taint was applied in source order).

Under CLAUDE.md's two-review rule this is a defect of the form. Two
measurement rounds (codex; each prototype reverted, the tree checksum
equal before and after) measured a type-directed form over the whole
repository: the corpus, the examples and their mirrors, the benchmark
workloads, the README and tutorial programs, and one full
`cargo test --workspace`. New rejections of existing programs:

| Form | New rejections | Actually capturing |
|---|---|---|
| F1: every value of a carrier type is may-capture unless it is syntactically clean; local facts are flow-insensitive | 54 | 0 |
| F1 + N1: `new C(args)` with clean arguments is clean | 7 | 0 |
| F1 + N1 + N2: a parameter that escapes is inferred, and its callers pass clean values | 0 | 0 |

Every open escape probe of the three reviews is rejected under
F1 + N1 + N2, and an indirect call is rejected when it would pass a
capture. The N2 round also found one unsound path in hot reload: a
lambda created by the old code survives a reload and calls a callee
whose parameter now escapes. §118.1 rule 10 closes it.

### 118.1 The rule

1. **Carrier types.** A function type, a `Generator`, and a container of
   a carrier type at any depth: an array, a `FixedArray`, a `Map`, a
   `Set`, an iterator result, a class with a field of a carrier type,
   and the nullable form of each.
2. **Clean values.** A value of a carrier type is clean when it is: a
   reference to a named function; a lambda literal with no captures; a
   read of a field or a global, because storage only receives clean
   values; `this`, whose fields only receive clean values; a call
   result, and the result of an `await` of a held handle, because a
   `return` only delivers clean values; a generator call whose arguments are all
   clean; `new C(args)` whose arguments are all clean; an aggregate
   literal whose parts are all clean; and an escaping parameter inside
   its own body (rule 7). Every other value of a carrier type **may
   capture**: a lambda with captures, a non-escaping parameter, and
   anything that flows from either.
3. **Local facts are flow-insensitive.** A local may capture when any
   assignment to it anywhere in its function may capture. The facts are
   joined to a fixed point before any use is checked, so loop order and
   source order do not matter.
4. **Reads follow their container.** A value of a carrier type that any
   operation reads out of a may-capture container, iterable, or
   generator may capture: an index, a for-of binding, `.next().value`,
   and the result of every built-in method whose receiver may capture.
   The rule is by type, not by a list of operations.
4a. **A parameter default is an assignment.** `b = a` in a parameter
   list assigns `a` to `b` for rules 3 and 7. It is not a storage
   boundary. A global initializer and a field initializer are.
5. **Escape boundaries.** A store to a field, a global, an element, an
   aggregate literal, a `Map` or a `Set`; a `return`, including the
   body of an expression-body lambda; a `yield`; a C callback slot; a
   capture by a lambda that escapes; an argument bound to an escaping
   parameter; and an argument of an `async` call that is not the
   operand of a direct `await`. A may-capture value at a boundary fails
   with S009, with a message that names the value and the boundary
   (§103).
6. **A direct `await` is not an escape.** An `async` call whose
   argument may capture is accepted only as the operand of a direct
   `await` at the call. The caller is suspended until the callee
   completes, and its frame stays alive through §116.1 rule 4a and
   §94.2. A held handle of such a call is rejected: the host queue
   resumes the callee without its holder.
7. **Escape inference.** A parameter of a carrier type of a function, a
   method, a constructor, or a lambda **escapes** when its value, by
   rules 3 and 4, reaches a boundary of rule 5 in its body, or is
   passed to an escaping parameter. The fact is a fixed point over the
   call graph of the whole program. Inside its body an escaping
   parameter is clean; a non-escaping parameter may capture.
8. **Callers.** An argument bound to an escaping parameter must be
   clean. A may-capture argument there fails with S009 at the argument,
   with a message that names the callee (class-qualified for a
   constructor and a method) and the parameter.
9. **Unknown callees.** An indirect call through a function value has
   no known body, so each of its carrier parameters escapes. A built-in
   operation keeps its declared behaviour: the callback of `forEach`,
   `map`, `filter`, `reduce`, `sort`, and the other array and
   collection operations does not escape.
10. **Reload.** The escape fact of every parameter is part of the
    declaration hash (§8.2's accepted and rejected swaps). A reload
    that changes whether a parameter escapes is a rejected swap, as a
    signature change is. A body edit that keeps every escape fact stays
    an accepted swap.

### 118.2 Sections this one amends

- §115.6 rule 3: a built-in call is a raise site when it calls a script
  callback, not when an operand has a function type. Measured: `a.push(named)`
  on an array of functions passed `check` and failed on the dev JIT with
  "a raise site with no raise edge".

- Collision C5: the rule of §118.1 is how C5 is enforced.
- §116.1 rule 4c: the lambda borrows a captured handle; §118 keeps the
  lambda from outliving the frame that owns the handle.
- §8.2 (hot reload): the declaration hash gains the escape facts.

### 118.3 Corpus (pre-registered)

Reject, each `tsc`-clean, each S009 at a pinned position, each accepted
at `274ccf4`:

- `r240-parameter-stored-in-field`: a capturing lambda passed to a
  callee that stores it (the error is at the caller's argument).
- `r241-parameter-returned`: a capturing lambda passed to a callee that
  returns it.
- `r242-expression-body-returns-parameter`: the same through an
  expression-body lambda.
- `r243-capture-passed-to-generator`: a generator with a capturing
  argument, returned.
- `r244-capture-passed-to-async`: a held handle of an async call with a
  capturing argument.
- `r245-yield-returns-capture`: a `yield` of a capturing lambda.
- `r246-capture-through-loop-back-edge`: a capture that reaches a
  `return` only on a second loop iteration.
- `r247-generator-parameter-stored`: a generator with a capturing
  argument, passed to a callee that pushes it into an array.
- `r248-capture-through-indirect-call`: a capturing lambda passed
  through a function value.

Accept: `a262-parameter-called-and-passed-down` (calls and downward
passes, with a capturing lambda and a named function), and the existing
generator-in-a-field entries `a217`, `a220`, `a221`, `a222`, and `a225`,
which stay accepted.

A reload test: a body edit that makes a parameter escape is a rejected
swap; a body edit that keeps the escape facts is an accepted swap.

### 118.4 Goldens that move

None. No accepted program changes. If one does, stop and report.

### 118.5 Exit criteria

0. The cost of the check is stated: the checker's time over the whole
   corpus before and after (core principle 15).

1. Every reject entry fails with S009 at its pinned position, and is
   accepted at `274ccf4`.
2. Every existing accept, warn, trap, and interop entry, every example,
   every benchmark workload, and every README and tutorial program is
   accepted, as the measurement found.
3. `tools/gate.sh full` is green.
4. A fresh review has no open CRITICAL or MAJOR, and
   `tools/hygiene.sh` is clean.
