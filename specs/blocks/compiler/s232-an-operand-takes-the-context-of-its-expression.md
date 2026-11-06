<!-- §232 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 232. An operand takes the context of its expression

*(Added 2026-11-06.)* Origin: §212.4 items 1 and 7, and §215.4 items
1 and 2. Under the owner's standing approval of 2026-11-06 for `tsc`
divergences with a known fix.

Problem, measured at `0bd4204a` with `tsc` 5.9.2, the libraries of the
project, and the debug CLI. `mk<T>(): T[]`, `zero<T>(): T`,
`empty<T>(): Buf<T>`, `K.make<T>(): Buf<T>`, and
`af<T>(): Promise<Buf<T>>`. Each form is the result of a function with
the stated return type.

| Form | Return type | subscript | `tsc` |
|---|---|---|---|
| `c \|\| zero()`, `c: boolean` | `boolean` | S100 no candidate | accepts |
| `zero() \|\| c` | `boolean` | S100 no candidate | accepts |
| `c && zero()` | `boolean` | S100 no candidate | accepts |
| `k?.make() ?? empty<P>()`, `k: K \| null` | `Buf<P>` | S100 no candidate | accepts |
| `k?.make() ?? empty()` | `Buf<P>` | S100 no candidate | accepts |
| `zero() ?? empty<P>()` | `Buf<P>` | S100 no candidate | accepts |
| `await (c ? af() : af())` | `Promise<Buf<P>>` | S100 no candidate | accepts |
| `zero() && c` | `boolean` | S100 no candidate | TS2322, `unknown` |
| `!zero()` | `boolean` | S100 no candidate | accepts, `T` is `unknown` |

§212.1 rule 1a gives a context to a conditional branch and to the right
operand of `??`. It gives none to an operand of `||` or `&&`, to the
left operand of `??`, to the operand of `as`, to an optional call, or to
the operand of `await` that is not a call. `||` and `&&` take `boolean`
operands only (the decided divergence `LogicalNonBooleanOperand`), and
an optional call is a value only as the left operand of `??`
(`OptionalCallValueWithoutFallback`). `tsc` gives each of these a
context, except the left operand of `&&` and the operand of `!`. There `tsc` infers `unknown`,
and the language has no such type (§212.1 rule 5).
*(Amended 2026-11-06: the coding agent measured that the contract
listed `s || mk()`, `mk() || s`, and a bare `k?.make()` as accepted, and
the two decided rules above reject them.)*

### 232.1 Rules

1. In a position of §212.1 rule 1, these operands keep the context of
   the expression: each operand of `||`, the right operand of `&&`, and
   the left operand of `??`. An optional call `a?.m()` or `a?.f()` in
   such a position takes the context with `| undefined` and `| null`
   dropped. A literal other than `null`, a template, and an array
   literal take no context from this rule, so their types do not
   change. *(Amended 2026-11-06 after the Phase Review, finding 1.)*
2. The operand of `e as T` takes no context. With `T` as its context, a
   call such as `empty() as Buf<P>` becomes `Buf<P> as Buf<P>`, an
   identity assertion, which C24 rejects (`IdentityAssertion`). So no
   `as` form becomes accepted, and §212.4 item 3 stays open.
   *(Amended 2026-11-06: the coding agent measured the identity
   assertion.)*
3. The operand of `await` takes `Promise<C>` as its context if the
   `await` has the context `C`. This holds through parentheses, a
   conditional expression, and an optional call.
4. The left operand of `&&` and the operand of `!` take no context. They
   keep the diagnostic of §212.1 rule 5, as `tsc` infers `unknown` there.
5. §212.1 rule 6 holds: no accepted program changes its meaning. The
   context decides only a type parameter that gave a diagnostic, or a
   literal that failed the context.
6. §212.4 item 6a stays for a left operand of `??` that is not an
   optional chain: in a generic body, the right operand takes no context
   if that left operand involves a type parameter. After an optional
   chain, the right operand keeps its context, as at the pin.
   *(Amended 2026-11-06 after the Phase Review, finding 4.)*
7. Inference is a check-time step. The program has no run-time cost.

### 232.2 Acceptance

1. Red first, at the pin: the accept entry
   `a419-operand-takes-context` is rejected. The tracking note records
   the output.
2. `a419` passes `tsc`. Its output under `node` equals its golden. It
   has each accepted row of the table, with a run of each operand that
   decides the result.
3. `r490-and-left-operand-has-no-context` has `if (pick("true") && c)`,
   with `pick<T>(text: string): T`. `tsc` accepts it, because a
   condition takes `unknown`, and subscript rejects it with the
   divergence block of §149. Its header states the `tsc` result,
   measured. In a `boolean` result position `tsc` rejects the same
   operand with TS2322. *(Amended 2026-11-06: the coding agent measured
   that a reject entry with `tsc: rejects` cannot carry the divergence
   block, §79.1 rule 1.)*
4. A unit test pins rules 1 to 4. Controls: the `!` row stays rejected,
   a form with an argument candidate keeps that candidate, and a
   literal operand keeps its present type where it fits.
5. If a corpus entry or golden changes its type or output, report it and
   stop.
6. The rejection-site total check passes. §212.4 items 1 and 7, and
   §215.4 items 1 and 2, cite this section. `a419` has an async entry,
   so the LIR text snapshot grows by its functions. No other golden
   moves.

### 232.3 Phase Review

*(Added 2026-11-06.)* A fresh review of `bcb2eda2` and the implementation
gave four MINOR findings and no CRITICAL or MAJOR finding. Numeric
contexts, `??` chains, `||` and `&&` operands, `await` of a conditional,
generic bodies, and the clearing of the optional-chain context give no
change of meaning, and the dev and ship tiers agree.

1. MINOR, contrived, fixed in the text: rule 1 did not state that a
   literal takes no context. §232.4 item 1.
2. MINOR, fixed: the literal control of acceptance 4 gave the same type
   with and without a context.
3. MINOR, fixed: no unit test failed if a mark of rule 1 was deleted.
4. MINOR, fixed: the `purpose` line of `a419` had 29 words, and rule 6
   did not state its scope.

### 232.4 Open

1. `(c ? "a" : null) ?? "b"` with a `Mode` result gives S100 got
   `string`. `tsc` accepts it. A literal takes no context from rule 1.
2. §212.4 item 3: `empty() as Buf<P>` (rule 2).
