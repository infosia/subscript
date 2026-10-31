<!-- §221 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 221. A method that moves its array is rejected on an element

*(Added 2026-11-01.)* Origin: §220.4 item 1. The owner selected this
section on 2026-11-01. The measurement is in
`specs/tracking/array-element-receiver.md`.

Problem, measured at `020215de`. `all: V[]` is a dynamic array of a
`@ValueType` class. `this` in `all[0].m()` is an address into the storage
of `all`. A method body that moves that storage leaves `this` on old
storage:

| Form | three tiers | `node` |
|---|---|---|
| a growth, then `return this.b` | `0` | `2` |
| a growth, then `this.a = 7`, then `all[0].a` | `1` | `7` |
| a growth, then `for…of` over `this.a` | `10` | `12` |
| a growth, then `this.a.reduce` | `10` | `12` |
| `list.splice(i, 1); this.hp = 0` | the next element changes | that element keeps its value |

§68.2 item 9 states that an address whose base is invalidated is not
used again. The verifier checks this inside one function. `this` is an
`Address` parameter, so the base is not in the function and no check
sees it. The program gives a wrong value, with no diagnostic.

A copy of the receiver is not the answer. A prototype that copied the
element in and out closed this table and broke programs with no
movement, because `this` and `all[i]` were two objects: a traversal of a
cyclic node graph did not end, and a transfer between two array elements
lost a write. Each of these was equal to `node` at the pin. It cost 54
times the address form on a 4 KB element on the ship tier, at each call
of a method that reads one cell. The cost depended on the element size in
another declaration, which core principle 16 rejects. The tracking note
has the numbers.

### 221.1 Rules

1. A receiver of a `@ValueType` method or accessor is *unstable* if its
   place crosses a dynamic array element after the last reference handle
   of the path. A dynamic array value is a reference handle. Its *element
   types* are the element types of the dynamic arrays that it crosses
   after that handle. `all[i]`, `all[i].inner`, and `all[i].a[1]`, with a
   `FixedArray` `a`, are unstable. `r.v`, `g`, a local, and `rs[0].v`,
   with `rs` an array of a reference class, are stable. `nested[0][0]` on
   `V[][]` is unstable with the element type `V` only, because a move of
   the outer array does not move the inner storage.
2. An operation *moves* a dynamic array of element type `E` if it can
   change the storage or the position of an element: `push`, `unshift`,
   `shift`, `splice`, `sort`, `reverse`, an assignment to `length`, and
   `copyWithin`, and each other operation of that kind in the prelude.
   `pop` does not move an element. An assignment to one element and `fill`
   do not move an element: they store at an index.
3. A call on an unstable receiver is rejected if the method can reach an
   operation that moves a dynamic array of one of its element types.
4. Reach is a fixed point over the call graph of the whole program. It
   follows a direct call, a method call, a constructor, an accessor, a
   parameter default, a `using` dispose, a lambda in a body, a generator
   body at `next`, and an async call. An async call reaches its whole
   body, also after an `await`. An indirect call reaches each function and
   lambda that the program uses as a value, with the same signature. The
   subset has no override, because `extends` is rejected.
5. The type decides, not the array object. A method that pushes to
   another `V[]` is rejected on `all[i]`. The diagnostic names the method,
   the operation, and its position.
6. The diagnostic gives the fix. The fix depends on the operation. After
   a growth at the end (`push`), the fix is the
   spelled copy `const t: V = all[i]; t.m(); all[i] = t;`. The source then
   shows the copy and its cost. After any other move (`unshift`, `shift`,
   `splice`, `sort`, `reverse`, `copyWithin`, `length = 0`), the index no
   longer names the element, so a write back changes another element. The
   fix is to run the operation after the method returns. If a method can
   reach both kinds, the text of the second kind applies. The
   text spells the place as the source does. If the place is not a plain
   form, the text gives the fix in words.
7. A call on a stable receiver, on a reference class, and on a copy is not
   affected. An argument that moves the array stays accepted: the address
   comes after the arguments (`a150`).
8. An accepted call has no copy. It keeps the address form of §68.2, so
   `this` and `all[i]` name one object, as in `node`.
9. The check is at the build. It adds no run-time cost. On a reload the
   check runs on the new program, because a swap compiles the whole
   module.
10. A foreign call reaches each function value that crosses the boundary:
    a function argument of a foreign call, and a value stored in a mirror
    field. C can call that value during the call, or store it and call it
    at a later foreign call. A foreign function with no such value reaches
    no operation. §18 neither allows nor forbids a call of an export
    during a foreign call, so the reach has no edge to an export. §221.4
    records this.
11. The interpreter poison registry of §68.2 item 9 is per frame. It sees
    a move in one function. A value identifier is local to a function, so
    a shared registry reported a false move on a correct recursion through
    `all[0]`. Rule 3 covers a move across frames.
12. A `for…of` binding of a value element is a copy on each tier (C2,
    §220.1 rule 4). The JIT gave an address into the array storage:
    `for (const v of all) { all[0].b = 9; print(v.b); break; }` printed `9`
    on the JIT, and `2` on the ship tier and the interpreter.

### 221.2 Acceptance

1. Red first, at the pin `020215de`: a reject entry for each form of the
   table is accepted. The tracking note records the result. Each entry
   states what `tsc` does, measured.
2. A reject entry pins each reach form of rule 4 that the subset accepts.
3. An accept entry pins the forms that the prototype broke: the cyclic
   node graph, a transfer with `j` equal to self, and a recursion through
   `all[0]`. Each golden equals the `node` output, and the entry is
   js-comparable if the form allows it, or cites C2.
4. An accept entry pins rule 2 and rule 7: `pop` in a method, a `push` to
   an array of another element type, an argument that moves the array, and
   the spelled copy of rule 6.
5. `a150-receiver-address-invalidation` keeps its golden.
6. A unit test pins rule 4 for an indirect call, with a control: a
   program with no such value is accepted.
7. The rejection-site total check passes with the new site.
8. Report each accept and example entry that the rule rejects. The
   tracking note names each one.
9. No `.expected` file moves.
9a. An accept entry pins rule 12, and a reject entry pins rule 10 with a
    mirror callback that moves the array.
10. A build of the largest corpus entry has no measurable check cost: the
    tracking note gives the time of the pass.

### 221.3 Phase Review

*(Filled at the end of the phase.)*

### 221.4 Open

*(Filled at the end of the phase.)*
