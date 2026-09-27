<!-- §120 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 120. A lifetime site names its operand

*(Added 2026-09-28.)* Origin: the review of stdlib §10.9 (batch 5b).

Problem: on the dev JIT, `new Map(m)` with a freed `m` gives
`trap [internal]: Map/Set key kind Bits with width 0 is invalid`.
`m.set(...)` and `m.size` on the same freed map give
`trap [use-after-delete]`, as §8.1a requires of the dev tier. The
result is the same for `Map<i32, i32>` and `Map<string, i32>`, measured
through `subscript run` at `e61e012`. The review reports the same hole
for `new Set(s)` with a freed `s`. A test-only JIT entry point
(`run_jit_with_freed_handle_diagnostics_and_native_libraries`) gives
`use-after-delete` at the call position for the same program, so a
test through that entry point does not show the hole.

*(Corrected 2026-09-28.)* `subscript run` runs with the retention mode
off. With the mode off, a use after `Context.free` is undefined on the
dev tier (§8.1a-1), so the `trap [internal]` above is not a contract
violation, and no result with the mode off is evidence. The defect of
the form stands on the mode-on results of the test-only entry point,
which runs the §8.1a-1 mode. With the mode on, at `e61e012`: `new
Map(x)`, `new Set<i32>(x)`, and `a.union(x)` trap at the call position,
not the operand position, because the dev JIT tests the first operand
of the call; the interpreter tests every handle operand, so the two
engines choose the operand separately.

Cause, read from the code at `26fabb8`:

- `Expr::trap_sites` (`compiler/src/hir.rs`) derives a
  `DevOnlyLifetime` site for a method receiver only. The other operand
  of a runtime operation (a constructor source, an argument of a
  container method) gets no site.
- The site carries only a position (`TrapSite::DevOnlyLifetime { pos }`,
  LIR `TrapKind::DevOnlyLifetime`). It does not name the value it
  tests, against §20.2 ("A site owns the values its guard tests").
- So each engine chooses the operand itself. The dev JIT tests the
  first operand of the call. The reference interpreter tests every
  handle operand. A second site on the same call cannot say which
  operand it tests, so adding sites alone does not close the hole.

This is a defect of the form (core principle 8), not of one site.

Measured through `subscript run` at `e61e012`, with a freed non-receiver
operand: `new Map(x)` gives `trap [internal]`; `new Set<i32>(x)` and
`Array.from(x)` complete with no trap; `a.union(x)` gives
`use-after-delete` at the call position, not the operand position.

A second defect, found by the Red measurement: the reference
interpreter stops with `invalid LIR: Map key has no layout` (or `Set`)
on a live program that copies a `Map<i32, i32>` or a `Set<i32>`.
`compute_class_layouts` (`codegen/src/interpreter.rs`) collects the
layouts of class, global, foreign-function, local, and SSA value types,
but not the key and value types inside a `Map` or `Set` type. No corpus
entry reaches the shape, so the differential gate does not see it (core
principle 12). Rule 6 needs the interpreter to reach the operation.

### 120.1 Rules

1. A `DevOnlyLifetime` site names the operand it tests: in HIR, the
   receiver or an argument index of the call; in LIR, an index into
   the instruction's evaluated operands. `Context.free` names its
   argument.
2. Every engine tests exactly the named operand of each site, in site
   order, and reports that site's position and trap kind. No engine
   tests an operand that no site names. A null operand (a nullable
   type, such as `Person | null`) is not an allocation: it passes the
   check on every engine, and no engine reads through it.
3. `Expr::trap_sites` is the one rule for which operands carry a site.
   A call to a runtime operation (a method or constructor of a built-in
   container, an ambient function, a runtime instruction such as
   `MapFromSource` or `SetFromSource`) carries one site for each operand
   that it reads through and whose type needs a lifetime trap
   (`HandleKind::needs_lifetime_trap`): the receiver first, then the
   arguments in order. The site position is the position of that
   operand.
3a. An argument whose handle the operation stores as an element, key,
   or value is not read through. It carries no site. In the runtime
   signature it is a `T`, `K`, or `V` parameter of a storing method.
   `Worker.post(x)` copies the message, so it reads through `x` and
   carries a site. `a.set(1, x)` and `xs.push(x)` copy the handle `x`; a later
   read of `x` tests it. Every other argument of a lifetime-trap type is
   read through.
3b. `Context.free` is the releasing operation, not a read. The form
   carries that fact: its site is a release site, distinct from a
   read site in HIR and LIR, so no engine identifies it by name. It
   keeps its kind and position (§8.1a: `double-delete` at the call
   position, `t22`, `t23`). Rules 3 and 6 do not apply to it.
   A read that the release itself performs (the load of an async-owner
   field before the owner is destroyed) is part of the release: it
   carries the release site, not a read site. So the first check of a
   freed argument is the release check, whatever fields the class has.
3c. A statement that reads through a value carries the site too. The
   subject of `for…of` over a `Map`, a `Set`, or their key and value
   views carries a site at the subject position, before the loop
   starts.
4. A call to a script function, a function value, or a foreign function
   carries no lifetime site for its arguments. A call to a synthesized
   helper (§119) is not a script-function call here: the program wrote
   an ambient call (`JSON.stringify(x)`), so rule 3 applies, and the
   site is at the argument position. The callee reads the
   value, and its own sites test it. A foreign function receives the
   handle as data (§8.1a); this section does not change that.
5. The ship tier keeps no check (§8.1a, §8.1b). It still matches every
   site explicitly, as §20.3 requires.
6. With the §8.1a-1 retention mode on, a freed operand of a runtime
   operation gives `trap [use-after-delete]` at the operand position on
   the dev JIT and on the reference interpreter. With the mode off, the
   use is undefined (§8.1a-1), as it is on the ship tier.

7. The reference interpreter collects the layout of every type that a
   collected type contains: the key and value types of a `Map`, the
   element type of a `Set` and of an array, and so on to the leaves.
   The collection is total over the type grammar, not a list of the
   shapes a probe reached.

### 120.2 Acceptance

1. Red first (core principle 10): at `e61e012` (the batch 5b
   implementation), measure each operation class below with a
   freed operand with the §8.1a-1 retention mode on (the dev-JIT entry
   point that sets it, and the corpus interpreter harness). A result
   with the mode off is undefined (§8.1a-1) and is not a measurement.
   Record the
   output per class in `specs/tracking/s120-lifetime-operand.md`.
2. One dev-tier test per operation class with a freed non-receiver
   operand, expecting `use-after-delete` at the operand position, with
   a live-operand firing control, with the mode on as in item 1. No
   test asserts a trap with the mode off.
   A class that no program can reach with a freed operand (the operand
   type is not accepted by `Context.free`) is recorded with the
   diagnostic that rejects it, in place of a test. The classes are at
   least: a
   constructor source (`new Map(m)`, `new Set(s)`, `Array.from(xs)`),
   the second operand of a Set algebra method, and a container argument
   of a container method. The round lists every other class that rule 3
   newly covers, with one test each.
3. A total check at build: a unit test walks every runtime-operation
   signature (`module.operation_signatures` and the built-in method
   table) and fails for any read-through parameter (rule 3a) of a
   lifetime-trap type that `Expr::trap_sites` gives no site. The LIR
   verifier also checks every instruction: a closed table over
   `InstructionKind` (a match with no wildcard arm) states which
   operands each kind reads through, and the verifier rejects an
   instruction with a read-through operand of a lifetime-trap type and
   no site that names it. Rule 3c is then a consequence, not a list. The
   check runs in both directions: the verifier also rejects a lifetime
   site on an operand that the table does not mark as read through or
   as guarded (an address computation whose result a later load or
   store reads, such as `AddressOfField`). The test builds a violating form to
   show it can fail (core principle 9).
4. The LIR text snapshot moves only by the new operand index on
   existing lifetime traps and by the new sites. No `.expected` golden
   moves. The round reports the counts.
5. A new accept entry copies a `Map<i32, i32>` and a `Set<i32>` (live)
   and runs on all three engines. It is Red on the interpreter at
   `e61e012`.
6. §8.1a and C-series entries do not change. The dev-JIT cost of the new
   sites is measured on the benchmark set (`tools/gate.sh full` covers
   it) and recorded.
