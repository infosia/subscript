<!-- §319 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 319. A nullish right operand in a generic body takes the context

*(Added 2026-11-16.)* Origin: §212.4 item 6a. The owner told the work
to continue down the open-item triage on 2026-11-16.

Problem, measured at `11ab7e2e`. With `class Buf<T> { n: i32 = 0; }`
and `function empty<T>(): Buf<T>`:

```ts
function f<U>(a: Buf<U> | null): Buf<U> { return a ?? empty(); }
function g<U>(a: U[] | null): U[] { return a ?? []; }
```

`tsc` accepts both. The checker gives S100 "cannot infer type parameter
`T` of `empty`: no candidate" and "cannot infer the type of an empty
array literal without context". §212 rule 1a gives the right operand of
`??` the context of the `??` expression, except in a generic body where
the left operand involves a type parameter: there a context changes the
type of `a ?? 1` and of `a ?? id(1)`, which §212 rule 6 forbids.

### 319.1 Rules

1. In a generic body, the right operand of `??` takes the context of
   the `??` expression (§212 rule 1) if, with no context, it gives one
   of these diagnostics:
   1. "cannot infer type parameter … no candidate" (§149.1 rule 5) for
      a generic call with no type arguments;
   2. "cannot infer the type of an empty array literal without
      context".
1a. An argument of the right operand, at any depth, that is a form of
   rule 1 takes the context that its parameter gives, as §212 rule 2
   gives it (`a ?? wrap(empty())`, `a ?? pair(u, empty())`). An
   argument keeps the context that it had at the pin first: rule 1
   adds a context only where the pin gave none. *(Added 2026-11-16
   after the Phase Review: the round checked each nested call argument
   with the missing-candidate context only, so
   `a ?? sum(u, id(1))` with `sum<T>(x: T, y: f32): T` lost the `f32`
   context of `id(1)` and gave S007; the pin and `tsc` accept it.)*
2. Each other right operand keeps the exception of §212 rule 1a. So
   `a ?? 1` and `a ?? id(1)` keep their type and their diagnostic.
3. §212 rule 6 holds: a context decides only an operand that gave a
   diagnostic at the pin.
4. The development tier, the ship tier, and the interpreter give the
   same output. The accept entry carries `js-comparable` and equals
   `node`.

### 319.2 Acceptance

1. Red first: an accept entry with `f` and `g` called with `null` and
   with a value, with output. It is rejected at the pin.
2. A test that `a ?? 1` and `a ?? id(1)` in a generic body give the
   same result as at the pin.
3. The CLI `check` output of each corpus entry is the same at the pin
   and after the change, except the new entry.

### 319.3 Phase Review

*(Filled in after the review.)*

### 319.4 Open

None.
