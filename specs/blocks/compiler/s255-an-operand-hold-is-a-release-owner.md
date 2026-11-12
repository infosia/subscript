<!-- §255 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 255. An operand hold is a release owner

*(Added 2026-11-12.)* Origin: §236.4 item 4. Selected by the owner on
2026-11-12.

Problem, measured at `96faa31e`. `v(): Promise<i32>` is async, and
`two(a: Promise<i32>, b: Promise<i32>): Promise<i32>` awaits both.

| Program | Release model of the LIR test | Output, three tiers |
|---|---|---|
| `await two(v(), v())` | "exception edge releases 1 owned handles; lexical scopes require 0" at the call | `2` |
| `two(v(), boom())` in a `try`, `boom` throws | two findings | `caught`, `1` |
| `two(bad(), v())` in a `try`, `bad` rejects | one finding | `caught`, `1` |
| `const a = v(); const b = v(); await two(a, b)` | none | `2` |
| `await takeArr(arr(), v())`, `arr(): Promise<i32>[]` | one finding at the call | `2` |
| `await takeGen(gen(), v())`, `gen(): Generator<i32>` | one finding at the call | `1` |

The lowering is correct. An operand of a counted type (an async handle,
an array that holds handles, a generator) that is not an immutable local is held to the end of its expression when a later
operand runs user code. The hold takes one count, and releases it on the
normal path and on each exception edge after it. The counts balance on
each path, and the memory accounting of the development tier finds no
handle that stays live and no handle released twice. The release model
(`codegen/tests/support/lir_facts_release.rs`) counts only the lexical
owners of the HIR, so it requires 0 releases where the lowering releases
1. No section states the hold, so the model has no rule to read.

### 255.1 Rules

1. An operand hold. A value of a counted type (an async handle, a
   generator, or an array or other container that holds a counted type
   at any depth) is held at these sites. Each hold takes one count.
   1. The receiver of a method call, if an argument or a parameter
      default of the callee runs user code, or if the method is `next`
      of a generator. A read of an immutable local is not held.
   2. An argument of a call, a `new`, an async call, or the creation of
      an async handle, if a later argument, or the default of an
      omitted parameter, runs user code. A read of an immutable local is
      not held.
   3. The default value of an omitted parameter, if the default of a
      later parameter runs user code.
   4. An element of an array literal, a spread element included, if a
      later element runs user code. A read of an immutable local is not
      held.
   5. The base of an indexed place that is not a `FixedArray` place, if
      the index runs user code: a read, an assignment target, and a
      compound or update target. A read of an immutable local is not
      held.
   6. The base of an assignment to an indexed place that is not a
      `FixedArray` place, if the value runs user code. A read of an
      immutable local is not held. If sites 5 and 6 both apply, the base
      holds two counts.

   An immutable local is a `const` declaration, a `const` binding of a
   `for…of` loop over a generator, or a capture of a lambda that owns
   its environment (an async lambda). A parameter, a `let`, a binding of
   a `for…of` loop over an array, and a capture of a lambda that borrows
   its environment are not. The lowering stores each element of an array
   loop into one slot, also when the binding is `const`, and passes a
   borrowed capture as a parameter.
2. An operand runs user code if it contains a call, an async call, the
   creation of an async handle by a call of an async function, an
   `await`, a `new`, or a `yield`, outside the body of a lambda.
   `Promise.all` does not run user code.
3. A held operand owns its count from its evaluation to the end of the
   innermost enclosing call, array literal, read, or assignment. The
   base of a subject of a `for…of` loop over a `FixedArray` place owns
   its count to the end of the loop. Each exit from that span releases the count
   once: the normal path and each exception edge.
4. The release model of the LIR test reads rules 1 to 3 from the HIR. At
   each exception edge it requires the lexical owners plus the operand
   holds whose span contains the edge. A parameter default is a body of
   its own, as the lowering emits it once per parameter: its edges count
   once, with the parameters before it as its owners. Each call that
   evaluates a default that can raise adds one edge at the call, with the
   owners and holds of the caller. The model does not read the
   lowering.
5. The lowering does not change. No program changes its output.

### 255.2 Acceptance

1. Red first, at the pin: the accept entry
   `a439-an-operand-hold-is-a-release-owner` fails the release model.
   It has the five forms of the table that the model rejects, prints
   each result, and is `js-comparable`. The tracking note records the
   findings.
2. After the change, `a439` passes the release model, the three tiers,
   and `node`.
3. A unit test of the model builds the violating form: the LIR of
   `two(v(), v())` with the release of the hold on the exception edge
   removed. The model reports it. A control: the same LIR with the
   release passes.
4. `main` of `a439` is async, so the LIR text snapshot grows by the
   `a439` block. No other golden moves. If another corpus entry or
   golden changes, report it and stop.

### 255.3 Phase Review

Two fresh reviews of the implementation. No CRITICAL finding.

1. MAJOR, realistic, fixed (first review): the lowering took holds at
   sites that rule 1 did not name, so a correct program failed the
   model: `tasks.push(start(i))` with a `let tasks`, a read
   `hs[it(1)]`, and the second hold of an assignment base. An audit of
   each hold site of the lowering gave the six sites of rule 1.
2. MINOR, fixed (first review): the model counted `Promise.all` as user
   code, and the lowering does not. Rule 2 states it.
3. MAJOR, realistic, fixed (second review): the model counted the edges
   in a parameter default once per call. The lowering emits a default
   once per parameter, so a function with a raising default and two
   calls failed the model. Rule 4 states the count.
4. MINOR, contrived, fixed (second review): the hold of a `for…of`
   subject over a `FixedArray` place lasts to the end of the loop. Rule 3
   states it.
5. MAJOR, realistic, recorded (second review): §255.4 item 4. It is a
   defect of the lowering that is older than §255.

Before the implementation, the coding agent stopped three times on a
difference between rule 1 and the lowering: a counted operand that is
not an async handle, a binding of a `for…of` loop, and a capture. Each
stop amended rule 1. An audit of each binding of the lowering then
closed the list.

### 255.4 Open

1. A `const` binding of a `for…of` loop over an array cannot change in one
   expression, so its hold of rule 1 takes a count that no exit needs:
   `for (const h of hs) { await two(h, v()); }` does one more retain and
   release per call than `const k = h; await two(k, v());`. The output
   is the same. The cost is not measured.
2. A capture of a lambda that borrows its environment cannot change in
   one expression either, and rule 1 holds it in the same way.
3. If sites 5 and 6 of rule 1 both apply, the second count is not
   needed: the hold of site 5 already keeps the base through the value.
   `hs[idx()] = boom()` with a `let hs` does one more retain and release
   than one hold needs. The output is the same.
4. A counted parameter default followed by another omitted default stops
   the lowering: `all(xs: Promise<i32>[] = [], k: i32 = 0)` called as
   `all()` gives "stores counted operand 0 (value 0) without a fresh
   single-use owner or a preceding retain". This is older than §255.
