# §173 — tsc rejection instances

Measurement date: 2026-10-07. Code pin: `ae165007dbb6261b8458a318f88426ef708244a3`.
TypeScript: stock 5.9.2. Node: v24.18.0. CLI: a debug build of the code pin.

The 19 target programs cover all ten items in the second list of §154.3.
Three additional programs isolate the reverse-cost class splits in item 10.
TypeScript accepts 16 target programs and rejects three.
The CLI accepts five target programs and rejects 14.
Eight tsc-accepted target programs reach a `TscRejects` site without a block.

Items 1 and 2 close through acceptance. Item 8 closes at its declaration under §155 rule 5.
Item 9 now carries a correct block, but its bare-`this` message remains wrong.
Items 3, 4, 5, 6, 7, and 10 remain open.

Sections §154 through §170 supply the contract comparison for each item.
Sections §160 and §161 change cost, not these acceptance rules.
Sections §163 and §165 through §170 do not close these remaining forms.

## Measurement configuration

Each program is `input.ts` in a separate directory under `$TMPDIR`.
Each TypeScript project extends the repository `tsconfig.json` and includes `prelude/**/*.d.ts`.
The project retains `strict: true`, including `useUnknownInCatchVariables`.
The emission overrides are `noEmit: false`, `module: CommonJS`, `moduleResolution: Node`, and `outDir: out`.
The TypeScript command is `node node_modules/typescript/bin/tsc -p "$project" --pretty false`.
The CLI command is `target/debug/subscript check "$input"`.

Each tsc-accepted program runs its emitted JavaScript under Node.
The wrapper binds `globalThis.print = console.log`, loads `out/input.js`, and calls the exported `main()`.
Every Node execution exits 0 with no stderr.
No tsc-rejected program runs under Node.

The source fences contain the complete programs, including the output probes.
Table positions use `line:column` in those fences.
`none` means no diagnostic, site, class, or block.
Each rejected target produces exactly one CLI diagnostic.
The tables name its code, site, class, and block state.
The message fences preserve the exact diagnostic text.
The site names follow the reached source branches and `compiler/src/check/rejection.rs`.

## 1. Nullable locals

Status: closed. §159 rules 1 and 2 preserve assignment facts across the switch join.
The §159 implementation at `5e708e40` removes the separate assignment-flow reset after the switch.
§162 rule 2 preserves the false condition fact after the while loop.
Both programs now reach no rejection site. No new lowering is necessary.

Evidence: `compiler/src/check/stmt.rs`, `check_switch` and `check_while`.

| Program | tsc exit; diagnostics | CLI exit; code; position | Rejection site | §154 class | Block | Node output |
|---|---|---|---|---|---|---|
| `1-switch` | 0; none | 0; none; none | `none` | `none` | none | `1 1` |
| `1-while` | 0; none | 0; none; none | `none` | `none` | none | `1` |

### `1-switch`

```ts
class N { x: i32 = 1; }
function f(k: i32): i32 { let n: N | null = null; switch (k) { case 1: n = new N(); break; default: n = new N(); break; } return n.x; }
export function main(): void { print(`${f(1)} ${f(0)}`); }
```

### `1-while`

```ts
class N { x: i32 = 1; }
export function main(): void { let n: N | null = null; while (n === null) { n = new N(); } print(`${n.x}`); }
```

## 2. Grouped case labels

Status: closed by §164 rules 1 through 4.
The shared exit predicate follows fallthrough from an empty case body to the next case body.
All three function forms pass the CLI. Node prints the same result for each form.
No new lowering is necessary.

Evidence: `compiler/src/check/fallthrough.rs`; the return checks consume its sequence predicate.

| Program | tsc exit; diagnostics | CLI exit; code; position | Rejection site | §154 class | Block | Node output |
|---|---|---|---|---|---|---|
| `2-function` | 0; none | 0; none; none | `none` | `none` | none | `1 1 0` |
| `2-lambda` | 0; none | 0; none; none | `none` | `none` | none | `1 1 0` |
| `2-method` | 0; none | 0; none; none | `none` | `none` | none | `1 1 0` |

### `2-function`

```ts
function f(k: i32): i32 { switch (k) { case 1: case 2: return 1; default: return 0; } }
export function main(): void { print(`${f(1)} ${f(2)} ${f(3)}`); }
```

### `2-lambda`

```ts
export function main(): void { const f = (k: i32): i32 => { switch (k) { case 1: case 2: return 1; default: return 0; } }; print(`${f(1)} ${f(2)} ${f(3)}`); }
```

### `2-method`

```ts
class N { f(k: i32): i32 { switch (k) { case 1: case 2: return 1; default: return 0; } } }
export function main(): void { const n = new N(); print(`${n.f(1)} ${n.f(2)} ${n.f(3)}`); }
```

## 3. An array length write

Status: open. The message lists `length` as accepted but rejects its write.
The read/write fact already exists in `member_on_context`.
The write reaches `arr_surface_error`, which selects `ArrayUnknownMember` for `length`.

Candidate: accept `xs.length = 0` as a clear operation.
Evaluate the receiver once. Remove every element. Release each counted element under §171, then set the length to zero.
This lowering is mechanical for the measured zero assignment.
`Context::array_truncate` already clears removed storage and changes the length.
It does not release counts, so a direct header write or a bare truncate call is insufficient.
The existing pop/discard paths supply the count-transfer and release operations on all three tiers.
A clear can reuse those operations without a new value model.

General length growth requires a separate decision: JavaScript creates holes, and this language has no array-hole value.
The zero witness proves no claim about arbitrary length writes.
If other writes stay rejected, split the write site from unknown member lookup.
Use `Diverges` with a new array-length-write collision row in `stdlib.md` §9 or C24.
State the write restriction in the message. No current row decides this write restriction.

Evidence: `compiler/src/check/expr/member.rs`, `compiler/src/check/expr/method.rs`, and `runtime/src/context/memory.rs`.
Count evidence: §171 rule 5 and `codegen/src/lir/verify_counted_operations.rs`.

| Program | tsc exit; diagnostics | CLI exit; code; position | Rejection site | §154 class | Block | Node output |
|---|---|---|---|---|---|---|
| `3-length` | 0; none | 1; S100; 1:58 | `ArrayUnknownMember` | `TscRejects` | none | `0` |

### `3-length`

```ts
export function main(): void { const xs: i32[] = [1]; xs.length = 0; print(`${xs.length}`); }
```

CLI message:

```text
error[S100]: `length` is outside the array surface (length, indexing, push, pop, and the Q22 Array methods)
```

## 4. Error stack and cause

Status: open for both members. §155 enforces modifiers; it does not extend the Error member surface.
The builtin Error class exposes no field for either member.
The generic missing-class-member branch therefore selects `ClassUndeclaredMemberRead`.

Candidate: keep each rejection at a dedicated `Diverges` site.
Add explicit `stack` and `cause` rows to `stdlib.md` §19, with their separate reasons.
The existing Error surface carries a class kind, `name`, and `message`.
`stack` requires a call-stack representation and a choice of text across the interpreter, JIT, and C tier.
Node prints execution frames and source positions; a constant message is not that stack.
This lowering is not mechanical from the current Error fields.

`cause` requires an arbitrary payload and an absent-value model.
The measured Error has no cause, and Node prints `undefined`.
C7 excludes `undefined`; the subset also excludes an arbitrary `unknown` value.
A general cause field therefore needs a type and absence decision before lowering.
Adding only a nullable Error cause changes the admitted TypeScript surface.
It does not cover every cause value. That is a separate candidate, not a mechanical fix for the general member.

Retain `TscRejects` for members that neither the Error lib nor the source class declares.
Split on the builtin Error receiver and the known member spelling.

Evidence: `compiler/src/check/expr/member.rs`, `ClassUndeclaredMemberRead`, and `stdlib.md` §19.
The Node stack below uses `<emitted module>` for its temporary filename.

| Program | tsc exit; diagnostics | CLI exit; code; position | Rejection site | §154 class | Block | Node output |
|---|---|---|---|---|---|---|
| `4-stack` | 0; none | 1; S018; 1:69 | `ClassUndeclaredMemberRead` | `TscRejects` | none | `Error: x` plus stack frames |
| `4-cause` | 0; none | 1; S018; 1:69 | `ClassUndeclaredMemberRead` | `TscRejects` | none | `undefined` |

### `4-stack`

```ts
export function main(): void { const e = new Error("x"); print(`${e.stack}`); }
```

CLI message:

```text
error[S018]: `Error` has no member `stack`
```

Node output:

```text
Error: x
    at Object.main (<emitted module>:4:29)
    at [eval]:1:127
    at runScriptInThisContext (node:internal/vm:219:10)
    at node:internal/process/execution:451:12
    at [eval]-wrapper:6:24
    at runScriptInContext (node:internal/process/execution:449:60)
    at evalFunction (node:internal/process/execution:283:30)
    at evalTypeScript (node:internal/process/execution:295:3)
    at node:internal/main/eval_string:71:3
```

### `4-cause`

```ts
export function main(): void { const e = new Error("x"); print(`${e.cause}`); }
```

CLI message:

```text
error[S018]: `Error` has no member `cause`
```

## 5. A Map pair initializer

Status: open. `stdlib.md` §10.4 and §10.9 reject non-Map constructor sources because pairs require tuple types.
`FormNewMapIterable` already names the intended rejection.
The argument check instead infers the inner array as `string[]` and rejects `1` before that site.
The early diagnostic suppresses the constructor diagnostic.

Candidate: keep the constructor rejection, but route this source form to the existing `Diverges` API site first.
Its collision is `stdlib.md` §10.4; its reason is the missing tuple type.
Recognize a non-spread pair-array literal before the homogeneous-array element check.
Keep the accepted `new Map(otherMap)` branch and its type checks.
Do not reclassify the generic array mismatch site: an ordinary `["a", 1]` has a different TypeScript context.
This diagnostic fix needs no lowering.

An acceptance candidate can lower literal pairs to a new Map plus ordered `set` calls.
That literal-only lowering is mechanical if it evaluates every key and value once in source order.
It also needs contextual `K` and `V` checks, duplicate-key semantics, and §172 count operations.
General `new Map(iterable)` support requires tuple and iterable forms that the current type form does not carry.
The one literal witness does not prove that the general lowering is mechanical.

Evidence: `compiler/src/check/expr/array_of_and_map_copy.rs`, `check_map_copy`, and `compiler/src/check/expr/aggregate.rs`.

| Program | tsc exit; diagnostics | CLI exit; code; position | Rejection site | §154 class | Block | Node output |
|---|---|---|---|---|---|---|
| `5-map` | 0; none | 1; S100; 1:70 | `AssignmentTypeMismatch` | `TscRejects` | none | `1` |

### `5-map`

```ts
export function main(): void { const m = new Map<string, i32>([["a", 1]]); print(`${m.size}`); }
```

CLI message:

```text
error[S100]: type mismatch: the array element expects `string`, got `i32`
```

## 6. Nominal classes inside compound types

Status: open for arrays and for both positions of the function type.
C1 still rejects structural substitution. No section from §155 through §170 changes C1.

Candidate: keep each rejection and classify the structural-compatible forms as `Diverges`, with a C1 block.
The failed comparison must compare nested class identities with structural class signatures.
`ts_nominal_assignable` already traverses arrays and function parameters/results with structural class comparisons.
`report_not_assignable` uses it only when both outer types are class-like.
Arrays and function types instead use `ts_erased_assignable`, which keeps nested class identities distinct.
These witnesses therefore reach `AssignmentTypeMismatch` despite their equal public field shapes.

Apply the structural comparison before the generic mismatch fallback for compound types.
Use a dedicated nested-nominality variant whose example contains an array and a function signature.
Keep `TscRejects` when the structural comparison fails.
No lowering is necessary because the form stays rejected under C1.
Acceptance requires a change to C1, not merely a cast between equal layouts.

Evidence: `compiler/src/check/type_rules.rs`, `report_not_assignable`, and `compiler/src/check/rejection_facts.rs`, `ts_nominal_assignable`.

| Program | tsc exit; diagnostics | CLI exit; code; position | Rejection site | §154 class | Block | Node output |
|---|---|---|---|---|---|---|
| `6-array` | 0; none | 1; S100; 3:75 | `AssignmentTypeMismatch` | `TscRejects` | none | `1` |
| `6-function-result` | 0; none | 1; S100; 4:51 | `AssignmentTypeMismatch` | `TscRejects` | none | `1` |
| `6-function-parameter` | 0; none | 1; S100; 4:57 | `AssignmentTypeMismatch` | `TscRejects` | none | `1` |

### `6-array`

```ts
class P { x: i32 = 1; }
class Q { x: i32 = 1; }
export function main(): void { const qs: Q[] = [new Q()]; const ps: P[] = qs; print(`${ps[0].x}`); }
```

CLI message:

```text
error[S100]: type mismatch: the initializer expects `P[]`, got `Q[]`
```

### `6-function-result`

```ts
class P { x: i32 = 1; }
class Q { x: i32 = 1; }
function q(): Q { return new Q(); }
export function main(): void { const p: () => P = q; print(`${p().x}`); }
```

CLI message:

```text
error[S100]: type mismatch: the initializer expects `() => P`, got `() => Q`
```

### `6-function-parameter`

```ts
class P { x: i32 = 1; }
class Q { x: i32 = 1; }
function q(v: Q): i32 { return v.x; }
export function main(): void { const p: (v: P) => i32 = q; print(`${p(new P())}`); }
```

CLI message:

```text
error[S100]: type mismatch: the initializer expects `(P) => i32`, got `(Q) => i32`
```

## 7. An abstract property

Status: open. §155 rule 3 rejects construction of an abstract class, but it does not exempt abstract properties from initialization.
The field-value check reads no abstract-property fact and selects `FieldAssignmentMissingUnassignedExit`.
C24 row 11 lists abstract members, but its reason is only "No lowering is decided."
That text alone does not justify continued rejection.

Candidate: accept the measured abstract field declaration.
Carry its abstract modifier into the class field form, and exempt that field from the concrete-initializer check.
Keep §155's construction rejection. Keep the current inheritance restriction.
The isolated declaration needs no runtime initializer because the program constructs no instance.
That lowering is mechanical for this witness.
It does not decide inheritance, abstract method dispatch, or concrete implementations of the field.
Those forms require separate evidence and contracts.

Alternative: reject the modifier at a dedicated `Diverges` site with C24 row 11 and a concrete reason.
The row must state the missing abstract-member representation and inheritance model.
The message must name the abstract member, not a missing concrete initializer.
No lowering is necessary for this alternative.

Evidence: `compiler/src/check/bodies.rs`, the field-value check, and `compiler/src/check/class_shape.rs`, the inheritance guard.

| Program | tsc exit; diagnostics | CLI exit; code; position | Rejection site | §154 class | Block | Node output |
|---|---|---|---|---|---|---|
| `7-abstract` | 0; none | 1; S100; 1:29 | `FieldAssignmentMissingUnassignedExit` | `TscRejects` | none | `ok` |

### `7-abstract`

```ts
abstract class P { abstract x: i32; }
export function main(): void { print("ok"); }
```

CLI message:

```text
error[S100]: field `x` of `P` has no initializer, and no constructor statement assigns it; write `x: i32 = …`, or assign `this.x = …` at the top level of the constructor
```

## 8. An optional function-type parameter

Status: closed by §155 rules 5 and 7.
The first diagnostic names the optional parameter at its declaration and carries the C7 block.
The call `cb(1)` produces no second diagnostic.
The form stays rejected; no new lowering is necessary.

| Program | tsc exit; diagnostics | CLI exit; code; position | Rejection site | §154 class | Block | Node output |
|---|---|---|---|---|---|---|
| `8-optional` | 0; none | 1; S012; 1:25 | `OptionalFunctionTypeParameter` | `Diverges(OptionalParameter)` | yes | `1` |

### `8-optional`

```ts
function f(cb: (a: i32, b?: i32) => void): void { cb(1); }
export function main(): void { f((a: i32): void => { print(`${a}`); }); }
```

CLI message:

```text
error[S012]: optional parameters in function types are not supported (C7)
```

## 9. An arrow in a static method

Status: changed. Both measured forms now carry `Diverges(ThisStaticMethodMember)` with C24 row 15.
§154 supplies the class split. §157 rule 4 keeps the restriction.
The member form also gives the correct remedy: name the class explicitly.

The bare-`this` message remains open under §157.3 item 1.
It says that `this` is available in methods, although this source is inside a static method.
Candidate: keep the rejection and its existing class/block, but state that a static method must name its class.
This diagnostic fix needs no lowering and no new collision row.
The arrow frame already carries `static_this_class`; the named site already distinguishes this context.

Evidence: `compiler/src/check/rejection_facts.rs`, `reject_static_this_member`, and `compiler/src/check/expr/entry.rs`, the `this` branch.

| Program | tsc exit; diagnostics | CLI exit; code; position | Rejection site | §154 class | Block | Node output |
|---|---|---|---|---|---|---|
| `9-static-this` | 0; none | 1; S100; 1:69 | `ThisStaticMethodMember` | `Diverges(ThisStaticMethodMember)` | yes | `1` |
| `9-static-bare-this` | 0; none | 1; S100; 1:67 | `ThisInStaticMethodArrow` | `Diverges(ThisStaticMethodMember)` | yes | `ok` |

### `9-static-this`

```ts
class N { static x: i32 = 1; static f(): i32 { const g = (): i32 => this.x; return g(); } }
export function main(): void { print(`${N.f()}`); }
```

CLI message:

```text
error[S100]: a static method must name its class instead of `this`
```

### `9-static-bare-this`

```ts
class N { static f(): void { const g = (): void => { const self = this; }; g(); } }
export function main(): void { N.f(); print("ok"); }
```

CLI message:

```text
error[S100]: `this` is only available in constructors and methods
```

## 10. Reverse-cost diagnostics

Status: open at all three sites. The first three rows are the requested rejected programs.
The last three rows are tsc-accepted controls at the same sites.
No section from §155 through §170 closes these class splits.

Candidate: keep the rejections, but split each site on its available source/type fact.
No candidate changes lowering or the accepted language.
The tsc-rejected shapes use `TscRejects` and carry no block.
The accepted controls keep `Diverges` with their existing C6, C21, and C14 rows.

For C6, separate property access on an unnarrowed `unknown` catch binding from permitted TypeScript uses of that binding.
The member-access consumer knows the use, but `reject_caught_read` currently sees only the identifier.
Carry that use fact to the rejection branch.
The template control proves that the whole catch-read site cannot become `TscRejects`.

For C21, separate a void result assigned to an explicit non-void destination from an unannotated void-result binding.
The expression check already receives the contextual type.
The annotated target gets TS2322; the inferred control gets no TypeScript error.
Keep the C21 rejection of a void value, but give the annotated mismatch no block.

For C14, separate an immediate read of a pending local from a read deferred inside a lambda.
`lookup_local_access` already computes the crossed-frame count and the pending binding.
The immediate target gets TS2448 and TS2454.
The closure control is tsc-clean and prints the later local's value, `4`.
Keep its C14 block, and give the immediate read a `TscRejects` site.
Both measured reads currently select `BlockPendingReadWithoutProgramShadow`.

Evidence: `compiler/src/check/exception.rs`, `reject_caught_read`; `compiler/src/check/expr/entry.rs`, `check_expr_with_header_receiver`;
and `compiler/src/check/lookup.rs`, `lookup_local_access`.

| Program | tsc exit; diagnostics | CLI exit; code; position | Rejection site | §154 class | Block | Node output |
|---|---|---|---|---|---|---|
| `10-catch` | 2; TS18046 | 1; S010; 1:80 | `CatchBindingUnnarrowedUse` | `Diverges(Exceptions)` | yes | not run |
| `10-void` | 2; TS2322 | 1; S100; 2:47 | `VoidExpressionValue` | `Diverges(VoidValue)` | yes | not run |
| `10-before` | 2; TS2448, TS2454 | 1; S100; 1:41 | `BlockPendingReadWithoutProgramShadow` | `Diverges(DeclarationScope)` | yes | not run |
| `10-catch-control` | 0; none | 1; S010; 1:83 | `CatchBindingUnnarrowedUse` | `Diverges(Exceptions)` | yes | `Error: x` |
| `10-void-control` | 0; none | 1; S100; 2:42 | `VoidExpressionValue` | `Diverges(VoidValue)` | yes | empty |
| `10-before-control` | 0; none | 1; S100; 2:56 | `BlockPendingReadWithoutProgramShadow` | `Diverges(DeclarationScope)` | yes | `4` |

### `10-catch`

```ts
export function main(): void { try { throw new Error("x"); } catch (e) { print(e.message); } }
```

TypeScript diagnostics:

```text
input.ts(1,80): error TS18046: 'e' is of type 'unknown'.
```

CLI message:

```text
error[S010]: the catch binding `e` is used outside `instanceof` and `throw`; narrow it first with `e instanceof Error`
```

### `10-void`

```ts
function f(): void {}
export function main(): void { const x: i32 = f(); print(`${x}`); }
```

TypeScript diagnostics:

```text
input.ts(2,38): error TS2322: Type 'void' is not assignable to type 'number'.
```

CLI message:

```text
error[S100]: a `void` expression is only allowed as an expression statement
```

### `10-before`

```ts
export function main(): void { print(`${x}`); const x: i32 = 1; }
```

TypeScript diagnostics:

```text
input.ts(1,41): error TS2448: Block-scoped variable 'x' used before its declaration.
input.ts(1,41): error TS2454: Variable 'x' is used before being assigned.
```

CLI message:

```text
error[S100]: `x` is read before its declaration in this block
```

### `10-catch-control`

```ts
export function main(): void { try { throw new Error("x"); } catch (e) { print(`${e}`); } }
```

CLI message:

```text
error[S010]: the catch binding `e` is used outside `instanceof` and `throw`; narrow it first with `e instanceof Error`
```

### `10-void-control`

```ts
function f(): void {}
export function main(): void { const x = f(); }
```

CLI message:

```text
error[S100]: a `void` expression is only allowed as an expression statement
```

### `10-before-control`

```ts
const outer: i32 = 3;
export function main(): void { const read = (): i32 => outer; const outer: i32 = 4; print(`${read()}`); }
```

CLI message:

```text
error[S100]: `outer` is read before its declaration in this block
```

## Groups for subsequent sections

- **Checker flow analysis:** no open target remains from items 1 and 2. §159, §162, and §164 cover their measured shapes.
- **Member surface:** items 3, 4, and 5. Decide the zero-length write, the two Error members, and Map constructor rejection order.
  Item 3 has a mechanical clear candidate. Item 4 needs explicit runtime/type decisions or separate rejection rows.
  Item 5 can retain its current collision and repair the first diagnostic without lowering.
- **Type relation:** item 6. Extend the structural class comparison into compound types and retain C1 with correct blocks.
- **Class declaration:** item 7. Carry the abstract-member fact and decide its initializer exemption or a specific C24 rejection.
- **Diagnostic class:** item 10 and the residual message of item 9.
  Split the three reverse-cost sites and retain the existing collision blocks for their accepted controls.
  Correct the bare static-arrow message without a new lowering.

The tracking note contains measurements and candidates only. It changes no contract or production code.

## Rule 4 branch measurement

Code pin: `451edc8b`. TypeScript: stock 5.9.2. CLI: a debug build of this pin.
Each program uses the measurement configuration above, with `noEmit: true`.
Each source file resides under `$TMPDIR`.

### Abstract property implementation

```ts
abstract class A { abstract x: i32; }
class B extends A { x: i32 = 1; }
export function main(): void { print(`${new B().x}`); }
```

TypeScript exits 0 with no diagnostics. The CLI exits 1 with two S100 diagnostics:

| Position | Message | Site and class |
|---|---|---|
| 1:29 | field `x` of `A` has no initializer, and no constructor statement assigns it; write `x: i32 = …`, or assign `this.x = …` at the top level of the constructor | `FieldAssignmentMissingUnassignedExit`, `TscRejects`, no block |
| 2:17 | class inheritance is not in the decided surface | `ReferenceClassInheritance`, `Diverges(ReferenceClassInheritance)`, `compiler.md` §115.1 block |

### Abstract method implementation

```ts
abstract class A { abstract f(): i32; }
class B extends A { f(): i32 { return 1; } }
export function main(): void { print(`${new B().f()}`); }
```

TypeScript exits 0 with no diagnostics. The CLI exits 1 with two S100 diagnostics:

| Position | Message | Block |
|---|---|---|
| 1:29 | function bodies are required | C24: no lowering is decided for abstract members |
| 2:17 | class inheritance is not in the decided surface | `compiler.md` §115.1: no class inheritance |

Both concrete subclasses implement their abstract member. The checker rejects both `extends` declarations.
Rule 4 selects the rejection branch: no subclass can implement the member within the accepted language.
The selected branch requires `AbstractMember` with C24 row 11.
Row 11 needs this reason: no subclass can implement an abstract member because the language rejects class inheritance.
The contract must select this branch before implementation.

## §173 fact splits

Contract pin: `a10b87ab`. TypeScript: stock 5.9.2.
Each corpus entry uses a copy under `$TMPDIR` for its CLI measurement.
The TypeScript project uses strict checks, ES2022, and the language prelude.

### Corpus evidence at the contract pin

| Entry | TypeScript | Pin diagnostic | Pin class or message defect |
|---|---|---|---|
| `r389-error-outside-surface` | accepts | S018 at 8:69 and 8:90: `Error` has no member `stack` or `cause` | Both diagnostics lack a block. |
| `r390-map-pair-initializer` | accepts | S100 at 8:70: type mismatch: the array element expects `string`, got `i32` | The element check precedes the Map rejection. |
| `r391-nested-nominal-classes` | accepts | S100 at 12:75, 12:98, and 12:126: the initializer type mismatch | The array, function result, and function parameter diagnostics lack a block. |
| `r392-abstract-property` | accepts | S100 at 8:29: field `x` of `P` has no initializer, and no constructor statement assigns it | The diagnostic names initialization and lacks a block. |
| `r393-static-arrow-this` | accepts | S100 at 8:67: `this` is only available in constructors and methods | The block is correct. The message names the wrong context. |
| `r394-tsc-rejected-reverse-cost` | TS18046, TS2322, TS2448, TS2454 | S010 at 10:51; S100 at 11:22 and 12:12 | All three diagnostics carry an incorrect block. |

The pin rejects all six entries with exit 1.
The corpus block test reports five entries with incorrect blocks.
All eight dedicated fact tests fail at the pin.
The static-arrow test reports the message defect that the corpus block test cannot detect.

### Resulting sites

| Guard | Site | Class and collision |
|---|---|---|
| Receiver class ID equals `self.error_class`; `stack` or `cause`, read or write | `ErrorMemberOutsideSurface` | `Diverges(ErrorMemberOutsideSurface)`, `stdlib.md` §19 |
| Non-spread pair-array literal with keys and values assignable to the Map types | `FormNewMapIterable` | `Diverges(NoTupleType)`, `stdlib.md` §10.4 |
| Compound mismatch that the structural comparison accepts | `NestedNominalClass` | `Diverges(NestedNominalClass)`, C1 |
| Abstract property declaration | `AbstractMember` | `Diverges(AbstractMember)`, C24 row 11 |
| Bare `this` in a static-method arrow | `ThisInStaticMethodArrow` | `Diverges(ThisStaticMethodMember)`, C24 row 15 |
| Property access on an unnarrowed catch binding | `CatchBindingUnnarrowedProperty` | `TscRejects`, no block |
| Void expression with a non-void destination type | `VoidExpressionNonVoidDestination` | `TscRejects`, no block |
| Pending local read without a lambda boundary | `ImmediatePendingLocalRead` | `TscRejects`, no block |

The Error guard compares the receiver class ID with `self.error_class` at both read and write sites.
Builtin error constructors use this shared class ID. A source class named Error keeps the missing-member site.
The pair-array guard precedes homogeneous array inference and checks each key and value against the Map types.
A non-assignable pair keeps `AssignmentTypeMismatch` with no block. Map copies keep their existing branch.
Unannotated pair inference includes null values and compares numeric types through their TypeScript number type.
The Map API witness includes both null orders, repeated null values, and mixed numeric literals.
The nested comparison checks arrays, function parameters, and function results at any depth.
The comparison reads the collected private and protected member facts.
Distinct restricted members keep `AssignmentTypeMismatch`, because TypeScript requires their declaration identity.
Eight field and method controls get TS2322 and keep no block.
A declared scalar type parameter keeps its existing site. Its apparent constraint cannot select the compound guard.
An abstract property rejects at its name and stays in the class shape with its declared type.
The field-value check excludes abstract properties. A `this.x` read adds no diagnostic.
The static-arrow message says: ``a static method must name its class instead of `this`; use `ClassName.member` ``.

The catch template, inferred void binding, and deferred local read keep their C6, C21, and C14 blocks.
A parenthesized unknown catch receiver gets TS2571. Direct, indexed, and called property uses get TS18046.
Optional named property accesses get TS2339. Optional indexed property accesses get TS7053. Neither carries a block.
The site table includes all six new sites and the controls for the original sites.
The new NestedNominalClass fragment contains an array assignment and a function type assignment.

### Required collision row text

| Record | Required text |
|---|---|
| `stdlib.md` §19, `stack` | Reject `Error.stack`: the three execution tiers have no call-stack representation with frames and source positions. |
| `stdlib.md` §19, `cause` | Reject `Error.cause`: it requires an arbitrary payload and an absent-value model. C7 provides no `undefined`. |
| `stdlib.md` §10.4 | Reject pair-array Map construction before array element inference. Pair elements require tuple types. Construct an empty Map, then call `set`. |
| C1 | Class identity remains nominal inside compound types, including array elements and function parameters and results at any depth. |
| C24 row 11 | Reject abstract properties: no subclass can implement an abstract member because the checker rejects class inheritance. |
| C24 row 15 | A static method must name its class explicitly. An arrow in that method follows the same rule. Use `ClassName.member`. |

The existing Rule 4 branch measurement above supplies the inheritance evidence.

### Test evidence

The §154 total test covers 1,677 witnesses and 476 variants in one TypeScript process.
The test passes. TypeScript costs 1.410 seconds; the checker costs 1.115 seconds; the total costs 3.144 seconds.
Four extended fact tests fail before these corrections. All eleven tests pass after them.
The eleven dedicated fact tests pass, including write controls, non-assignable Map pairs, and optional catch property accesses.
Fifteen added witnesses measure these cases and their controls in the §154 TypeScript batch.
The r392 corpus test requires exactly one AbstractMember diagnostic after its `this.x` read.
The type-shape test pins the updated compound guard without Map and Set branches.
The generic matrix keeps its existing scalar classification and passes all 39,119 cells.
The final CLI rejects each new entry with exit 1.
The r389, r390, r391, r392, and r393 entries carry two, one, three, one, and one blocks, respectively.
The r394 entry carries three diagnostics and no block.

### Required checks

| Check | Result |
|---|---|
| `cargo build --offline --locked --workspace --all-targets` | pass |
| `cargo test --offline --locked -p subscript-compiler` | pass |
| `cargo test --offline --locked -p subscript-codegen` | pass |
| `cargo test --offline --locked -p subscript-runtime` | pass |
| `cargo fmt --check` | pass |
| `cargo clippy --offline --locked --workspace --all-targets` | pass; no new warning |
| `tools/hygiene.sh` | pass |

Clippy reports existing warnings. The corrections add no warning.
The tree keeps every existing `.expected` file.
Every changed Rust file stays below 2,000 lines.
The generator supplies both generated document changes.

### Changed files

- `compiler/src/check/bodies.rs`
- `compiler/src/check/class_shape.rs`
- `compiler/src/check/exception.rs`
- `compiler/src/check/expr/array_of_and_map_copy.rs`
- `compiler/src/check/expr/call.rs`
- `compiler/src/check/expr/entry.rs`
- `compiler/src/check/expr/literal.rs`
- `compiler/src/check/expr/member.rs`
- `compiler/src/check/expr/operator.rs`
- `compiler/src/check/lookup.rs`
- `compiler/src/check/member_modifiers.rs`
- `compiler/src/check/mod.rs`
- `compiler/src/check/rejection.rs`
- `compiler/src/check/rejection_fact_tests.rs`
- `compiler/src/check/rejection_facts.rs`
- `compiler/src/check/rejection_programs.txt`
- `compiler/src/check/rejection_sites.rs`
- `compiler/src/check/rejection_targets.txt`
- `compiler/src/check/rejection_witness_index.rs`
- `compiler/src/check/rejection_witness_sites.rs`
- `compiler/src/check/rejection_witnesses.txt`
- `compiler/src/check/type_rules.rs`
- `compiler/src/divergence.rs`
- `compiler/src/divergence/entries.rs`
- `compiler/tests/apparent_type_shapes.rs`
- `compiler/tests/array_of_and_map_copy.rs`
- `compiler/tests/corpus_reject.rs`
- `corpus/reject/r389-error-outside-surface.ts`
- `corpus/reject/r390-map-pair-initializer.ts`
- `corpus/reject/r391-nested-nominal-classes.ts`
- `corpus/reject/r392-abstract-property.ts`
- `corpus/reject/r393-static-arrow-this.ts`
- `corpus/reject/r394-tsc-rejected-reverse-cost.ts`
- `generated-docs/corpus-index.md`
- `generated-docs/language-reference.md`
- `specs/blocks/collisions.md` — Claude edited this file.
- `specs/blocks/stdlib.md` — Claude edited this file.
- `specs/tracking/s173-tsc-rejects-instances.md`
