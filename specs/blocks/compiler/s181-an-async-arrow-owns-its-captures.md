<!-- §181 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 181. An async arrow owns its captures

*(Added 2026-10-09.)* Origin: the owner's review of the async surface
on 2026-10-08, which asked whether the compiler can do the rewrite to
a class with fields and an async method itself. On 2026-10-08 the
owner selected this section, after `finally` (§180). The measurement
round at `f561804d` is `specs/tracking/s181-async-closure-measurement.md`.

Problem: an async arrow that captures a binding fails with S009
(collision C24; §167 rule 5; the explanation is "An async
callable uses a null environment"). The capturing async arrow is the most common
TypeScript async callback form. TypeScript accepts every measured
program, and `node` prints its output:

| Shape | `node` | Today |
|---|---|---|
| call a capturing arrow directly; hold it; pass it to a function that awaits it later | `10` | S009 |
| store it in a field and call it after the defining function returns | `7` | S009 |
| create one per loop iteration, each capturing the iteration's `const` | `0`, `1`, `2` | S009 |
| capture a handle, a reference object, a string | `7`, `8`, `seven` | S009 |

The accepted equivalent is a class whose fields hold the captured
values and whose async method does the work. A source rewrite of each
shape to that class form prints the `node` output in the three tiers,
and creates and calls it at the cost of the hand-written class (0.99,
release, 10,000 calls). Each creation adds one Context allocation;
`Context.collect()` reclaims an unreachable one, as it reclaims the
hand-written class.

### 181.1 Rules

1. **Captures.** An async arrow (§167) can capture a `const` binding
   of an enclosing function. A captured `let` or `var` binding stays
   S009; its message says to copy the value into a `const` first, or
   to use a class with a field. A parameter is captured as a `const`
   binding is, if the checker treats it as one for synchronous
   lambdas; otherwise the existing rule stands. A capture of `this`
   stays S009 (an async method value stays rejected, §167 rule 9).
2. **Environment.** Each evaluation of a capturing async arrow
   allocates one environment: a hidden reference object with one field
   for each captured binding, in the class layout (§172). The arrow's
   value carries its code and its environment. A non-capturing async
   arrow keeps the null environment of §167.
3. **Counted captures.** Storing a counted value (a handle, a
   generator, a counted container) into an environment field acquires
   one count; collection and `Context.free` release it, as for any
   reference-object field (§172). §175 (a counted capture stays in its
   block) does not apply to a capture of an async arrow, because the
   environment owns its own count. It still applies to a synchronous
   lambda.
4. **Roots.** An environment is reachable while a reachable value
   holds the arrow, and while an async frame that a call of the arrow
   started is live: the frame roots its environment from its first
   instruction to its completion. Each tier implements this root.
   A function value carries a managed word: every storage that can hold
   a function value (a local, a parameter, a global, a temporary that
   lives across a call or a suspension, a frame slot, a field, an array
   element, a `Map` value) roots the environment word of the value it
   holds, in each tier.
5. **No implicit collection.** An unreachable environment stays
   allocated until an explicit collection or Context release
   (invariant 2).
6. **Values.** A capturing async arrow has the type of an async
   function value (§167): it can be called, held, passed, stored in a
   field or an array, and awaited through its handle, with the §70
   rules.
7. **Hot reload** follows §167 rule 10: an arrow value keeps the body
   and the environment layout of the module that created it; a frame
   that its call suspends follows the stale-frame rule.
8. `Array.prototype.map` with a counted callback result stays
   rejected (§171 rule 5a, S014).
9. The three tiers give the same output and the same collection
   results.

### 181.2 Acceptance

1. Red first, at the contract pin: accept entry `a353` with the shapes
   of the problem table (direct, held, passed and awaited later, stored
   in a field and called after the defining function returns, one per
   loop iteration, captured handle, reference object, and string), and
   a `Context.collect()` while a call of a capturing arrow is
   suspended; `js-comparable: yes` if `node` agrees. Reject entry
   `r398`: a captured `let` (S009 with the new message).
2. Unit tests: a test for each holder kind of rule 4 (a synchronous
   local, an async local across an `await`, a synchronous parameter, a
   global, a field, an array element), each with a `Context.collect()`
   while the arrow is held and same-size allocations after it, in the
   three tiers; a check that compares the root registration of each
   function-value storage with a fact derived separately (core
   principle 9), with a test that builds a storage with no root; a
   collection test: an unreachable environment that holds a counted
   handle is released by `Context.collect()` (counts flat over many
   creations) in the three tiers, with a same-shape control that keeps
   it reachable.
3. Cost: measure and record the creation and call of a capturing
   async arrow against the hand-written class form, and the benchmark
   workloads against the pin.
4. Goldens: the LIR text golden moves for `a353`; the generated docs
   move. No other `.expected` golden moves.

### 181.3 Sections this one amends

- §167 rule 5 (a capturing async arrow is rejected): replaced by rules
  1–7 for `const` captures.
- §175: rule 3 above.
- Collision C24: the capturing async arrow row.
