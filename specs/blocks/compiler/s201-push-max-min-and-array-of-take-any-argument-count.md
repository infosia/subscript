<!-- §201 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 201. `push`, `Math.max`, `Math.min`, and `Array.of` take any argument count

*(Added 2026-10-28.)* Origin: on 2026-10-27 the owner asked for the
divergences from TypeScript that are cheap to remove. On 2026-10-28
the owner selected this one first of the second group. The measurement
is in `specs/tracking/multi-argument-calls.md`.

Problem, measured at the pin `43526876` (`tsc` 5.9.2 accepts each row,
and `node` runs it):

| Form | `subscript check` |
|---|---|
| `xs.push(2, 3, 4)` | S100 "`push` expects 1 argument(s) (1 required), got 3" |
| `Math.max(1, 2, 3)` | S014 "`max` is rejected: Variadic parameters are outside the language. (Q19)" |
| `Math.min(4)` | S014 "`Math.min` takes exactly 2 f64 argument(s), got 1" |
| `Math.max()` | S014 "`Math.max` takes exactly 2 f64 argument(s), got 0" |
| `Array.of<i32>(1, 2)` | S014 "Variable arity needs the variadic-parameter prerequisite" |

The recorded reason is that the language has no variadic parameters
(Q19, Q27, §105.3, `stdlib.md` §9.11 rule 3). That is true, but none
of these calls needs one. Each call has a fixed argument list in the
source, so the checker can write the call as a sequence of the
one-argument or two-argument form.

### 201.1 Rules

1. **`push`.** `r.push(a1, …, ak)` on an `Array` receiver, with
   `k >= 0`, evaluates `r` once, then `a1` to `ak` in order. After all
   arguments are evaluated, it appends each value in order, as
   `r.push(ai)` does. The result is the length after the last append.
   With `k = 0`, nothing is appended, and the result is the length.
   An argument expression that reads `r` sees `r` before the first
   append, as in JavaScript: `xs = [1]; xs.push(xs.length, xs.length)`
   gives `[1, 1, 1]`.
2. **`Math.max` and `Math.min`.** `Math.max(a1, …, ak)` with `k >= 2`
   is `Math.max(Math.max(a1, a2), a3, …)`, a fold from the left. The
   same rule applies to `Math.min`. Each argument is an `f64`, as in
   the two-argument form (`stdlib.md` §1). `Math.max(a)` and
   `Math.min(a)` are `a`. `Math.max()` is `-Infinity`, and
   `Math.min()` is `Infinity`. The fold gives the ECMA result: `NaN`
   in any argument gives `NaN`, and `+0` is greater than `-0` in any
   order.
3. **`Array.of`.** `Array.of(a1, …, ak)` with `k >= 2` is the array
   literal `[a1, …, ak]`. Its element type comes from the arguments and
   the type argument, as the literal gets it (`stdlib.md` §9.11
   rule 2).
4. The receiver and the arguments evaluate once, by §82.1 rule 3. A
   synthetic local holds the receiver reference or an argument value.
   An argument value is copied into it, as the append copies it. If
   an argument is not free of order, the receiver is read into a
   synthetic local before the first argument, whatever its kind: a
   local, a global, or a static field. So an argument that assigns
   the receiver does not change the array that the appends reach, as
   in JavaScript. *(Amended 2026-10-28 after the Phase Review, M1.)*
   An argument is free of order if it is a literal, an enum member,
   a function reference, or a lambda (C5 lets a lambda capture
   `const` locals only). In an initializer that has no statement
   list, a `push` with two or more arguments fails if the form needs
   a synthetic local. The text names the operand that needs it: the
   receiver or an argument.
4a. **The sequence form.** The HIR carries an expression
   `Seq { effects, value }`: it runs each effect in order, then
   evaluates `value`, and its type is the type of `value`. Rule 1
   lowers to one `Seq` in statement position and in value position: the effects store `a1` to `ak`
   in synthetic locals and run the first `k - 1` appends, and the
   value is the last append. The HIR→LIR lowering writes a `Seq` as its
   effects followed by its value, so it adds no run-time operation.
   Each HIR walker treats a `Seq` as its parts in order. No source form
   other than this section lowers to a `Seq`.
5. A spread argument (`xs.push(...ys)`, `Math.max(...xs)`) stays
   rejected. Each spread argument gives the call-spread text, which
   names the variadic parameter that the call needs. *(Amended
   2026-10-28 after the Phase Review, m1: the `Math.max` and
   `Math.min` spread texts named a two-argument limit.)*
6. These stay rejected, each with a text that names its own reason:
   - `Math.hypot` with three or more arguments. A fold of
     `Math.hypot` rounds differently from the N-argument form: 80,198
     of 200,000 random triples differ under `node`. The divergence
     entry names no replacement: `Math.sqrt` of the sum of squares
     also rounds differently.
   - `unshift` with two or more arguments, and the insert form of
     `splice`. The texts do not change. A sequence of one-element
     `unshift` calls moves the elements once for each argument
     (principle 16), so this form needs a runtime insert of `k`
     elements.
   - `concat` with two or more arrays. The text does not change.
7. The forms add no call and no branch. Each synthetic local is a
   stack slot with one store, and a value-class argument is copied
   once into it.

### 201.2 Acceptance

1. Red first, at the pin `43526876`: the accept entry
   `a384-multi-argument-calls` is rejected. The reject entry
   `r16-math-variadic-max`, changed to `Math.hypot(1, 2, 3)` with the
   rule 6 text, fails its expected error at the pin. The tracking note
   records each result.
2. `a384` passes `tsc`, and its output under `node` equals its golden.
   It pins rule 1 evaluation order with an argument that reads the
   receiver, `push()` with no argument, a `push` on a reference element
   type, `Math.max` and `Math.min` with zero, one, two, and four
   arguments, `NaN` and `-0` in a fold, and `Array.of` with three
   arguments.
3. `r211-array-of-variadic` is removed. Its form is accepted, and
   `a384` carries it.
4. The divergence entries and the rejection sites that only the
   removed limits reached are removed. The rejection-site total check
   passes.
5. A unit test pins rule 1: the HIR evaluates every argument before
   the first append, in statement position and in value position (a
   `Seq`). A control with one argument gives the existing single
   `push`. A unit test pins the rule 4a lowering of a `Seq`.
6. `stdlib.md` §1, §9 (`unshift`), and §9.11 rule 3 cite this section.
   `collisions.md` Q19 and the spread paragraph that cites Q19 state
   the new limit.
7. No golden moves.

### 201.3 Phase Review

*(Added 2026-10-28.)* A fresh review of `43526876..bc606770` gave one
MAJOR and five MINOR findings.

1. MAJOR, realistic, fixed: a `push` with two or more arguments read
   a global, a static-field, or a local receiver again after its
   arguments. `g.push(1, reset())`, where `reset` assigns `g = []`,
   appended to the new array on both tiers; `node` appends to the old
   one. The two tiers agreed, so only a golden shows it (principle
   12). Rule 4 now binds the receiver.
2. MINOR, fixed: `Math.max(...xs)` named a two-argument limit. Rule 5
   now gives the call-spread text.
3. MINOR, fixed: the initializer text named the receiver when only an
   argument needed a synthetic local.
4. MINOR, fixed: rule 4 cited §199.1 rule 2, which does not cover an
   argument value. Rule 4a named value position only. Rule 7 did not
   state the store of a synthetic local.
5. MINOR, fixed: the `Math.hypot` divergence entry gave
   `Math.sqrt(1 * 1 + 2 * 2 + 3 * 3)` as the replacement. It prints
   `3.7416573867739413`, and `node` `Math.hypot(1, 2, 3)` prints
   `3.741657386773941`.

Open: `compiler/src/hir.rs` has 2,000 lines.
