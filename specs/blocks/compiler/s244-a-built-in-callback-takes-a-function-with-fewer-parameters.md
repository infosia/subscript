<!-- §244 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 244. A built-in callback takes a function with fewer parameters

*(Added 2026-11-07.)* Origin: §243.4 item 2 and §242.4 item 1. Under
the owner's standing approval of 2026-11-06 for `tsc` divergences with
a known fix.

Problem, measured at `6f9a38c3` with `tsc` 5.9.2, the libraries of the
project, and the debug CLI. `none(): void`, `one(x: i32): void`, and
`two(x: i32, i: i32): void` are functions, `m` is a `Map<string, i32>`,
`s` is a `Set<i32>`, and `b` is a `boolean`.

| Form | subscript | `tsc` |
|---|---|---|
| `[1, 2].forEach(() => { count++; });` | S014 "`forEach` callbacks take 1 parameter(s), or 2 with a trailing `i32` index; got 0 (Q27)" | accepts |
| `[1].forEach(none);` | S014, the same | accepts |
| `[1, 2].map((): i32 => 0)` | S014, the `map` kind | accepts |
| `[1, 2, 3].filter((): boolean => true)` | S014, the `filter` kind | accepts |
| `m.forEach((v: i32) => print(`v${v}`));` | S014 "`Map.forEach` callbacks take exactly 2 parameter(s); extra lib callback parameters are not accepted (Q24)" | accepts |
| `m.forEach(() => print("m"));` | S014, the same | accepts |
| `s.forEach(() => print("s"));` | S014 "`Set.forEach` callbacks take exactly 1 parameter(s); …" | accepts |
| `[1, 2].reduce((acc: i32): i32 => acc + 1, 0)` | S014 "`reduce` callbacks take 2 parameter(s), or 3 with a trailing `i32` index; got 1 (Q27)" | accepts |
| `[6].forEach(b ? two : (x: i32): void => {});` | S100 "conditional branches have no common type: `(i32, i32) => void` and `(i32) => void`" | accepts |
| `[7].forEach(b ? one : none);` | S100 "conditional branches have no common type: `(i32) => void` and `() => void`" | accepts |

Q22, Q24, and Q27 fix the arities of a built-in callback. Their reasons
are about parameters that the language does not pass: the third
`(v, i, arr)` parameter of an array callback (C5), the third parameter
of `Map.forEach`, and the index of a `sort` comparator. No reason
covers a callback with fewer parameters than the method passes. A
callback that does not read a parameter gets the same value for each
parameter that it reads.

§243 rules 1 and 2 give the form: a lambda literal takes the trailing
parameters of its place, and a function declaration or a static method
takes a lambda with no environment. A built-in callback has an inline
lowering (§235 rule 5c), so the added parameters have no cost.

### 244.1 Rules

1. A built-in callback place takes a lambda literal, or a reference of
   §243 rule 2, with fewer parameters than the smallest arity of the
   place. The value takes the place of that smallest arity, as in §243
   rules 1 and 2. The callback places are those of `compiler.md` §242
   rule 1 that state an arity: the array and `FixedArray` callbacks,
   `Map.forEach`, `Set.forEach`, and `Map.groupBy`.
2. In a conditional of §242 rule 1a, the place of each leaf has the
   largest arity of the leaves that the place allows. A leaf with fewer
   parameters takes that place as in rule 1. So `b ? two : one` takes
   the place with the index, and `b ? one : none` takes the place
   without it.
3. Each callback keeps its inline lowering. The call passes the same
   arguments that it passes today.
4. These stay rejected: a callback with more parameters than the place
   allows (the `(v, i, arr)` form of C5, the third parameter of
   `Map.forEach`, an index on `sort`), a function value with an
   environment, a generic function, an async function or lambda, and a
   reference to a function with a default parameter. A lambda literal
   with a default parameter is a lambda literal of rule 1, as in §243.
4a. The diagnostic of a value with fewer parameters that rule 1 does not
   take names that cause: the new environment has no owner, or the
   default parameter has no adapter. It is not the diagnostic of a
   callback with too many parameters. A value with fewer parameters
   whose parameter or result types do not fit the place gets the type
   mismatch diagnostic of the place, as at the full arity. A
   conditional gets the diagnostic of its branches; rule 4a does not
   classify its leaves (§244.4).
5. The development tier, the ship tier, and the interpreter give the
   same output. The accept entry carries `js-comparable` and equals
   `node`.

### 244.2 Acceptance

1. Red first, at the pin: the accept entry
   `a430-built-in-callback-takes-a-function-with-fewer-parameters` is
   rejected. The tracking note records the output.
2. `a430` passes `tsc`. Its output under `node` equals its golden. It
   has each form of the table, and a `FixedArray` callback with no
   parameters. Each callback prints or counts, so the number of calls
   is visible.
3. A unit test pins rules 1, 2, and 4. It has each form of rule 4
   (rejected) as a control.
4. If a corpus entry or golden changes its type or output, report it
   and stop. No golden moves.

### 244.3 Phase Review

*(Added 2026-11-08.)* Two fresh reviews of the implementation. The
contract gained rule 4a. No CRITICAL finding. The first review measured
about 120 callback forms; dev, ship, and `node` gave the same bytes.

1. MAJOR, realistic, fixed: a function value with fewer parameters that
   rule 1 does not take, such as a local `f` in `xs.forEach(f)`, got
   the diagnostic and the reason of a callback with too many
   parameters. Rule 4a: a site of its own, `CallbackFewerParametersUnadapted`.
2. MAJOR, realistic, fixed: the same class. A value with fewer
   parameters whose types do not fit, such as
   `m.forEach((k: string) => print(k))` on a `Map<string, i32>`, got the
   arity diagnostic. It now gets the type mismatch of the place, so no
   value with fewer parameters reaches the arity diagnostic.
3. MINOR, fixed: a conditional with a leaf that has a default parameter
   got the environment text of rule 4a. A conditional now gets the
   diagnostic of its branches.
4. MINOR, fixed: a stale citation, stale witness locations, a stale
   test name, dead code (`ParameterCount`), and an async control that
   was missing.
5. MINOR, recorded: §244.4.

### 244.4 Open

1. A conditional of leaves with fewer parameters in a `map` or `reduce`
   callback stays rejected:
   `xs.map(t ? ((v: i32): i32 => v) : ((v: i32, i: i32): i32 => i))`.
   `tsc` accepts it. §242 rule 1a places leaves only where the place
   is known before the argument is checked. This is older than §244.
2. Rule 4a does not classify the leaves of a conditional, so
   `[7].forEach(b ? one : f)` with a local `f` gets "conditional
   branches have no common type", not the cause. `tsc` accepts it.
   *(Closed by §246 for a const local `f`, accepted at `b9bef6f9`. A
   leaf that §246 rule 4 rejects is §246.4 item 1.)*
3. The reason of `CallbackParameterShape`, "a parameter for the
   container itself is not passed", is true for the third parameter of
   `Map.forEach` and of `Set.forEach`. It is not true for an extra
   index on `sort`, or for `map` with four parameters. This is older
   than §244.
4. A conditional diagnostic names the padded type of a leaf, such as
   `(i32, i32) => void` for `one`.
