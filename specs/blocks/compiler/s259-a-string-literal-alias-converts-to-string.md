<!-- §259 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 259. A string-literal alias converts to `string`

*(Added 2026-11-13.)* Origin: the Q32 operations of
`specs/blocks/collisions.md` and §24.1. The owner selected the scope on
2026-11-13: the conversion applies at a place that expects `string`,
and the comparison and the members of §24.1 do not change.

Problem, measured at `9e8638ac` with `tsc` 5.9.2, the libraries of the
project, and the debug CLI. `type Mode = "a" | "b";`,
`type Wire = CEnum<{ low: 3; high: 7; }>;`, `m: Mode` holds `"b"`,
`w: Wire` holds `"high"`, and `show(s: string): void` prints `s`.
`tsc` accepts each form, with no error. The checker gives S100 "type
mismatch: the … expects `string`, got `Mode`" at `m`.

| Form | Place |
|---|---|
| `const s: string = m;` | initializer |
| `let s: string = "x"; s = m;` | assignment |
| `show(m);`, `print(m);` | argument |
| `new P(m)`, `constructor(public s: string)` | argument of `new` |
| `xs.push(m)`, `xs: string[]` | argument of a method |
| `function name(x: Mode): string { return x; }` | return value |
| `const f = (): string => m;` | lambda body |
| `const xs: string[] = [m, "c"];` | array element |
| `k.s = m;`, `s: string` a field of a class | assignment to a field |
| `const s: string \| null = m;` | initializer, nullable |
| `const s: string = w;` | initializer, wire alias |

Template interpolation runs at the pin, as a control:
`` print(`t ${m}`) `` prints `t b`.

`tsc` types a literal union as a subtype of `string`. §24.1 and Q32
reject an assignment with `string` in the two directions, and give no
reason for the direction from the alias to `string`. The value has a
static string table (§24.2), so the conversion is mechanical. A string
literal is interned in its Context (`subscript_rt_str_lit`), so a
string made from the table has the cost of a string literal of the
same text.

These forms stay rejected, measured at the pin:

| Form | Checker | `tsc` |
|---|---|---|
| `const n: Mode = s;`, `s: string` | S100 | TS2322 |
| `m === s`, `s: string` | S100 "operator `===` not defined for `Mode` and `string`" | accepts |
| `m.length` | S100 "`Mode` has no member `length`" | accepts |
| `"x" + m` | S100 "operator not defined for `string` and `Mode`" | accepts |
| `const xs: string[] = ms;`, `ms: Mode[]` | S100 | accepts |

### 259.1 Rules

1. A value of a string-literal alias, plain (§24) or wire (§50),
   converts to `string` at each place that checks a value against the
   expected type `string`, `string | null`, or `string | undefined`.
   These places include an initializer, an assignment, an argument of
   a call, a method call, or a `new`, a return value, a lambda body,
   an array element, and a field.
2. The text of the converted value is the member spelling. It is the
   text that template interpolation prints (§24.2, §216). For a wire
   alias, it is the member name, not the wire value.
3. The conversion makes the string as a string literal of the same
   text does. It takes no allocation that such a string literal does
   not take.
4. These stay rejected: a `string` where an alias is expected, an
   alias where another alias is expected, `===` and `!==` with
   `string`, a member of `string` on an alias value, an operator with
   a `string` operand, and a container or function type of the alias
   where the same container or function type of `string` is expected.
5. An array `Mode[]` holds `i32` discriminants and `string[]` holds
   string handles, so one array cannot be the other. A conversion
   makes a new array: its cost depends on the length, and a write
   through one name does not reach the other. `Mode[]` where
   `string[]` is expected stays rejected.
6. The comparison of §24.1 does not change: `===` and `!==` between
   alias values and against member literals lower to an integer
   compare.
7. No program that `tsc` rejects changes from rejected to accepted. The
   development tier, the ship tier, and the interpreter give the same
   output. The accept entry carries `js-comparable` and equals `node`.

### 259.2 Acceptance

1. Red first, at the pin: the accept entry
   `a443-a-string-literal-alias-converts-to-string` with each form of
   the first table is rejected. The tracking note records the output.
2. After the change, `a443` passes `tsc`, the three tiers, and `node`.
   It prints each converted value, and the `length` of one through a
   `string` local.
3. The reject entry `r501-an-alias-array-is-not-a-string-array` holds
   `const xs: string[] = ms;` with `ms: Mode[]`. Its header states that
   `tsc` accepts it and cites Q32.
4. A unit test pins rules 1 to 6: each form of the first table is
   accepted, with the type of the converted value, and each form of the
   second table is rejected, with its diagnostic. A control: an alias
   comparison has no string comparison call in the emitted C (§24.4
   item 3).
5. The implementation commit adds a revision paragraph to Q32 in
   `specs/blocks/collisions.md`, and a line to §24.1 that cites this
   section.
6. `main` of `a443` is not async and not a generator, so the LIR text
   snapshot does not change. No golden moves. If a corpus entry or
   golden changes its type or output, report it and stop.

### 259.3 Phase Review

One fresh review of the implementation. No CRITICAL finding.

1. MAJOR, realistic, fixed: a `case` label goes through the check of a
   value place, so `switch (s) { case m: }` with `s: string` was
   accepted, while `s === m` stays rejected (rule 4). A `case` label is
   a comparison, so it is rejected again. The generic matrix pin
   returns to 7815.
2. MINOR, realistic, fixed: Q32 stated that the string table is used
   only for formatting. The Q32 revision names the conversion.
3. MINOR, realistic, fixed: a codegen test ran `a443` on the three
   tiers, which the golden harness already does.
4. MINOR, realistic, fixed: the unit test found a conversion by the
   text `Cast(` only. It now checks the type of each conversion.
5. MINOR, realistic, fixed: no test built a malformed `AliasString`
   for the LIR verifier. A test now does, with a valid control.
6. MINOR, contrived, recorded: §259.4 item 1.
7. MINOR, realistic, recorded: §259.4 items 2 and 3.

### 259.4 Open

1. A wire value that is not a member converts to the first member in
   the development tier and the ship tier, and stops the interpreter.
   Template interpolation has the same difference. A script cannot
   make such a value: `5 as Wire`, `new Array<Wire>(n)`, and
   `w!: Wire` are rejected. Only a host can pass one.
2. An array literal with no annotation takes the type of its first
   element. `[m, "c"]` gives S100 "array element expects `Mode`, got
   `string`", and `["c", m]` is accepted as `string[]`. `tsc` accepts
   the two.
3. `m as string` gives S100 "cannot convert `Mode` to `string`".
   `tsc` accepts it.
4. `m === s`, `m.length`, and `"x" + m` stay rejected (rule 4). `tsc`
   accepts each.
