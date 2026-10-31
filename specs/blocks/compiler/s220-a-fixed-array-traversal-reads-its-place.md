<!-- §220 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 220. A `FixedArray` traversal reads its place

*(Added 2026-10-31.)* Origin: §218.4 item 1. The owner selected this
section on 2026-10-31. The measurement is in
`specs/tracking/fixed-array-traversal.md`.

Problem, measured at `bb79fcd0`:

| Subject, with a write of element 1 to 9 in the body | three tiers | `node` |
|---|---|---|
| a local `b` | `1,2` | `1,9` |
| a global, changed in a called function | `1,2` | `1,9` |
| a field `r.a` | `1,2` | `1,9` |
| a nested field `r.v.a` | `1,2` | `1,9` |
| an element `m[0]` of a `FixedArray` | `1,2` | `1,9` |
| an element `m[0]` of a dynamic array | `1,2` | `1,9` |
| `g.forEach`, with a global `g` | `1,2` | `1,9` |

`collisions.md` C2 states a copy on an assignment, on a pass, and on an
index. It names no copy at a loop. Q30 states that `for…of` is an index
loop over the storage of the container. So the copy is a defect of the
lowering, not a decided divergence.

The form also has a cost. On the ship tier, 300,000 traversals of a
`FixedArray<f64, 256>` field take 0.35 s with `for…of` and 0.06 s with an
index loop. The coding agent measured the cause: the C tier keeps the
cursor in the `roots` struct and writes it three times per step. With no
copy and with the cursor, the loop still takes 0.35 s. The copy is the
cost of an early exit: 0.17 s against 0.02 s on `subscript run`.
Two copies exist: the checker lifts the subject into a `const`, and
§218.1 rule 12 adds a hidden local.

### 220.1 Rules

1. If the subject of a `for…of` or the receiver of a callback method
   over a `FixedArray` is a place, the traversal reads the place. Each
   step reads the element from the place at that step.
2. The traversal evaluates the place one time, at the start. A
   reference handle, an index, and each other operand of the place keep
   the value of that evaluation. `r.a` with `r = s` in the body stays on
   the first `r`, as `node` does.
3. No address lives across a step. Each step computes the element
   address from the kept operands. A suspension in the body copies no
   array. The one exception is `this` in a `@ValueType` method, an
   `Address` parameter that each step reads. §220.4 item 1 records its
   effect.
3a. `Context.free` of a base in the body gives the Q6 behaviour of an
   access after a delete at the next step: a dev trap, and an
   unspecified read on the ship tier (`t23`).
4. A step binds the element by the C2 copy on an index. A write to the
   binding does not reach the array.
5. A whole assignment to the place in the body changes what the next
   steps read, because an assignment copies into the storage of the
   place (C2). `b = c` with `b` a `FixedArray` gives the new elements.
   `node` rebinds `b` and reads the old array. `collisions.md` C2 names
   this divergence.
6. A dynamic array element at any depth of the subject keeps its index.
   `m[0]` and `m[0].a` have this form. A step
   checks that index as an index read does. After a removal that makes
   the index invalid, the step traps with the trap of that read. `node`
   keeps the array object. `collisions.md` C2 names this divergence.
7. A subject that is not a place, such as a call result, keeps the hidden
   local of §218.1 rule 12. The loop exit stores a zero value in that
   local, so `Context.collect()` after the loop does not keep its handles.
   If the element type is counted, that local is the one owner of the
   elements, and the loop exit releases it. A raise that leaves the loop
   skips the store, and the local holds its handles until the function
   returns.
8. A `FixedArray` traversal has no cursor. It is an index binding, a
   compare with the length `N` of the type, and an element read. The
   form has no run-time cost beyond the element read. The traversal
   allocates nothing and copies no array. The checker does not lift a
   `FixedArray` subject into a `const`.
9. A pass to a function stays a copy (C2). A generator parameter stays a
   copy. A const local that a lambda captures stays a copy (C5).
10. The rule applies to each `FixedArray` callback method that the
    prelude declares, and to `for…of`. A callback method is an inline
    loop, not a runtime call.
10b. `reduce` and `reduceRight` with a lambda evaluate the seed before
    they build the closure, so `await` in the seed is accepted. A lambda
    has no effect of its own, so the order is not observable.
10a. A traversal has no check at the start. The first step checks the
    place, so a place has one read site. With `N = 0`, a place with an
    invalid index does not trap. An argument of a method, such as the
    seed of `reduce`, runs before that check. *(Added 2026-10-31: the
    coding agent measured these forms.)*
11. The tiers agree. The interpreter, the JIT, and the C tier read the same
    place at each step.

### 220.2 Acceptance

1. Red first, at the pin `bb79fcd0`: a new accept entry with each form of
   the table gives output that differs from its golden. The tracking
   note records the output.
2. The entry has `js-comparable: yes`, and its golden equals the output
   of `node`.
3. A second accept entry pins rule 5 and rule 6. Its header cites C2 and
   states why it is not comparable. A trap entry pins the removal of
   rule 6.
4. A corpus entry pins a traversal with `yield` and `await` in the body
   over a place of `string` elements, on the three tiers.
5. A unit test pins rule 3: the LIR of a traversal over a place has no
   hidden local and no `Address` value that lives across a step.
6. A unit test pins rule 7 with a control: a handle of a call-result
   subject is released by `Context.collect()` after the loop, and a
   handle that the program still holds stays.
7. The ship-tier cost table of the tracking note shows that the loop of
   the problem statement is near the index loop. The figure is a
   measurement, not a limit.
8. No golden moves, except a golden that the tracking note names with
   the rule that moves it.

### 220.3 Phase Review

*(Added 2026-10-31.)* A fresh review of the tree on the contract commit
gave three MAJOR and five MINOR findings.

1. MAJOR, realistic, fixed: a `for…of` over a call result of type
   `FixedArray<Generator<T>, N>` or `FixedArray<Promise<T>, N>` gave
   invalid LIR. `1 2 5 6` before §220. Rule 7 makes the hidden local the
   one owner. `a409` pins it.
2. MAJOR, realistic, rare shape, not fixed: item 1 of §220.4. Rule 3
   names the exception. The owner line for a rare shape applies: the
   defect is in the receiver form, and it predates §220 on the index
   loop.
3. MAJOR, realistic, rare shape, fixed: `await` in the seed of `reduce`
   with a lambda gave invalid LIR on both receivers. Rule 10b. `a410` pins
   it.
4. MINOR, fixed: eight `fixed_*` runtime functions had no caller.
5. MINOR, fixed: the `IteratorCreate` subject check lost its test.
6. MINOR, fixed: contract text on the cursor, on the depth of a dynamic
   array element, and the C2 citation.
7. MINOR, contrived, fixed in the contract: `Context.free` of a base in
   the body. Rule 3a.
8. MINOR, open: item 2 of §220.4.

The coding agent found one defect at the pin in the M1 fix. A counted
local with storage and a state binding live at a `yield` gave invalid
LIR: `edge 5 -> 6 has type Data(Generator(I32)), expected Data(I32)`.
`terminate` pushed the loaded owner after the state parameters. The fix
is in `codegen/src/lir/builder.rs`, and `a409` pins it.

### 220.4 Open

1. `this` in a `@ValueType` method is an address into the storage of the
   receiver. If the receiver is a dynamic array element and the body grows
   that array, later reads use old storage. `all[0].walk()` with a
   `grow()` of 5,000 `push` calls in the body gives `10` on the three
   tiers for `for…of` over `this.a` and for `this.a.reduce`, and `node`
   gives `12`. Before §220 these two forms gave `12`. A `this.a[i]` index
   loop gives `0`, before §220 too. A fix belongs to the receiver form and
   closes all three forms.
2. An unused `const e: FixedArray<i32, 0>[] = []` fails the C compile:
   `use of undeclared identifier 'SubFA_i32_0'`. `subscript run` accepts
   it. Contrived. §218.4 item 2 has the same type.
3. `ElementSource` in the runtime has one variant.
