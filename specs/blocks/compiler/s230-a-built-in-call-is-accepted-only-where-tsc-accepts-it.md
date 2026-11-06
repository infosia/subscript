<!-- §230 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 230. A built-in call is accepted only where `tsc` accepts it

*(Added 2026-11-06.)* Origin: §229.4 items 1 and 2. The owner selected
this section on 2026-11-06.

Problem, measured at `2500762a` with `tsc` 5.9.2, the libraries of the
project (`ES2022`, `ESNext.Disposable`), and the debug CLI. Each program
below is accepted, and `tsc` rejects it, against invariant 5.
`type Mode = "a" | "b"`, and `xs: i32[]`. Each form is the result of a
function with the stated return type.

| Form | Return type | subscript | `tsc` |
|---|---|---|---|
| `xs.reduce((acc: Mode, x: i32) => "b", "a")` | `Mode` | accepts | TS2769 |
| `xs.reduce((acc: Mode, x: i32) => "b", m)`, `const m: Mode = "a"` | `Mode` | accepts | TS2769 |
| `xs.reduceRight((acc: Mode, x: i32) => "b", "a")` | `Mode` | accepts | TS2769 |
| `xs.map<i32, i32>((x: i32) => x)` | `i32[]` | accepts | TS2558 |
| `xs.filter<i32>((x: i32) => x > 0)` | `i32[]` | accepts | TS2345 |

`tsc` and subscript accept these neighbours:

| Form | Return type |
|---|---|
| `xs.reduce<Mode>((acc: Mode, x: i32) => "b", "a")` | `Mode` |
| `xs.reduce((acc: Mode, x: i32): Mode => "b", "a")` | `Mode` |
| `xs.reduce((acc: Mode, x: i32) => x > 0 ? "b" : acc, "a")` | `Mode` |
| `xs.reduce((acc: string, x: i32) => "b", "a")` | `string` |
| `xs.reduce((acc: i32, x: i32) => acc + x, 0)` | `i32` |

Cause of the first three rows, from the `tsc` message: with an initial
value and no type argument, the first overload that fits is
`reduce(callbackfn, initialValue: T): T`. `tsc` types the expression body
of the callback once, with the element type `T` as the context of its
result. A literal result keeps its literal type only if `T` contains a
literal type of that kind. Otherwise the result widens: `"b"` becomes
`string`. The generic overload then reuses that result, and `string` is
not a `Mode`.

The cause of the last two rows: the checker reads no type argument of
`filter`, and reads a list of `map` only if it has one entry. A list
that the checker does not read has no effect and no diagnostic.

### 230.1 Rules

1. For `reduce` and `reduceRight` with an initial value and with no type
   argument, a callback with an expression body and no result annotation
   has the result type that `tsc` gives it: the checker types the body
   with the element type as the context of its result. If that type is
   not assignable to the accumulator type, the call is rejected. The
   diagnostic names the `tsc` result and gives two fixes: a type
   argument, or a result annotation.
1a. Rule 1 does not apply if the accumulator parameter has an
   annotation and another parameter has none. `tsc` then types the body
   after the inference, and it accepts `(acc: Mode, x) => "b"`. If no
   parameter has an annotation, or the accumulator has none, rule 1
   applies, as `tsc` rejects these forms. *(Added 2026-11-06 after the
   Phase Review, finding 1.)*
2. Rule 1 changes no other call. A call with a type argument, an
   annotated result, a block body, or no initial value keeps its rule.
3. A type argument list on a built-in method or function is accepted only
   if the checker reads each entry of it. Each other list is rejected,
   and the diagnostic names the number of type arguments that the
   method takes. Each place where the checker checks a call runs
   the check through one helper, so a call in a `for…of` subject, an
   expression statement, or an optional chain is checked too. *(Amended 2026-11-06 after the Phase
   Review, finding 2.)*
4. The reference lists the built-in methods that read a type argument,
   with the count of each.
5. No run-time cost. Both rules change the checker only.

### 230.2 Acceptance

1. Red first, at the pin: `r486-reduce-literal-result-widens` has the
   first row of the table, and `r487-unread-type-argument` has the
   `filter` row. Each is accepted at the pin. The tracking note records
   the output.
2. Each reject entry states its `tsc` result, measured. The
   `reduceRight` row, the `const m` row, and the `map<i32, i32>` row are
   unit tests.
3. Each neighbour of the second table stays accepted. A unit test pins
   each, as a control of rule 1.
4. A unit test pins rule 3 for each built-in that reads a type argument,
   with the right count accepted and a wrong count rejected.
5. If a corpus entry, an example, or a golden uses a form that rule 1 or
   rule 3 rejects, report it and stop.
6. The rejection-site total check passes. §229.4 items 1 and 2 cite this
   section. No golden moves.

### 230.3 Phase Review

*(Added 2026-11-06.)* A fresh review of `6b31e0d2` and the implementation
gave seven MINOR findings and no CRITICAL or MAJOR finding.

1. MINOR, realistic, fixed: rule 1 rejected four forms that `tsc`
   accepts, such as `(acc: Mode, x) => "b"`. Rule 1a now states them.
2. MINOR, realistic, fixed: rule 3 did not check a call in a `for…of`
   subject or a statement `unreachable<i32>()`. Each call entry now runs
   one helper.
3. MINOR, contrived, open: §230.4 item 1.
4. MINOR, contrived, open: §230.4 item 2.
5. MINOR, contrived, open: §230.4 item 3.
6. MINOR, fixed: the count test had no case for `Context.bytesInto` or
   `Context.fromBytes`, and no control for a user generic call.
7. MINOR, fixed: the reference text of rule 1 was not in STE, and it did
   not state rule 1a. One part stays: when rule 1 rejects a call, the
   S014 of a callback with the wrong parameter count does not show. The
   program is rejected in both cases.

### 230.4 Open

1. *(Closed by §231.)* `xs.reduce((acc: Mode, x: i32) => { return "b"; }, "a")` was accepted,
   and `tsc` gives TS2322 and TS2769. Rule 2 keeps a block body.
2. `xs.flatMap<i32, undefined>(…)` is rejected with "takes 1 type
   argument, got 2". `tsc` accepts it, because its declaration has a
   second type parameter `This`.
3. *(Closed by §231.)* A type argument list on a call of a user function or method was
   outside rule 3. `b.get<i32>()` on a non-generic method, `f<i32>(1)`
   on a `const` arrow, and `Reg.f<i32>(1)` on a static field of a
   function type are accepted, and `tsc` gives TS2558. The awaited form
   `await b.get<i32>()` gives "method `get` is not generic".
