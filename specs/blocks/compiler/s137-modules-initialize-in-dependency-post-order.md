<!-- §137 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 137. Modules initialize in dependency post-order

*(Added 2026-09-30.)* Origin: the module initialization order, which
the owner ranked low on 2026-09-30 with the instruction to measure the
cost of dependency post-order first. The Step 0 round is
`specs/tracking/module-init-order.md`.

Problem: module globals and top-level statements initialize in
discovery order (§128 rule 8), and the entry module is always first.
Measured at `2d86edb7`:

- An entry global initializer that reads an imported global, directly
  or through a call, is S100 ("accessed before declaration"). `main`
  imports `vb` from `b`, `b` imports `va` from `a`, and each module
  global reads the imported one: two S100 here; `tsc` accepts, and
  `node` prints `a / b / c / main / 5`.
- A module that only `import type` reaches runs its initializer here,
  before or after its importer by discovery order. The C18 program
  prints `a init / b init / 2` here and `b init / 2` in `node`. `tsc`
  erases `import type`. Rule 3 keeps this divergence and states why.

Cost of the prototype (dependency post-order): the order is one fact
that the checker sets and the LIR lowering reads; the interpreter, the
dev JIT, the ship C tier, workers, and hot reload read one initializer
function and do not change. The ordering costs 118–161 µs for 401
modules and 799 edges. The run-time cost is zero: the emitted C
changes only the statement order inside `subscript_init`. Under the
prototype no golden, LIR snapshot, emitted C test, reject, warn, trap,
or `node` comparison moved; two unit tests that pin the S100 above
moved.

### 137.1 Rules

1. Module initializers run in `node`'s ESM evaluation order: a
   depth-first search from the entry module over its edges in source
   order marks a module when it enters it and runs the module after
   its last edge returns. An edge to a marked module is skipped, so a
   module in a cycle runs when the search completes it.
2. The edges are every import declaration, an `import type`
   declaration included, every `export { … } from`, and every
   `export * from`. An edge to a mirror is not an edge.
3. Every loaded module runs its initializer, a module that only
   `import type` declarations reach included. This is a divergence
   from `node`, where `tsc` erases `import type` and the module never
   runs; C18 records it. *(Changed 2026-09-30 after the second Phase
   Review. The first text did not run such a module, on the premise
   that no value path reads its globals (§134 rule 2). The premise is
   false: `JSON.parse<A>` builds an `A` from a type-only import, and a
   method of `A` then reads the globals of its module. Measured:
   SIGSEGV on the dev JIT, where the pin prints `a init / 1 / hello /
   3`. A class type reaches a value through every producer that is not
   a constructor call, and through the fields of another class, so no
   site rule closes the class; a module that always runs makes it
   unreachable. The owner chose the first text on 2026-09-30 on that
   premise.)* `hir::Global` keeps its initializer; no global is
   without one.
4. The file order, the module identities, the `globals` order, and
   the emitted declaration order do not change. The run order is a
   separate fact of the HIR that the LIR lowering reads: the ordered
   list of the identities of every module, a module with no
   initializer included. The initializer segments follow that list,
   and the reload check of rule 3c compares it.
3b. A regex literal global is not a module global. Every regex literal
   global of the program initializes before the first module
   initializer, whichever file's check made it. The LIR lowering
   verifies that every global is in exactly one initializer segment or
   is a regex literal global, and fails with an error otherwise. *(Added 2026-09-30 after the Phase Review. A regex
   literal in a generic body belonged to the file whose check first
   instantiated the body; when that file did not run, or ran later,
   the regex was not initialized: `hit<i32>` with `/a+/` printed
   `false` where the pin and `node` print `true`. The same defect
   exists at the pin inside one file: a global initializer that calls
   a later function with a regex literal prints `false` on all three
   engines, `true` in `node`.)*
3c. A hot-reload swap that changes the run order list of rule 4 is
   refused with the reload diagnostic that names both lists. *(Added 2026-09-30 after the Phase Review: a swap
   from `import type { A }` to `import { A }` was accepted, the
   initializer of `a` never ran, and the process ended with SIGSEGV on
   the first read of its globals.)*
5. The initializer-order check (a global read before its initializer
   ran) covers every file of the program: the order inside a module is
   its source order, and the order between modules is the run order of
   rule 1. A read across a cycle that the run order does not satisfy
   stays S100.
5a. The check covers a top-level statement as it covers a global
   initializer: a top-level statement that reads a global, directly or
   through a call route, before that global's initializer ran is S100
   with the route. *(Added 2026-09-30 after the Phase Review. At the
   pin, `hook(); const m: Foo = new Foo();` in one file, where `hook`
   reads `m`, ends with SIGSEGV; `node` throws a ReferenceError. With
   the run order, the same shape across a cycle, which printed at the
   pin, ends with SIGSEGV.)*
5b. The route scan is total. It matches every statement kind and every
   callee kind of the HIR with no catch-all arm, so a new kind does
   not build until the scan names it. A `using` declaration is a call
   of its dispose hook at the end of its scope.
   - Units. Every function body, method body, lambda body, and
     generator body is a unit with a summary: the globals it reads,
     its direct calls, the function values it makes, and whether it
     has an indirect call. A lambda literal of the HIR carries an id
     that the checker assigns, unique in the checked program (each
     instance of a generic body has its own); no consumer other than
     the scan reads it, and no emitted output changes.
   - Made values. A lambda literal, a declared function or method used
     as a value, and a generator creation each make a value. A call of
     a declared async function is a direct call of its whole body.
   - Indirect calls. A call of a function value, a callback of a
     built-in method whose argument is not a lambda literal or a
     declared function, a step of a generator value (`next`,
     `for…of`), a method call whose receiver or method the scan cannot
     resolve, a recorded call with no summary, and a foreign call. A
     built-in callback whose argument is a lambda literal or a declared
     function is a direct call of that unit. Whether a built-in method
     takes a callback is an exhaustive match with no default.
   - Items. The scan takes the top-level statements and global
     initializers one by one in run order. Item k has the set M(k): the
     values made by every earlier item and every value that item k can
     make, with no order inside the item, because a loop or a
     generator can run a value made later in the same item. An
     indirect call inside item k can run any value in M(k). The reads
     of item k are the reads of its direct closure, and, if that
     closure has an indirect call, the reads of the closure of every
     unit in M(k). The closure is a fixed point; the scan computes the
     reads of each unit once per item and keeps one parent link per
     unit, and it builds a route only for a reported read, so the cost
     is linear in the units for each item.
   - The global under initialization is not yet initialized.
   *(Rewritten 2026-10-01 after the fifth Phase Review. The fourth
   text kept the made set in walk order: a loop ran a value made in an
   earlier iteration at a call the walk had passed, and a generator
   step ran a value made after the generator, both accepted and ended
   with SIGSEGV. It followed every value at a built-in callback, so a
   top-level `forEach` with a lambda was rejected when an earlier
   stored function read a later global. Its fixed point followed every
   path, and ten callbacks made `check` run for minutes. The third
   text made an indirect call a read of every global and rejected
   every global that a host call initializes. The first two texts
   made async and generator calls indirect, then traced generator
   values by local name.)*
5c. Rule 5b rejects some programs that `tsc` accepts and `node` runs:
   an indirect call can run any value made by an earlier item or by
   its own item, not only the one it runs, and an async function body
   is followed whole, not to its first `await`. The error names the
   global and the route.
6. §128 rule 8 changes: the file order is the discovery order, and the
   run order is rule 1. The C18 "Initialization order" paragraph states
   rule 1 as the order and rule 3 as the divergence.

### 137.2 Acceptance

1. Red first: an accept entry (a directory entry, `js-comparable`)
   with the measured shape: the entry and an imported module read
   imported globals in their initializers, and each module initializer
   prints. It is rejected at the contract pin; the round records the
   Red output. Goldens on all three engines and `node`.
2. An accept entry with the C18 program: a module reached only by
   `import type` runs before its importer and prints. The entry is not
   `js-comparable` and cites C18. A test pins the measured shape of
   rule 3: `JSON.parse<A>` with a type-only `A`, then a method that
   reads the globals of its module, prints on all three engines.
3. Unit tests: a cycle (`main` → `x` ⇄ `y`) runs `y`, `x`, `main`; a
   read across the cycle that the order does not satisfy is S100; an
   `import { type A }` edge runs its module; sibling imports keep their
   source order. The two unit tests in
   `compiler/tests/module_global_names.rs` that pin the S100 of an entry
   global reading an imported global move to a read across a cycle, so
   their purpose (the diagnostic names the route) is kept.
4. No existing `.expected` golden moves.
5. Tests for the Phase Review rules, each Red before its fix: rule 3b
   (the two-module generic regex shape, the later-running shape, and
   the one-file shape print `true` on all three engines; an accept
   entry, `js-comparable`, carries the one-file shape); rule 3c (the
   swap is refused, with a control swap that is accepted); rule 5 (an
   early read inside a module that only `import type` reaches is
   S100); rule 5a (the one-file shape and the
   cross-cycle shape are S100 with the route; a control where the call
   follows the initializer is accepted and runs). If rule 5a rejects a
   corpus accept, warn, or trap source or an example, the round stops
   and reports the source.
6. Tests for the second Phase Review, each Red before its fix: the
   `using` dispose shape and the foreign callback shape are S100; each
   rule 5c shape is S100 with the message, and the same shape with the
   global declared first is accepted and runs. Refusal tests assert
   the full diagnostic with both lists. Every silent test has a firing
   control in the same shape.
7. Tests for the third Phase Review: the shadowed-generator shape
   (a parameter `g` stepped after a nested `const g`) is S100;
   `const s: i32 = sum(counting());` is accepted and prints on all
   three engines; an unresolved method call is an indirect call; the
   lowering check rejects a global in two segments and an owner that
   names a missing global. The a300 entry is Red at the pin (its entry
   module prints, so the order shows). Every S100 test asserts the
   full message. A test file that compiles ship C states its cost.
8. Tests for the fourth Phase Review, each Red before its fix: a
   global that a host call initializes is accepted (with the interop
   mirror); a function whose body calls a host function initializes a
   global; a top-level `sort` with a comparator in one module and a
   later global in its importer is accepted and prints as `node`;
   `const ys: i32[] = xs.map(…)` is accepted and runs. Controls, each
   S100 with the route: a lambda passed to a built-in reads a later
   global; a callback registered with the host reads a later global
   and a later host call runs before that global; a lambda made inside
   a followed body and run by a later indirect call reads a later
   global. The measurement runs again.
9. Tests for the fifth Phase Review, each Red before its fix: the loop
   shape (a value reassigned in a loop and called in the next
   iteration) and the generator shape (a step after a later value is
   made) are S100; the four top-level shapes with a stored function
   and a built-in callback lambda are accepted and print as `node`; a
   program with twelve callbacks that call `filter` and a function
   value checks in under one second; the lowering rejects a top-level
   statement in no segment and in two segments, and a segment count
   that differs from the module count. Silent tests have a firing
   control in the same shape. The scan returns errors, not panics.
10. Tests for the sixth Phase Review: a regex literal in a module that
   is not first in the run order, after a module with a top-level
   statement, checks and prints `true` on all three engines (the
   segments that the scan validates hold no regex literal global); a
   cost test counts unit visits, not wall-clock time, and has a firing
   control of the same shape.

### 137.3 Open

An indirect call follows every function value made so far (rule 5b),
so a program is rejected when an unrelated earlier lambda reads a
later global. A scan that knows which value a call runs accepts it.

A `using` declaration in a top-level block never runs its dispose hook
(`{ using r: R = new R(); print("in"); }` prints `in` here and at the
pin; `node` prints `in / d`). The top-level form has no `Stmt::Using`,
so the scan sees no hook. The fix of that defect must give the
top-level block the `Stmt::Using` form, or it opens an early-read
route. It is a separate section.
