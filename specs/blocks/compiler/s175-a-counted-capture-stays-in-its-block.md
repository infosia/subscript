<!-- §175 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 175. A counted capture stays in its block

*(Added 2026-10-07.)* Origin: `compiler.md` §171.3 item 3. On
2026-10-07 the owner selected the static rejection form over a
frame-owned count.

Problem: a lambda borrows a captured counted value (§116.1 rule 4c).
§118 keeps the lambda inside its defining function, but not inside the
block of the binding that it captures. The exit of that block releases
the binding (§171), and a later call of the lambda reads a released
value.

Measured at `ce0ce709`, dev JIT:

```ts
async function work(n: i32): Promise<i32> { return n; }
async function reuse(): Promise<i32> { const h = work(99); return await h; }
export async function main(): Promise<void> {
  let f: () => Promise<i32> = () => work(0);
  {
    const h = work(7);
    f = () => h;
    print(`before ${await f()}`);
  }
  print(`reuse ${await reuse()}`);
  print(`after ${await f()}`);
}
```

The program prints `before 7` and `reuse 99`, then traps
`use-after-delete` at the captured read. `node` prints `after 7`. The
§171 tracking note (`specs/tracking/s171-counted-holders.md`, "Counted
captures that outlive their source block") records the same failure in
the C AOT tier and the interpreter, for `Promise<i32>`,
`Promise<i32>[]`, and `IterResult<Promise<i32>[]>`.

The same shapes with an `i32`, an `i32[]`, a reference-class object, or
a `Generator` capture print the `node` output at `ce0ce709`, because a
block exit releases no count for them.

### 175.1 Rules

1. **Capture scope.** A value of a carrier type (§118.1 rule 1) has a
   set of capture blocks. A lambda literal has the block of each
   counted (§171 rule 1) binding that it captures, and the capture
   blocks of each captured value of a carrier type. Every other value
   gets the union of the capture blocks of the values that flow into
   it, by §118.1 rules 3 and 4. A loop body is the block of a binding
   that it declares, and each iteration exits it.
2. **Local facts are flow-insensitive.** A local gets the union over
   every assignment to it anywhere in its function, joined to a fixed
   point before any use is checked (§118.1 rule 3).
3. **The local boundary.** An assignment or an initializer that gives a
   local a value with a capture block `B` is rejected if the local is
   declared outside `B`. A local declared in `B`, or in a block inside
   `B`, is accepted. The check does not depend on a later call of the
   lambda.
4. **The diagnostic.** The rejection is S009 at the assigned value, at
   a new `Diverges` site of §154, `CaptureOutlivesBlock`, with
   collision C5. The message names the captured binding and the local.
   It states that the captured binding is released when its block
   exits.
5. **Other boundaries.** A binding of the function body has the body as
   its block, so a capture of it never gives a rejection under rule 3.
   §118.1 rule 5 boundaries keep their rule, and a value that reaches
   one of them is not checked again under this section.
6. **No runtime change.** No LIR, codegen, or runtime behaviour
   changes. Every accepted program keeps its output in the three tiers.

### 175.2 Acceptance

1. Red first, at the contract pin: reject entry `r396`, `tsc:
   accepts`, one S009 for each shape: a direct assignment to an outer
   `let` (the program above); a capture through a `const` function
   value of the block; a lambda that captures a function value that
   captures the counted binding; and a loop body that assigns to a
   `let` outside the loop. Each shape is accepted at the pin, and the
   pin result in each tier is recorded.
2. Accept entry `a345`, `js-comparable: yes`: the same four shapes with
   the local declared in the captured block; an outer-block capture
   assigned to an outer `let`; an `i32` capture and a reference-object
   capture assigned to an outer `let`; and a `const` copy of a parameter
   in the function body, captured in an inner block.
3. Unit tests: one test per rule 1 shape, each with a same-shape
   control that rule 3 accepts; the §154 total test with the new site
   and its witness.
4. The cost of the check is stated: the checker time over the whole
   corpus before and after (core principle 15).
5. The LIR text golden moves only for the new accept entry, if its
   selection includes it. No other `.expected` golden moves.
6. Every existing accept, warn, trap, and interop entry, every example,
   and every benchmark workload stays accepted. If one becomes
   rejected, stop and report it.

### 175.3 Sections this one amends

- §171.3 item 3: this section closes it.
- Collision C5: a lambda that captures a counted binding stays in the
  block of that binding.

### 175.4 Open

The Phase Review found these. None is CRITICAL or MAJOR.

1. *(Closed.)* The checker time over the corpus increased 7.40% (debug,
   best of three): `fact` allocated a `Vec` on each call of the §118
   hot path, and the block work ran when no lambda captured a counted
   binding. Both causes are removed; the ratio to the pin is 1.011
   (`specs/tracking/s175-counted-capture-block.md`).
2. Rule 3 rejects a for-of binding and a `const` copy of an element of
   a live array. The array keeps its count, so the pin prints the
   `node` output for these shapes, and the message "is released when
   its block exits" is not true for them.
3. The test name `each_loop_iteration_owns_its_counted_bindings` says
   "owns" for a for-of binding, which borrows.
4. `uncounted_bindings_keep_the_same_outer_assignment` has no rejected
   control in the shape of its four accepted programs.
5. The four reject entries share the id `r396`. No test needs a unique
   id; C5 cites the full names.
