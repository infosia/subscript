<!-- §306 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 306. Cycles through a local lambda or a list, and the class hash

*(Added 2026-11-15.)* Origin: §290.4 item 1, §253.4 item 1, and §294.4
item 1. On 2026-11-15 the owner told Claude to continue with the open
items. The standing approval of 2026-11-06 covers a `tsc` divergence
with a known fix.

Problem, measured at `fc8b8b85` with `tsc` 5.9.2 and the debug CLI. The
checker accepts each form, and `tsc` rejects each one (invariant 5).

| Form | `tsc` |
|---|---|
| `const a = () => { const inner = () => b(); return inner(); }; const b = () => { return a(); };` | TS7023 at `a`, `inner`, and `b` |
| `const a = () => { return table[0](); }; const table = [a];` | TS7023 at `a` |
| `function f<undefined>(x: i32): void { return undefined; }` | TS2368 "Type parameter name cannot be 'undefined'" |

§290 rule 1 counts only a direct call of another undecided lambda in a
`return` operand. A call of a local lambda that reads the result, and a
call through a value whose type holds the undecided lambda, are not
counted.

The reload declaration hash does not hold `ClassDef::callback_lifetime`
or `ClassDef::boundary_header` (§294.4 item 1), and no test makes a new
field of `hir::ClassDef` need a decision. §304.3 item 1 found the same
class of gap for `hir::Global`: a reload accepted an edit that changed
the lowering of existing code.

Out of scope: §290.4 item 2 (`return id(b)();`).

### 306.1 Rules

1. §290 rule 1 changes. A lambda is in a cycle when the type of a
   `return` operand needs the result of an unannotated lambda whose
   result is not decided yet. These reads need the result:
   a. a call of the lambda (§290 rule 1);
   b. a call of a local lambda whose own result needs it;
   c. a call through a value whose every source is an undecided
      lambda of the cycle: an element of an array literal, a
      conditional, or an alias whose each source is such a lambda. A
      value with a source outside the cycle, such as another lambda
      whose result is decided, does not need the result. `tsc` accepts
      the call through such a value. *(Amended 2026-11-15 after the
      Phase Review, finding 1.)*
   A read of the lambda as a value whose type the operand does not
   need, such as the argument of `after(pong)` with an annotated
   `after`, does not need the result. `tsc` accepts it.
2. The diagnostic is the S100 of §290 rule 1, at each lambda of the
   cycle that the checker reaches. A local lambda of the cycle gets it
   too.
3. A type parameter whose name `tsc` reserves is rejected with S100 "a
   type parameter cannot have the name `<name>`". The names are
   `undefined`, `never`, `string`, `object`, `number`, `boolean`, `any`,
   `unknown`, `bigint`, and `symbol` (`tsc` 5.9.2 gives TS2368 for
   each, measured). The site is `TscRejects`. *(Amended 2026-11-15 after
   the Phase Review, finding 3.)*
4. The text of a class in the declaration hash holds `callback_lifetime`
   and `boundary_header`. A test destructures `hir::ClassDef` with no
   `..`, as the tests of `hir::Field` and `hir::Global` do, and names
   the decision for each field.
5. No other program changes from accepted to rejected, and no output
   changes.

### 306.2 Acceptance

1. Red first: one reject entry for each row of the table, each with its
   measured `tsc` header and one deciding diagnostic. At the pin, the
   checker accepts each one.
2. Unit tests for rule 1, each with an accepted control in the same
   shape: a local lambda that does not read the result, and an array of
   the lambda whose element the operand does not call. The §290 controls
   (`after(pong)`, a lambda that calls itself) stay accepted.
3. A reload test: an edit that changes only `callback_lifetime` or only
   `boundary_header` of a class is refused (Red at the pin), with a
   control edit that keeps the class.
4. The §154 total test passes with the new rejection site.
5. No `.expected` golden moves.

### 306.3 Phase Review

Evidence: `specs/tracking/cycle-and-class-hash.md`. At the contract pin,
`r516`, `r517`, and `r518` are accepted, and the two reload tests of
rule 4 fail. Over `corpus/accept` and `corpus/reject`, the diagnostics
of the pin and the change differ only on these three entries.

One fresh review found one MAJOR finding, realistic, and two MINOR
findings.

1. MAJOR: rule 1c as first written counted a call through each value
   that holds an undecided lambda. A dispatch table
   `const states = [idle, walk]; … states[s]()`, which `tsc` and the
   pin accept, became rejected. `tsc` gives TS7023 only when each
   source of the value is in the cycle. Rule 1c is amended, and eight
   measured forms are accepted unit tests.
2. MINOR, fixed: `tsc` gives TS2368 for nine more type parameter names.
   Rule 3 is amended to the list.
3. MINOR, fixed: the cycle check walked the edge graph on each read. A
   chain of 3000 lambdas took 9.1 s against 0.87 s at the pin. A cache
   of the reached set makes it 0.915 s against 0.906 s.

The first full gate failed in the debug suite only: eight tests of
§306 saw the checker before the change. The debug test binary of that
run failed again when run alone, and a fresh build of the same commit
passed. The stale debug artifacts were removed
(`cargo clean -p subscript-compiler`), and the second full gate passed.

### 306.4 Open

1. `c ? v : b`, with a function declaration `v` and a lambda `b` of the
   cycle, stays accepted. `tsc` gives TS7023. The pin gives the same
   result.
