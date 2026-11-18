<!-- §336 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 336. A type argument and a plain template give their literal context

*(Added 2026-11-19.)* Origin: §103.5 item 7 and
`specs/tracking/text-module-measure.md` §9.7 item 2, rank 14 of
`specs/tracking/backlog.md`. A `tsc` divergence with a fix in sight
(standing approval of 2026-11-06).

Problem, measured at `892bcbc8`:

| Form | subscript | `tsc` |
|---|---|---|
| `new Set<i32>([])`, `new Set<string>([])` | S100 "cannot infer the type of an empty array literal without context" | accepts |
| `t.normalize(`NFC`)` (a template with no substitution) | S014 "The form must be omitted or the literal \"NFC\"" | accepts |

`const s: Set<i32> = new Set([])` and `Array.from<i32>([])` are
accepted, so an annotation or another type argument already gives the
empty literal its element type. `t.normalize("NFC")` is accepted.

### 336.1 Rules

1. **A `Set` type argument is the context.** In `new Set<T>(e)`, the
   type argument gives the argument `e` the context `T[]`, so an empty
   array literal there has the element type `T`.
2. **A plain template is a literal.** A template with no substitution
   is a string literal at the `normalize` form argument, as §299 rule
   2a treats it for a literal operand. `` t.normalize(`NFC`) `` is the
   form `t.normalize("NFC")`. `` `NFD` `` keeps the S014 of a form that
   is not available.
3. **Nothing else changes.** `new Map<K, V>([])` keeps its S014 (no
   pair element). The output of an accepted program does not change.

### 336.2 Acceptance

1. Red first: an accept entry with `new Set<i32>([])`,
   `new Set<string>([])` with an `add`, and `` t.normalize(`NFC`) ``.
   It prints each result. It is `js-comparable: yes` if `node` prints
   the same bytes. At the pin, `check` rejects it.
2. A unit test for rules 1 and 2, with the controls `` `NFD` `` (stays
   S014) and `new Set([])` with no context (stays S100).
3. The development tier and the ship tier print the same output. The
   CLI `check` output of each other corpus entry is the same at the pin
   and after.

### 336.3 Sections this one amends

- §103.5 item 7 closes; `text-module-measure.md` §9.7 item 2 closes.

### 336.4 Phase Review

Form. `new Set<T>(e)` checks `e` with the context `T[]` through the
path that an annotation already uses (`compiler/src/check/expr/construction.rs`).
The `normalize` form check reads a template with no substitution as its
cooked string (`compiler/src/check/expr/method.rs`). No lowering
changes.

Measured, the pin `57dfcf1a` and after, development tier, ship tier,
and `node` the same (a486):

| Form | Pin | After |
|---|---|---|
| `new Set<i32>([])` | S100 | `0` |
| `new Set<string>([])` with `add("a")` | S100 | `1:true` |
| `` t.normalize(`NFC`) `` | S014 | `é` |

`new Set([])` with no context stays S100, `new Map<K, V>([])` stays
S014, and `` `NFD` `` and a template with a substitution stay S014. The
CLI `check` output of 1,100 other corpus entries is the same at the pin
and after.

One fresh review found no CRITICAL or MAJOR finding. Its MINOR notes
need no change: a template with an invalid escape has no cooked value
and cannot equal `"NFC"`; `tsc` accepts `normalize("nfc")` through the
prelude signature, as it did before this section.
