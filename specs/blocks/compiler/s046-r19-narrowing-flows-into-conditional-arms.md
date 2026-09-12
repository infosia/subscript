<!-- §46 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 46. R19 — narrowing flows into conditional arms

Owner decision 2026-08-03 (downstream request R19; blocking the
generator change §45 was meant to enable). §45 gave the conditional
its contextual type but not the flow facts its condition
establishes, so

```ts
function viaIf(v: C | null): u32 {
  if (v !== null) { return use(v); }        // accepted
  return 0;
}
function viaCond(v: C | null): u32 {
  return v !== null ? use(v) : 0;           // rejected, S005
}
```

differ on a plain reference class — reproduced at the pin,
`tsc`-clean. The `if` form narrows; the conditional arm does not.

Recorded, because it is the second instance of the same review
gap: §45's corpus exercised the conditional's *shape* but every
case constructed its value inline, so no case reached a nullable
**local** — exactly as the OBS-3 corpus exercised descriptor shapes
but never a *returned* descriptor. A corpus entry pins a
construction, and a construction is not a flow.

### 46.1 Rule

**A conditional expression's arms are checked under the narrowing
its condition establishes**: the then arm under the facts the
condition proves, the else arm under their negation — the same
facts, from the same analysis, that the `if` statement already
applies. No new narrowing analysis is introduced; the existing
`narrow_paths` result is applied at one more site. The narrowing
does not outlive the conditional: a path narrowed inside an arm is
not narrowed after the expression.

### 46.2 Corpus

`a125-conditional-arm-narrowing` (accept): the `viaIf`/`viaCond`
parity above for a reference class, an opaque handle, and — through
the interop fixture — the generator's real shape,
`x !== null ? toX(x) : null` supplying a nullable boundary
aggregate argument from a nullable local, in both condition orders,
with both paths observed. `r120-narrowing-escapes-conditional`
(reject): a path narrowed inside an arm used after the conditional
still rejects (`tsc` status recorded).

### 46.3 Exit criteria (pre-registered)

1. `a125` byte-identical under both tiers.
2. `r120` pins (code, line) with its `tsc` status recorded; no
   existing reject entry changes meaning.
3. Checker unit tests: both condition orders; nested conditionals;
   a conditional inside an `if` arm and the reverse; narrowing
   invalidated by assignment inside an arm.
4. `tsc` gate green with `a125`; no existing golden moves; full
   gate green; zero-warning sweep green.
