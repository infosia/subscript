# An operand hold is a release owner — `compiler.md` §255

Status: IN PROGRESS.

## Measurement at `96faa31e` (Step 0)

A test file in the working tree, removed after the round, ran the
release model of the LIR test (`codegen/tests/support/lir_facts_release.rs`)
and the three tiers. `v(): Promise<i32>` is async, `two(a, b)` awaits
both, `boom(): Promise<i32>` throws before it returns, and `bad()` is an
async function that throws.

| Program (async `main`) | Release model | Output, interpreter = JIT = ship | live bytes | Control live bytes |
|---|---|---|---|---|
| `await v()` | none | `1` | 9 | – |
| `await two(v(), v())` | "exception edge releases 1 owned handles; lexical scopes require 0" at the call | `2` | 9 | 9 |
| `const a = v(); const b = v(); await two(a, b)` | none | `2` | 9 | 9 |
| `two(v(), boom())` in a `try` | two findings | `caught`, `1` | 72 | 72 (`await boom()` alone) |
| `two(v(), bad())` in a `try` | one finding | `caught`, `1` | 71 | 71 (`await bad()` alone) |
| `two(bad(), v())` in a `try` | one finding | `caught`, `1` | 71 | 71 |

The live bytes are the development tier memory accounting after the run,
with the diagnostics of a freed handle on. Each program with a finding
has the live bytes of its control, which holds no handle: the residual
is the caught `Error`. No diagnostic of a freed handle fired.

The LIR of `main` for `two(v(), v())`: `%0` and `%1` are the two handles,
`AsyncHandleRetain(%0)` follows the second argument, and the suspension
carries `%0` into the resume block as `%4`. The resume block releases
`%4` on the normal path, and the release landing of the `await` releases
`%4` on the exception edge. `%4` is the binding `<input hold N>` of the
lowering: the first argument is held while the second argument runs user
code. The counts balance on each path. The model counted only the
lexical owners of the HIR, so it required 0 releases where the lowering
releases 1.

Each such call does one retain and one release more than the callee
needs, because the hold ends at the end of the expression, not at the
call (§255.4).

## Audit of the bindings of the lowering

Each binding that the lowering declares, against rule 1 (an immutable
local is a `const` declaration, a `const` binding of a `for…of` over a
generator, or a capture of a lambda that owns its environment):

| Binding | Lowering, mutable | Rule 1 | Result |
|---|---|---|---|
| `let` / `const` declaration (`stmt.rs`, the HIR `mutable` flag) | follows the declaration | `const` is immutable | same |
| destructuring in a declaration, a `for (let …;;)` init, a `switch` case declaration | an HIR `Let`, the same flag | the same | same |
| a `for…of` binding over an array, a `Map`, a `Set`, a string, or a `FixedArray` | `true` | not immutable | same |
| a generator `for…of` binding (the HIR `mutable` flag) | follows the declaration | a `const` binding is immutable | same |
| a parameter (`add_parameter`) | `true` | not immutable | same |
| a capture of a lambda that owns its environment | `false` | immutable | same |
| a capture of a lambda that borrows its environment (`add_parameter`, `ParameterKind::Capture`) | `true` | not immutable | same |
| the `catch` binding | `false` | not listed | the type is `Error`, never counted, so no hold |
| a `using` binding | not a `declare_binding` site | not listed | the type is a disposable class, never counted, so no hold |
| a global or a module binding read in a function | not a local: held | not a local: held | same |
| the hidden bindings (`<input hold>`, `<for-of owner>`, `<for-of place>`, `<generator for-of owner>`, `<generator for-of step>`, cursors) | `false` or hidden | no source name | no source read |

The rounds before the audit found three forms where an earlier text of
rule 1 and the lowering differed: an array or a generator operand
(`takeArr(arr(), v())`, `takeGen(gen(), v())`), a `const` binding of an
array `for…of`, and a `const` binding of a generator `for…of`. Each one
ran correctly on each tier with the live bytes of its control. The
audit then found the capture of a lambda that borrows its environment:
`const f = (): Promise<i32> => two(a, boom())` in a `try` gave
"exception edge releases 1 owned handles; lexical scopes require 0",
`c` then `1`, and 64 live bytes, as its control. Rule 1 now states
each form as the lowering has it.

## Audit of the operand holds of the lowering

The Phase Review found holds that the earlier rule 1 did not name. Each
place in `codegen/src/lir` that takes an operand hold. `hold_input`
holds a value of a counted type only, with one count, by a hidden
binding `<input hold N>`. "Needs hold" is false only for a read of a
local that the lowering declares immutable. The span ends at the
`finish_input_holds` of the innermost hold scope: a call
(`call.rs:13`), an expression of `lower_expr` (`expr.rs:83`: an async
call, an async handle creation, an array literal), a read
(`array_ownership.rs:163`), or an assignment (`expr.rs:774`).

| Site | Expression | Operand held | Condition | Counts | Span ends | Earlier rule 1 |
|---|---|---|---|---|---|---|
| `call.rs:142-153` | call | the receiver of a method; the callee value (`SubFn`, never counted) | a generator `next`, or an argument runs user code, or a parameter default of the callee runs user code; not an immutable local receiver | 1 | the call | not named |
| `call.rs:784-794` | call, `new`, async call, async handle creation | an argument | needs hold; a later argument, or the default of an omitted parameter, runs user code | 1 | the call | named |
| `call.rs:840-848` | the same | the default value of an omitted parameter | the default of a later parameter runs user code | 1 | the call | named; the model did not walk defaults |
| `expr.rs:336-349`, `expr.rs:385-395` | array literal, spread literal | an element | needs hold; a later element runs user code | 1 | the literal | named |
| `place.rs:110-113` | an indexed place that is not a `FixedArray` place: a read, an assignment target, a compound or update target | the base | needs hold; the index runs user code | 1 | the read, or the assignment | a read not named |
| `expr.rs:841-852` | assignment to an indexed place | the base | needs hold; the value runs user code | 1 | the assignment | named |

When the index and the value of an indexed assignment both run user
code, the base holds two counts: the first hold spans the second, so
the second is redundant (§255.4 item 3). The lowering's predicate of
user code does not include `Promise.all`; the earlier model did.

Measured with the model of rule 1 before this audit:

| Program | Finding | LIR |
|---|---|---|
| `let tasks: Promise<i32>[] = []; tasks.push(start(i))` in a `try`, `start` throws | "releases 1; require 0" at `start` | the receiver is held |
| `let hs …; await hs[it(9)]` in a `try`, `it` throws | "releases 1; require 0", and "releases 2; require 1" outside the `try` | the base is held for a read |
| the same with `const hs` | none | no hold |
| `let hs …; hs[idx()] = boom()` in a `try` | "releases 2; require 1" | two retains of the base |
| `let hs …; hs[0] = boom()` in a `try` | none | one hold |

A default value is lowered as a helper function (`<default …>`) that
takes the parameters before it, so a call that evaluates a raising
default has two edges at the position of the default: the call of the
helper in the caller, and the raise in the helper. The model walks each
evaluated default in the scope of the callee parameters before it, as
its lexical owners, and requires the edge of the call with the owners of
the caller.

## Red at the pin

The release model of `e0c0d962` (the code of `96faa31e`; the contract
commits changed specs only) on
`a439-an-operand-hold-is-a-release-owner.ts` gave 14 findings:

- 79:18 (`two(v(), v())`), 81:21 and 81:36 (`two(v(), boom())`), 86:22
  (`two(bad(), v())`), 106:40 (the lambda `two(c, boom())`): "exception
  edge releases 1 owned handles; lexical scopes require 0";
- 93:18 (`takeArr(arr(), v())`) and 94:22 (`takeGen(gen(), v())`):
  "releases 3 …; … require 2";
- 97:21 (the array `for…of`) and 103:28 (the generator `for…of` with
  `let`): "releases 7 …; … require 6";
- 116:18 (`tasks.push(start(i))`), 126:28 (`await ps[it(9)]`), 137:22
  (`withDefault(v())`, its default throws): "releases 1 …; … require
  0";
- 131:17 (`ps[idx()] = boom()`): "releases 2 …; … require 0";
- 135:24 (`await ps[it(0)]` outside a `try`): "releases 7 …; … require
  6".

## After the change

The release model reports no finding on `a439`, nor on the corpus (the
`lir` test, 65 tests). `a439` checks with no errors and `tsc` 5.9.2
accepts it. The development tier, the ship tier, the interpreter, and
`node` give the same bytes: `plain 2`, `throws caught`, `rejects
caught`, `held 2`, `array 2`, `generator 31`, `for-of 2`, `generator
const 31`, `generator let 31`, `capture caught`, `after 1`, `push caught
2`, `task 1`, `task 1`, `read caught`, `store caught`, `ps 1`, `default
caught`, one per line. `main` is async, so the LIR text snapshot grows
by the `a439` block only (1,421 lines added, none removed).

## Form

- `codegen/tests/support/lir_facts_release.rs` reads the six sites of
  rule 1 and rules 2 and 3 from the HIR, with its own `runs_user_code`,
  which leaves out `Promise.all`, and no call into the lowering. A call
  walks the receiver, the arguments, and the default of each omitted
  parameter in order (`call`); an array literal, an indexed read, and an
  assignment to an indexed place walk their operands in order
  (`hold_sequence`). A held operand adds its counts from its evaluation
  to the end of its expression; the base of an indexed assignment adds
  one count for an index that runs user code and one for a value that
  runs user code. A default that a call evaluates is walked in the scope
  of the callee parameters before it, and its raise is an edge of the
  call.
- The walk keeps a stack of the locals in scope, each with whether it is
  immutable by rule 1, from the declarations, the `for…of` heads, the
  parameters, and the captures with `owns_environment`.
- The unit test `an_operand_hold_is_released_on_the_exception_edge`
  removes the release of the hold from the landing of the `await` in
  `two(v(), v())`. The model then reports "exception edge releases 0
  owned handles; lexical scopes require 1". The control is the same LIR
  with the release.

## Files

- `codegen/tests/support/lir_facts_release.rs`
- `corpus/accept/a439-an-operand-hold-is-a-release-owner.ts` and
  `.expected`
- `codegen/tests/lir-goldens/corpus.txt` (the `a439` block),
  `generated-docs/corpus-index.md`
- `specs/blocks/compiler/s236-an-async-return-adopts-a-handle.md`
  (§236.4 item 4 closed)
