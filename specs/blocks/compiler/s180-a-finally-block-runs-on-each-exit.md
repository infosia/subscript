<!-- §180 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 180. A `finally` block runs on each exit

*(Added 2026-10-08.)* Origin: the owner's review of the async surface
on 2026-10-08. On 2026-10-08 the owner selected `finally`, completion
replacement as JavaScript does it (rule 3), and generator close at a
for-of exit (rule 7). The measurement round at `3de68c37` is
`specs/tracking/s180-finally-measurement.md`.

Problem: `try { … } finally { … }` is `tsc`-clean and fails with S010
(§115.3 rule 2; `r233-finally`). No record states a reason: the §115
owner decisions were three other points. A cleanup after `await` is a
common TypeScript form, and the language has no equivalent for code
that is not a resource (`using` needs a disposable object).

The measurement prototype reused the `using` exit actions. The
accepted forms of the node programs (normal exit, `return`, `break`,
`continue`, caught and uncaught exceptions, nested `try`, async with
and without `catch`, `await` in `finally`, `using` with `finally`)
print the node output in the three tiers. 269 interpreter corpus
entries match; the cost is 0.98 of the pin. Two controls show defects
that a complete form must close: two suspending finalizers exchange
their exceptions through the Context-wide park stack, and a callee
that throws inside a finalizer during an exception exit traps (kind
30) where node replaces the exception.

### 180.1 Rules

1. **Forms.** `try { … } finally { … }` and
   `try { … } catch … { … } finally { … }` are accepted.
2. **When the finalizer runs.** The finalizer runs once on each exit
   of the `try` block and of the `catch` block: fall-through,
   `return`, `break`, `continue`, and an exception. Nested statements
   run their finalizers from the inner to the outer, interleaved with
   `using` disposal in lexical order. A trap runs no finalizer (C6;
   a trap runs no dispose hook either).
3. **Completion.** When the finalizer falls through, the prior
   completion continues: the return value that was evaluated before
   the finalizer, the same exception object with its last-throw
   position (§116.1 rule 4), or the same jump. A `return`, `throw`,
   `break`, or `continue` in the finalizer, and an exception that
   leaves it, replace the prior completion as JavaScript does. A
   replaced counted return value is released. Each outer finalizer
   and `using` hook still runs for the new completion.
4. **`using` keeps its rule.** A dispose hook that throws while an
   exception propagates through the hook's own scope exit traps (kind
   30, §115). Inside a finalizer, the held exception does not
   propagate through a hook that the finalizer runs: an exception from
   that hook is an exception of the finalizer, so a local `catch` takes
   it, and one that leaves the finalizer replaces the held exception
   (rule 3), as JavaScript does.
5. **Frame-owned exception storage.** A finalizer can hold `await`
   and `yield`. The exception that a finalizer holds belongs to the
   frame of the finalizer, not to a Context-wide stack: two suspended
   finalizers keep their own exceptions, in the three tiers.
6. **Async.** An exception that leaves an async body after its
   finalizers completes the handle (§116).
7. **Generator close at a for-of exit.** When a for-of over a
   generator ends by `break`, `return`, `continue` to an outer
   loop, or an exception from the loop body, the loop closes the
   generator before it releases its holder: the generator runs the
   finalizers that enclose its current `yield`, from the inner to the
   outer, and becomes exhausted. Exhaustion closes nothing. A plain
   drop runs no finalizer (§176 rule 3 stands; node runs none). A
   later `next()` of a closed generator returns `done`. An exception
   that leaves the generator body during close traps (§115.4 item 3).
   A `yield` in a finalizer during close suspends the generator and
   ends the close, as JavaScript does; the loop exit continues.
   `generator.return()` stays outside the language (S100).
8. The three tiers give the same output, traps, and positions.

### 180.2 Acceptance

1. Red first, at the contract pin (S010 at `finally`):
   - accept entry `a349`: the exits of rule 2, the replacements of
     rule 3 (each of `return`, `throw`, `break`, `continue` in a
     finalizer, and a callee that throws in a finalizer during an
     exception exit), nested finalizers with `using`, and a counted
     return value replaced by a finalizer return; `js-comparable: yes`
     if `node` agrees;
   - accept entry `a350`: async finalizers: `await` in a finalizer, two
     suspended finalizers with their own exceptions (the measurement's
     concurrent control), and the identity of an exception kept across
     `Context.collect()` in a finalizer;
   - accept entry `a351`: generator close on for-of `break`, `return`,
     and an exception from the loop body; a plain drop; a closed alias
     whose `next()` returns `done`;
   - trap entry `t109`: an exception that leaves a generator finalizer
     during close traps (rule 7).
   `r233-finally` is retired (`retired:r233-finally`).
2. Unit tests: the LIR verifier rejects a finalizer exit with no
   completion record; a test reads each tier's frame-owned exception
   slot with two suspended finalizers (core principle 9: built state,
   a same-shape control with one finalizer).
3. Cost: measure and record the interpreter corpus and the benchmark
   workloads against the pin (release, best of three, each binary
   alone, with no other load on the host). Attribute a growth only
   when it is outside the run-to-run spread that
   `specs/blocks/benchmarks.md` records, or when a code cause is in
   sight.
4. Goldens: the LIR text golden moves for entries that hold async or
   generator functions; the generated docs move. No other
   `.expected` golden moves.

### 180.3 Sections this one amends

- §115.3 rule 2: removed (`finally` is accepted).
- Collision C6: `finally` is accepted.
- §176 rule 3: a for-of exit closes the generator before the holder
  release (rule 7); a plain drop still runs no body code.
- §60/§97: a finalizer and a dispose hook share the exit order of
  rule 2.
