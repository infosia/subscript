<!-- §127 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 127. An import binding is read-only

*(Added 2026-09-29.)* Origin: the §126 Phase Review found it. The
contract has one sound answer (`tsc` rejects the program), so it
proceeds without an owner question.

Problem: an import of a module global binds the exporter's declaration
(§125, §126), and the checker's assignment rule
(`compiler/src/check/expr/assign.rs`) reads only the declaration's
`mutable` flag. So a write through an import is accepted. Measured at
`d735bd1` through `subscript run`: `lib.ts` exports `let count: i32 = 4`
and `bump()` (adds 1); `main.ts` imports both, writes `count = 10`,
calls `bump()`, and prints `11`.

`tsc` 5.9.2 rejects `count = 1`, `count += 1`, and `count++` through an
import with TS2632 "Cannot assign to 'count' because it is an import".
`node` throws `TypeError: Assignment to constant variable`. `tsc`
accepts a write to a field of an imported object (`box.v = 5`) and to a
static field of an imported class (`Box.s = 3`). An accepted program
must type-check under `tsc` (CLAUDE.md invariant 5).

### 127.1 Rules

1. An import binding is read-only in the importing module, renamed or
   not. An assignment, a compound assignment, an increment, or a
   decrement whose target is an import binding is S100, at the target,
   with a message that names the binding and says it is an import.
2. The exporting module writes its own module global as before, and
   the importing module reads the new value (a live binding, §125
   rule 1).
3. A write through an import binding to a field of an object, or to a
   static field of an imported class, does not change.
4. The rule reads a fact that the scope carries: an import binding is
   marked as one where the import registers it. The assignment rule
   does not compare file indices or names to find it.

### 127.2 Acceptance

1. Red first: a reject entry (a directory entry) with an assignment
   through an import, accepted at `d735bd1`; the round records the Red
   output. Its header states the measured `tsc` result (TS2632).
2. Unit tests: each assignment form of rule 1, renamed and not, is
   S100; firing controls in the same tests: the exporter's own write,
   a field write through the import, and a static field write through
   an imported class are accepted (rule 3).
3. No existing `.expected` golden moves.
