<!-- §139 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 139. A `using` in a top-level block disposes at the block exit

*(Added 2026-10-01.)* Origin: found during §137 (the second and third
Phase Reviews; §137.3 records it).

Problem: §60 rule 2 accepts `using` in a function block scope and
rejects a module-level `using`. A `using` inside a block that is itself
a top-level statement (a bare block, an `if` body, a loop body) is
accepted, and its dispose hook never runs. Measured at `9e333aef`, on
the dev JIT, with a class `R` whose `[Symbol.dispose]` prints
`d <name>`:

- `{ using r: R = new R("a"); print("in a"); }` prints `in a`.
- `if (true) { using q: R = new R("b"); print("in b"); }` prints
  `in b`.
- `for (let i: i32 = 0; i < 1; i++) { using s: R = new R("c");
  print("in c"); }` prints `in c`.
- The same `using` in a function body and in `main` prints the dispose
  line after the body.

`tsc` 5.9.2 accepts the program, and `node` v24.18.0 prints `in a /
d a / in b / d b / in c / d c`. The three engines agree with each
other, so the differential gate cannot see the defect (core principle
12). §137 rule 5b follows a dispose hook only through `Stmt::Using`,
which the top-level form does not carry, so an early read inside such
a hook is not scanned today.

### 139.1 Rules

1. A `using` declaration in a block of a top-level statement has the
   scope of that block. §60 rules 3 to 7 apply to it as to a `using`
   in a function block: dispose runs at every exit of the block, in
   reverse binding order, with the §97 null guard.
2. A module-level `using` (a `using` statement that is itself a
   top-level statement) stays S100 (§60 rule 2).
3. A top-level `using` carries the same HIR form as a `using` in a
   function block, so every consumer reads one form: the three
   engines, the §137 rule 5b scan (a dispose hook is a call at the
   block exit), and the ownership and warning passes.

### 139.2 Acceptance

1. Red first: an accept entry (`js-comparable`) with the three measured
   shapes and a function control; it prints no dispose line for the
   top-level blocks at the contract pin; the round records the Red
   output. Goldens on all three engines and `node`.
2. Unit tests: dispose at each exit of a top-level block (natural end,
   `break` and `continue` in a top-level loop, a thrown exception
   caught by a top-level `try`); a dispose hook that reads a later
   global is S100 through the hook (§137 rule 5a); the module-level
   `using` stays S100.
3. No existing `.expected` golden moves.
