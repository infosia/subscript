<!-- §158 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 158. A local is assigned before it is read

*(Added 2026-10-05.)* Origin: `compiler.md` §154.3 item 3 (`collisions.md`
C24 row 3). The owner selected it on 2026-10-04.

Problem: a local with no initializer is rejected: `let x: i32; if (c) {
x = 1; } else { x = 2; }` gives S100 "local declarations require an
initializer" at `f60fa6f9`. C24 row 3 gives the reason "the binding
holds `undefined` until its first assignment". That reason holds only
for a read before the first assignment, and `tsc` rejects that read
(TS2454). §108 already proves the same fact for a field at the end of a
constructor.

`tsc` 5.9.2, measured on locals with no initializer (`cond()` is an
opaque `boolean`):

| Assignments before the read | `tsc` |
|---|---|
| both branches of `if`/`else` | accepts |
| one branch of `if`, no `else` | TS2454 |
| `else` branch ends in `throw`, `return`, or `unreachable()` (`never`) | accepts |
| every `case` and `default` of a `switch`, each ending in `break` | accepts |
| `try` and `catch` both assign | accepts |
| `while (true) { x = 1; break; }` | accepts |
| only in the body of `for` or `for…of` | TS2454 |
| `x = cond() ? 1 : 2; x += 1;` | accepts |
| `s = t = "a"` | accepts |

`finally` (§115 rule 2, C6) and `do…while` (C24 row 29) are outside the
subset, so no row uses them. Every row is in the subset when the local
has an initializer (measured at `34566a91`).

### 158.1 Rules

1. A local `let` with a type annotation and no initializer is accepted.
   A `const` still needs an initializer, as in TypeScript. A `let` with
   neither an annotation nor an initializer (`let x; x = 1;`) is
   rejected (S100, `Diverges`, C24 row 3): `tsc` gives it an evolving
   type from its later assignments, and this checker takes a local type
   from its annotation or its initializer only (§156 rule 1).
2. Every read of the local, and every compound assignment, `++`, and
   `--` of it, must come after an assignment on every path from the
   declaration. The paths follow the rules of the table: a branch that
   ends in `throw`, `return`, `break`, `continue`, or a call of a
   `never` function does not reach the read; a loop body that can run
   zero times does not assign for the code after the loop; `while
   (true)`, `for (;;)`, and `for (; true;)` exit only through `break`.
   A `switch` evaluates its discriminant, then each `case` test in
   order on the dispatch path, so a `case` test is a read. A `switch`
   with no `default` over every member of a source enum or of a
   string-literal union alias has no path that skips every case.
3. A read that rule 2 does not prove is rejected (S100) at the read,
   with the name and the remedy "assign `x` on every path before this
   read, or give it an initializer". `tsc` rejects such a read (TS2454),
   so the site is `TscRejects`.
4. The analysis is the §108 flow analysis, applied to a local: one
   analysis, not a second. It runs on every function body, also on a
   generic body that has no instance (§135 rule 1). If it needs a fact that its form does not
   carry for a local, the form grows.
5. A module variable with no initializer stays rejected (C24 row 3): a
   function can read it before any assignment runs, and `tsc` does not
   check that read.
6. A lambda cannot capture a `let` (C5), so no lambda reads the local.
   The C5 site keeps its class.
7. The storage of the local has a zero value until the first
   assignment on every tier. Rule 2 makes the zero value unobservable.

### 158.2 Residual policy

The rule 3 site is a measured claim: this checker approximates the
`tsc` flow analysis. A program that `tsc` accepts and this checker
rejects at the rule 3 site is a missing fact: it is fixed when it is
found and does not block a phase. A program that `tsc` rejects and this
checker accepts breaks invariant 5 and is MAJOR.

### 158.3 Acceptance

1. Red first: one accept entry, `js-comparable: yes`, with every
   accepted row of the table, for an `i32`, an `f64`, a `string`, a
   reference class, and a `T | null` local. Its golden is the `node`
   output. At the contract pin, the checker rejects it; record the
   diagnostics.
2. Reject entries for the one-branch `if`, the `for` body, and the
   `for…of` body (`tsc: rejects TS2454`), and for a module variable with
   no initializer (C24 row 3, `tsc: accepts`).
3. Unit tests for each rule 2 path, each with a control in the same
   shape.
4. The §154 total test passes; the site that rule 1 removes leaves the
   site table for locals.
5. No existing `.expected` golden moves.

### 158.4 Open

1. The HIR folds parentheses, so `while ((true))`, `if ((true))`, and
   `for (; (true);)` are treated as their unparenthesized forms. `tsc` does not narrow a parenthesized literal (TS2454 for a
   local, TS2564 for a field). The path it skips is dead, so no zero is
   visible; the field site then shows a block that says `tsc` accepts.
   Contrived.
2. Assignments inside conditions that `tsc` narrows (`if (!(c && (x =
   1) > 0)) return;`, `true ? (x = 1) : 0`) are not followed; the read
   is rejected and `tsc` accepts. Contrived.
3. A descriptor literal evaluates its members in declaration order, not
   source order (`{ b: x, a: (x = 1) }`), so the flow analysis follows
   the declaration order. This order rule is older than §158. Contrived.
4. The §108 field sites moved toward `tsc` with the shared analysis:
   11 of 26 constructor probes of the Phase Review now carry a block
   that says `tsc` accepts (`while (true)` with `break`, `unreachable()`,
   an assignment inside an expression, `switch`, `try`, `using`, loop
   conditions, a `for` initializer); `tsc` accepts each.
