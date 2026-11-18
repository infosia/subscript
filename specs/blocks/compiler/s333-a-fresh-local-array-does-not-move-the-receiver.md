<!-- §333 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 333. A fresh local array does not move the receiver

*(Added 2026-11-18.)* Origin: §221.4 item 3, rank 12 of
`specs/tracking/backlog.md`. A `tsc` divergence with a fix in sight
(standing approval of 2026-11-06).

Problem, measured at `add6648d`. §221 rule 3 rejects a call on an
unstable receiver (`all[i].m()`) if the method can reach an operation
that moves a dynamic array of the receiver's element type. Rule 5: the
type decides, not the array object. So a method that builds a new
array of its own type is rejected:

```ts
@ValueType
class V {
  a: i32 = 0;
  twice(): V[] { const out: V[] = []; out.push(this); out.push(this); return out; }
  plus(): i32 { let xs: V[] = [new V(1)]; xs.push(new V(2)); return this.a + xs.length; }
}
all[0].twice(); all[1].plus();
```

Each call gives S100 "`V.twice` can move a dynamic array of `V` by
`push`". `tsc` accepts both. The array that `push` moves is made in the
same call of the method, so it is not the storage of `all`, and `this`
stays valid.

### 333.1 Rules

1. **A fresh local.** A local binding of a function or a method body
   that is not a generator body is *fresh* if each value that the body
   gives it is an array literal. A `const` gets one value; a `let` gets
   the value of each assignment. A generator body runs across several
   `next` calls, so its array can exist before a later step moves it.
   *(Amended 2026-11-18 after the Phase Review: a generator stored its
   local array in a global, a method on an element of that array
   resumed the generator, and the push moved the array that held
   `this`; the development tier printed `40 1` for `41 41`.)*
2. **No move.** An operation of §221 rule 2 whose array operand is a
   read of a fresh local of the same body is not a move for §221 rule
   3. The array is made in the call, so no element of an array that
   existed at the call moves.
3. **Other operands stay.** An operation on a parameter, a field, a
   global, a call result, an array that a built-in returns
   (`slice`, `concat`), or a local that another value reaches, stays a
   move by §221 rule 5. A fresh local passed to another function is a
   parameter there, so an operation in that function stays a move.
4. **Nothing else changes.** The diagnostic, the reach of §221 rule 4,
   and the address form of §221 rule 8 do not change. The check adds
   no run-time cost.

### 333.2 Acceptance

1. Red first: an accept entry with the two forms of the problem, each
   on `all[i]`, that prints each result. It is `js-comparable: yes` if
   `node` prints the same bytes. At the pin, `check` gives S100.
2. A reject entry that stays S100 with the same text: a method that
   pushes `this` to an array parameter, a `let` local assigned from a
   global array, and a local from `slice`. Its header states what `tsc`
   does, measured.
3. A unit test for rules 1 to 3, each with a control in the same shape
   (core principle 9).
4. The development tier and the ship tier print the same output. The
   CLI `check` output of each other corpus entry is the same at the pin
   and after.

### 333.3 Sections this one amends

- §221 rule 5: an operation on a fresh local of the same body is not a
  move. §221.4 item 3 closes.

### 333.4 Phase Review

Form. `compiler/src/check/element_receiver_fresh.rs` gives the fresh
locals of each body that is not a generator body: each local whose
initializer and every assignment, in its scope, is an array literal.
The move reach of §221 skips an operation whose array operand reads one
of them. The diagnostic, the reach, and the address form do not change.

Measured, the pin `1be04b99` and after:

| Form | Pin | After, development tier and ship tier |
|---|---|---|
| `all[0].twice()`: `out.push(this)` twice on a local `[]` | S100 | `3,3` |
| `all[1].plus()`: `let xs = [new V(1)]; xs.push(…)` | S100 | `9` |
| a push to an array parameter, to a `let` from a global, to a `slice` result (`r554`) | S100 | S100, the same text |

`tsc` accepts each form. a483 is not `js-comparable` (C2, the
`@ValueType` decorator). The CLI `check` output of 1,094 other corpus
entries is the same at the pin and after.

One fresh review found one finding: a generator body runs across
several `next` calls, so a fresh local could exist before a later step
moved it. A method on an element of that array resumed the generator,
and the development tier printed `40 1` for `41 41`. The review rated
it contrived to borderline. Rule 1 now excludes a generator body, and a
unit test pins the shape. The review found no other defect.

### 333.5 Open

1. A fresh local passed to a helper that pushes to it stays a move
   (rule 3); the reach does not follow the array object.
