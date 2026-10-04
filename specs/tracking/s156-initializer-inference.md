# §156 round 1: initializer inference

## Status

Stopped at the form requirement in the handoff. No production code changed.
The compiler and codegen test suites did not run. Three-tier agreement is not established.

## Red evidence

Contract pin: `51927a28`.
The working tree was clean before this round.
`cargo build --offline --locked -p subscript-cli` passed before any change.

The pin's CLI rejected `corpus/accept/a329-initializer-inference/main.ts` with exit 1 and 16 S100 diagnostics.

| File | Position | Message |
|---|---|---|
| main.ts | 9:7 | module-level variables require a type annotation |
| main.ts | 10:5 | module-level variables require a type annotation |
| main.ts | 11:7 | module-level variables require a type annotation |
| main.ts | 12:7 | module-level variables require a type annotation |
| main.ts | 13:7 | module-level variables require a type annotation |
| main.ts | 14:7 | module-level variables require a type annotation |
| main.ts | 17:5 | fields require a type annotation |
| main.ts | 18:5 | fields require a type annotation |
| main.ts | 19:5 | fields require a type annotation |
| main.ts | 20:12 | static fields require a type annotation |
| main.ts | 25:5 | fields require a type annotation |
| main.ts | 28:23 | parameters require a type annotation |
| main.ts | 30:22 | parameters require a type annotation |
| lib.ts | 7:14 | module-level variables require a type annotation |
| main.ts | 35:22 | a lambda with a block body requires a return type annotation |
| main.ts | 39:22 | a lambda with a block body requires a return type annotation |

The following command accepted a329 with exit 0:

```sh
node_modules/.bin/tsc --strict --target ES2022 --module commonjs \
  --lib ES2022,ESNext.Disposable --outDir "$scratch" \
  prelude/lang.d.ts corpus/accept/a329-initializer-inference/main.ts \
  corpus/accept/a329-initializer-inference/lib.ts
```

Node ran the emitted CommonJS module with `print = console.log` and a no-op `ValueType` decorator.
The driver called `main()` once. Its output supplied the new golden:

```text
10,2,0.5,pool,3,6,7
4,c,1.5,8,3
5,0
pool
```

No existing golden changed.

## Missing declaration facts

`compiler/src/check/mod.rs` defines `GlobalSig` with only `ty` and `mutable`.
`ScopeItem::Global` carries only its symbol.
Neither form distinguishes an undecided inferred declaration from a rejected declaration or an annotated declaration.

`compiler/src/check/expr/literal.rs` reads `GlobalSig.ty` and substitutes `Type::Error` when the signature is absent.
Rule 3 needs a separate undecided state at that consumer.
An undecided unannotated read needs a diagnostic; a rejected declaration needs poison suppression under §154 rule 9.
`Type::Error` cannot state both facts.

The declaration form also needs to preserve a delayed lambda-result dependency:

```ts
const readLater = () => later;
const later = 1;
export function main(): void { print(`${readLater()}`); }
```

Stock `node_modules/.bin/tsc` accepted this probe with `--noEmit --strict --target ES2022 --module ESNext` and the prelude.
The pin's CLI rejected both module declarations with S100.

Rule 3 excludes the deferred lambda read from the early-read rejection.
At `readLater`'s declaration, the current form carries neither `later`'s decided type nor a deferred dependency for the lambda result.
`check_lambda` checks the body immediately; `GlobalSig` holds only a concrete `Type`.

The handoff requires a stop when a consumer needs a fact that its form does not carry.
This round stops before a second inference path or a consumer-side initializer scan supplies those missing facts.
The declaration form needs an explicit type-decision state and a representation for delayed lambda-result dependencies.

Rules 1–7, the reject entries, the layout test, rejection-site migration, and the requested Green checks remain incomplete.

## Round 2 status

Stopped at the handoff's form requirement. No production code changed.
The decision state does not carry an annotated type before the checked initializer exists.
An annotated recursive lambda needs that type during its initializer check.
The handoff rejects every read of an in-progress declaration as an inference cycle.

## Round 2 Red evidence

Contract pin: `210fd9b8`.
The round 1 corpus files and tracking note were present at the start.
`cargo build --offline --locked -p subscript-cli` passed before this round changed any file.
The accept entry now includes the rule 3 probe:

```ts
const readLater = () => later;
const later = 1;
```

The pin's CLI rejected the updated a329 with exit 1 and 18 S100 diagnostics.

| File | Position | Message |
|---|---|---|
| main.ts | 9:7 | module-level variables require a type annotation |
| main.ts | 10:5 | module-level variables require a type annotation |
| main.ts | 11:7 | module-level variables require a type annotation |
| main.ts | 12:7 | module-level variables require a type annotation |
| main.ts | 13:7 | module-level variables require a type annotation |
| main.ts | 14:7 | module-level variables require a type annotation |
| main.ts | 15:7 | module-level variables require a type annotation |
| main.ts | 16:7 | module-level variables require a type annotation |
| main.ts | 19:5 | fields require a type annotation |
| main.ts | 20:5 | fields require a type annotation |
| main.ts | 21:5 | fields require a type annotation |
| main.ts | 22:12 | static fields require a type annotation |
| main.ts | 27:5 | fields require a type annotation |
| main.ts | 30:23 | parameters require a type annotation |
| main.ts | 32:22 | parameters require a type annotation |
| lib.ts | 7:14 | module-level variables require a type annotation |
| main.ts | 37:22 | a lambda with a block body requires a return type annotation |
| main.ts | 41:22 | a lambda with a block body requires a return type annotation |

Stock `tsc` accepted the updated entry with exit 0:

```sh
node_modules/.bin/tsc --noEmit --strict --target ES2022 --module ESNext \
  --lib ES2022,ESNext.Disposable prelude/lang.d.ts \
  corpus/accept/a329-initializer-inference/main.ts \
  corpus/accept/a329-initializer-inference/lib.ts
```

## Round 2 missing declaration fact

`signatures.rs::resolve_signatures` resolves an annotated global's type in pass B.
`bodies.rs::check_body_decl` checks its initializer in pass C.
`expr/literal.rs::check_ident` reads that declared type during the initializer check.
An initializer can read its own declaration inside a lambda:

```ts
const countdown: (n: i32) => i32 =
    (n: i32): i32 => n === 0 ? 0 : countdown(n - 1);
export function main(): void { print(`${countdown(3)}`); }
```

The pin's CLI and stock `tsc` accepted this probe with exit 0.
Both also accepted a pair of annotated bindings whose lambda bodies call each other.
The `tsc` commands used `--noEmit --strict --target ES2022 --module ESNext` and the prelude.

An annotation also breaks a type-inference cycle:

```ts
const read = () => value();
const value: () => i32 = (): i32 => read();
export function main(): void {}
```

Stock `tsc` accepted this probe with the same options and exit 0.

These measurements establish acceptance. The conflict below follows from the requested transitions; no modified checker ran.

The handoff replaces `GlobalSig.ty` with one state and requires a checked initializer in the decided state.
Before that checked form exists, an annotated declaration cannot use the decided state as specified.
An undecided read must check its initializer. An in-progress read must report a cycle.
For `countdown`, that check reads the in-progress binding, although its declared type is already known.
For the mixed pair, an eager check of `value` reads the in-progress `read`, although `value`'s annotation determines `read`'s result.

The form must carry a known annotated type independently of initializer-check progress.
The read transition must permit that type without a cycle rejection or another initializer check.
The handoff does not state this fact or this transition. Core principle 8 requires a stop before implementation.

The compiler tests, codegen tests, fmt, clippy, and API generator did not run after this stop.
Three-tier agreement remains unestablished. No golden changed. No commit was made.


## Round 3 status

Stopped at the handoff's form requirement. No production code changed.
The specified transition rejects a recursive inferred binding whose lambda has a complete annotated signature.
Stock `tsc` accepts that form. The lambda result annotation breaks the type dependency before the body check.

The round 3 corpus adds the annotated `countdown` and the annotated-breaks-cycle probe.
The `main` output also exercises `countdown` and `readLater`.
The annotated-breaks-cycle pair is not called because its calls do not terminate.

## Round 3 Red evidence

Contract pin: `e3210602`.
The round 1 and round 2 corpus files and tracking note were present at the start.
`cargo build --offline --locked -p subscript-cli` passed before this round changed any file.
The initial a329 check returned exit 1 with the same 18 S100 diagnostics recorded in round 2.
After the round 3 probes were added, the pin's CLI returned exit 1 with 19 S100 diagnostics:

| File | Position | Message |
|---|---|---|
| main.ts | 9:7 | module-level variables require a type annotation |
| main.ts | 10:5 | module-level variables require a type annotation |
| main.ts | 11:7 | module-level variables require a type annotation |
| main.ts | 12:7 | module-level variables require a type annotation |
| main.ts | 13:7 | module-level variables require a type annotation |
| main.ts | 14:7 | module-level variables require a type annotation |
| main.ts | 15:7 | module-level variables require a type annotation |
| main.ts | 16:7 | module-level variables require a type annotation |
| main.ts | 19:7 | module-level variables require a type annotation |
| main.ts | 23:5 | fields require a type annotation |
| main.ts | 24:5 | fields require a type annotation |
| main.ts | 25:5 | fields require a type annotation |
| main.ts | 26:12 | static fields require a type annotation |
| main.ts | 31:5 | fields require a type annotation |
| main.ts | 34:23 | parameters require a type annotation |
| main.ts | 36:22 | parameters require a type annotation |
| lib.ts | 7:14 | module-level variables require a type annotation |
| main.ts | 41:22 | a lambda with a block body requires a return type annotation |
| main.ts | 45:22 | a lambda with a block body requires a return type annotation |

Stock `tsc` accepted the updated entry with exit 0:

```sh
node_modules/.bin/tsc --strict --target ES2022 --module commonjs \
  --lib ES2022,ESNext.Disposable --outDir "$scratch" \
  prelude/lang.d.ts corpus/accept/a329-initializer-inference/main.ts \
  corpus/accept/a329-initializer-inference/lib.ts
```

Node ran the emitted module with `print = console.log` and a no-op `ValueType` decorator.
The driver called `main()` once. Its output supplied the new entry's golden:

```text
10,2,0.5,pool,3,6,7
4,c,1.5,8,3
5,0
pool
0,1
```

No existing accept golden changed.

## Round 3 missing declaration fact

These two probes passed stock `tsc` with exit 0:

```ts
const countdown = (n: i32): i32 => n === 0 ? 0 : countdown(n - 1);
export function main(): void { print(`${countdown(3)}`); }
```

```ts
const left = (): i32 => right();
const right = (): i32 => left();
export function main(): void {}
```

The commands used `--noEmit --strict --target ES2022 --module ESNext`, `--lib ES2022,ESNext.Disposable`, and the prelude.
The pin's CLI rejected the first probe with one module-annotation diagnostic and the second with two.
The pin's CLI and stock `tsc` also accepted separately annotated recursive and mutually recursive bindings with exit 0.

`signatures.rs::resolve_signatures` reads the binding's annotation, not the annotation of a lambda in its initializer.
`expr/lambda.rs::check_lambda_with` resolves the lambda parameter and result annotations before it checks the body.
`expr/literal.rs::check_ident` needs the enclosing global's function type at the recursive read.

The specified transition starts each unannotated binding as undecided and sets it in progress before the initializer check.
For the first probe, the local lambda path checks its body and reads the in-progress `countdown` binding.
For the second, the `left` body needs `right`; the `right` body then reads the in-progress `left` binding.
Rule 3 requires a `TscRejects` cycle diagnostic in both cases, although the measured programs are `tsc`-accepted.
These consequences follow from the specified transitions. No modified checker ran.

The form needs to carry the complete type of an annotated lambda separately from the unfinished body check.
Its read transition must permit that type during a recursive read of an unannotated binding.
The round 3 form grants this transition only to a binding with its own type annotation.
An initializer's lambda result annotation is a separate source fact.

The handoff requires a stop when a consumer needs a fact that the form does not carry.
This round stops before a new transition or a separate initializer-signature inference changes the contract.
The reject entries, production implementation, layout test, and rejection-site migration remain incomplete.
The compiler tests, codegen tests, fmt, clippy, and API generator did not run after this stop.
Three-tier agreement remains unestablished. No commit was made.


## Round 4 status

Contract pin: `0daa8357`.
The round 1–3 corpus files and tracking note were present at the start.
`cargo build --offline --locked -p subscript-cli` passed.
The round did not repeat the a329 Red measurement.
No production code changed.

The stop concerns a signature fact, not a new inference rule.
Rule 1 gives a defaulted parameter the type of its initializer.
Rule 3 permits a call result to use a declared result type before a body check.
The declaration form cannot represent that result separately from unfinished parameter types.

### Rule 3 measurements

Each probe ran with stock `tsc` 5.9.2:

```sh
node_modules/.bin/tsc --noEmit --strict --target ES2022 --module ESNext \
  --lib ES2022,ESNext.Disposable prelude/lang.d.ts "$scratch/probe.ts"
```

Each probe also declared an empty exported `main(): void`.

| Probe | Exit | Diagnostic |
|---|---|---|
| `const g = (n: i32): i32 => n === 0 ? 0 : g(n - 1);` | 0 | None |
| `const hs = [(n: i32): i32 => hs.length];` | 0 | None |
| `const left = (): i32 => right(); const right = (): i32 => left();` | 0 | None |
| `const f = (n: i32) => n === 0 ? 0 : f(n - 1);` | 2 | TS7023 |
| `class C { g = (n: i32) => this.g(n); }` | 2 | TS7024 |

These commands are measurements. The required Rust regression tests remain incomplete.

### Missing signature fact

The following program passed stock `tsc` with exit 0:

```ts
function first(n = second()): i32 { return n; }
function second(): i32 { return 1; }
export function main(): void { print(`${first()}`); }
```

The pin's CLI rejected it with one S100 diagnostic at `first`'s parameter:
`parameters require a type annotation`.
The version with `n: i32 = second()` passed the pin's CLI with exit 0.

A source-order change alone does not represent the necessary signature facts.
This second program also passed stock `tsc` with exit 0:

```ts
function first(n = second(1)): i32 { return n; }
function second(n = first(1)): i32 { return n; }
export function main(): void { print(`${first()}`); }
```

The pin's CLI rejected it with two parameter-annotation S100 diagnostics.
Stock `tsc` emitted CommonJS with `--strict --target ES2022 --module commonjs --lib ES2022,ESNext.Disposable` and the prelude.
Node called `main()` with `print = console.log` and printed `1`.
The fully annotated version also passed stock `tsc` and printed `1` under Node.
The pin's CLI aborted on that annotated control with a stack overflow and exit 134.
This abort occurred before any production change; its cause was not established.

`signatures.rs::resolve_fn_sig` resolves every parameter before it resolves the declared result type.
`resolve_signatures` inserts the `FnSig` only after that whole operation returns.
`expr/call.rs::check_direct_call_with_arguments` requires the stored `FnSig`; an absent signature produces `Type::Error` without a diagnostic.
`check_args_with_arguments` also reads every supplied argument's parameter type from that signature.

`FnSig` holds a resolved result and `Vec<ParamSig>`.
`ParamSig` holds only a concrete `ty`, its name, and `has_default`.
Neither form distinguishes an unfinished parameter type from a rejected parameter type.
The supplied module-binding and field states do not represent an unfinished function signature.

For the second probe, each parameter needs the other function's declared result before either complete signature exists.
A whole-signature in-progress state also rejects this dependency, although no result-type cycle exists.
A call-result consumer cannot read the initializer or return annotation to replace the missing stored fact (rule 5).
`Type::Error` cannot mark an unfinished parameter, because argument checks use it to suppress checks for rejected parameters.

The form needs independently stored declared results and arity, with a separate decision state for each inferred parameter type.
The local call path must read those facts while other parameter types remain undecided.
The checked default must also stay separate from its parameter type decision.

The handoff requires a stop when the form cannot carry a fact that a consumer needs.
This round stops before a separate call-result inference or a poison placeholder supplies the missing signature fact.
Rules 1–7, the reject entries, the layout test, and the rejection-site migration remain incomplete.
The requested compiler tests, codegen tests, fmt, clippy, and API generator did not run after this stop.
Three-tier agreement remains unestablished. No golden changed. No commit was made.

## Round 5 status

Contract pin: `100a9c51`.
The round 1–4 corpus files and tracking note were present at the start.
`cargo build --offline --locked -p subscript-cli` passed before any change.
The round did not repeat the a329 Red measurement.

### Default overflow cause

The annotated mutual-default program reproduced the pin's stack overflow with exit 134:

```ts
function first(n: i32 = second(1)): i32 { return n; }
function second(n: i32 = first(1)): i32 { return n; }
export function main(): void { print(`${first()}`); }
```

Stock `tsc` 5.9.2 accepted this program with exit 0.
The check used `--noEmit --strict --target ES2022 --module ESNext --lib ES2022,ESNext.Disposable` and `prelude/lang.d.ts`.

The defect is in `compiler/src/raise_sites.rs`, after body checks finish.
`Module::expression_can_raise` calls `call_defaults_can_raise` for a direct call.
That function scans the callee's default expressions, which call `expression_can_raise` again.
It scans every default, including defaults whose arguments the source call supplies.
The annotated program therefore follows `first → second → first → second` without a terminating fact.
The checker does not check a parameter default again at each call.
`bind_params` checks each default at its declaration.
The repeated operation is the later exception-effect scan.

A temporary trace at `call_defaults_can_raise` recorded the repeated names in that order.
A captured backtrace showed `expression_can_raise` nested under the default scan of another `call_defaults_can_raise`.
The trace process exited at a fixed visit count solely to capture evidence before the abort.
This limit was not a fix. The round restored the source byte-for-byte and rebuilt the CLI.
No production change remains.

### Missing default-effect fact

The required consumer is `Module::expression_can_raise`, which must read settled facts without recursion through callee defaults.
`hir::Param` stores the checked default expression but no settled default-exception fact.
`Function::can_raise` describes the function body, not the defaults that execute in the caller.
`function_body_can_raise` scans only `function.body`.
`call_defaults_can_raise` separately scans `function.params`.
Generator bodies and async bodies have exception boundaries that their caller-evaluated defaults do not share.
The body fact cannot replace the default fact without a change to its contract.
The new parameter type states represent type decisions; they do not represent this exception fact.

The smallest form change is one `default_can_raise` fact on each checked `hir::Param`.
It is false when the parameter has no default.
The exception pass must settle default facts and body facts together to a least fixed point.
A call must read the default facts only for omitted arguments, using the argument count already in its checked form.
The scan must read callee facts instead of recursively scanning callee expressions.
This form closes the recursive-default class without a depth limit.

The handoff requires a stop when a consumer needs a fact that the form cannot carry.
This round stops before it adds a default-effect fact or changes the meaning of the existing body fact.

### Corpus evidence

The a329 entry now contains both mutual-default programs, annotated and unannotated.
Stock `tsc` accepted the complete entry with exit 0:

```sh
node_modules/.bin/tsc --strict --target ES2022 --module commonjs \
  --lib ES2022,ESNext.Disposable --outDir "$scratch" \
  prelude/lang.d.ts corpus/accept/a329-initializer-inference/main.ts \
  corpus/accept/a329-initializer-inference/lib.ts
```

Node ran the emitted module with `print` and a no-op `ValueType` decorator.
It called `main()` once and supplied the complete a329 golden:

```text
10,2,0.5,pool,3,6,7
4,c,1.5,8,3
5,0
pool
0,1
1,1
```

Only the new, untracked a329 golden changed. No existing tracked golden changed.
The production implementation, reject entries, layout test, and rejection-site migration remain incomplete.
The requested compiler tests, codegen tests, fmt, clippy, and API generator did not run after this stop.
Three-tier agreement remains unestablished. No commit was made.


## Round 6: implementation

HEAD remains `100a9c51`. This round keeps the round 1–5 corpus and evidence.
The owner approved the stored type states and the separate default-exception fact.
The round does not repeat the Red measurement.

### Declaration and checked-expression facts

Module bindings, static fields, instance fields, and parameters carry `Undecided`, `InProgress`, `Decided(Type)`, or `Rejected`.
An annotation decides the type before initializer checks.
An inferred declaration uses the local initializer path and keeps its checked expression.
Rejected declarations provide `Error` poison to later reads without another diagnostic.
Function results enter the signature table before parameter defaults need them.

A type decision checks an annotated lambda signature without its body.
It checks an unannotated result body and postpones call arguments until parameter types settle.
Generic function and class bodies enter queues when a type decision requests an instance.
The queues finish after the decision pass. No call checks the callee body again.

Delayed checks need an expression identity, declaration context, and source statement owner.
`hir::Expr::pending_work` carries the unique checker work identity until the initializer completion pass consumes it.
A source position cannot carry this identity: `new Methods().echo<i32>(1)` gives its receiver and call the same start position.
The completion pass is the consumer. Every ordinary expression carries `None`.
The record also carries the source file, generic substitutions, parameter scope, lexical class, synthetic owner, and captures.
Stable lambda identities let completion propagate transitive captures to enclosing closures.
Synthetic owner kinds place delayed prefixes before source statements or in their original loop conditions and updates.

The existing local inference helper also applies the `null` restriction to these declarations.
Empty array inference retains the existing local rejection.
Block lambdas collect their own return types and exclude nested lambda returns.
Different return types use the C7 divergence site. An inferred value result with fallthrough uses a separate C7 site.

### Exception facts

Each checked `hir::Param` carries `default_can_raise`, initially false.
`raise_sites::decide_can_raise` settles body and default facts together to a least fixed point.
The consumers are call, construction, async call, and async handle exception-site checks.
A consumer reads defaults only after the supplied argument count.
No exception scan follows a callee default expression recursively.
Generator body boundaries remain separate from caller-evaluated defaults.
Tests cover supplied and omitted defaults for free functions, constructors, methods, async functions, and generators.

### Rejection sites and downstream checks

The module, static-field, instance-field, and block-lambda annotation requirements leave the rejection-site table.
Their witnesses now pass or exercise their remaining rejection sites.
The parameter annotation sites apply only without defaults and use `TscRejects`.
Cycle inference has its own `TscRejects` site.
Bare fields with no constructor use `TscRejects`; constructor-only field typing retains its declared divergence.
The apparent-type audit pins exact `Error` poison comparisons separately from value-shape operations.

The LIR call-operand test now visits a named default only when a call omits its argument.
A declaration-only default has no call operands in LIR, so requiring those operands produced a false failure for a329.
The test traverses each reachable default once. Production codegen is unchanged.
The layout test compares inferred and annotated `i32` and `f64` fields against the same platform C struct.

### Corpus measurements

Stock `tsc` 5.9.2 accepts the complete a329 entry with `--strict --target ES2022 --module commonjs`.
The command includes `--lib ES2022,ESNext.Disposable`, `prelude/lang.d.ts`, and both source modules.
Node calls `main()` with `print`.
The entry imports an empty generic `ValueType` decorator from its source module; the shared Node shim does not grow.
Its output agrees byte-for-byte with the retained a329 golden:

```text
10,2,0.5,pool,3,6,7
4,c,1.5,8,3
5,0
pool
0,1
1,1
```

With `--noEmit --strict --target ES2022 --module ESNext` and the same prelude and library, stock `tsc` rejects r351 with TS7023.
The diagnostic points to line 8. The same command accepts r352 with exit 0.


### Final validation

- `cargo test --offline --locked -p subscript-compiler`: passed, including all 15 initializer tests and the §154 total test.
- `cargo test --offline --locked -p subscript-codegen`: passed, including the layout test and the corpus differential tests.
- a329 agrees with its golden under the interpreter, dev JIT, ship C AOT, and Node.
- `cargo fmt --check`: passed.
- `cargo clippy --offline --locked --workspace --all-targets`: passed. Library warning counts are compiler 5, runtime 18, and codegen 13.
- The `tools/gate.sh` library warning baseline is 7, 18, and 13. No count exceeds its baseline.
- `cargo run --offline --locked -p subscript-compiler --bin generate-api-reference`: passed.
- `tools/hygiene.sh` and `git diff --check`: passed.

No existing `.expected` golden changes. All changed Rust files remain below 2,000 lines.
The generator changes only `generated-docs/corpus-index.md`.
No contract rule requires another owner decision. No commit is made.


## Round 7: handoff findings

The round keeps the round 1–6 implementation and corpus.
No contract rule changes. No commit is made.

### Checked slots and body queues

C1: only supplied arguments with undecided parameter types need delayed checks.
Builtin signatures have fixed parameter types, so their arguments never enter the delayed argument check.
The checked call keeps one slot per supplied argument.
The completion pass replaces those slots; it never extends the argument list.
Fixed argument slots keep their completed child forms when the remaining argument checks finish.
N4: a union callee checks each source argument once and shares that checked argument across its member contracts.
The callback-body test observes one diagnostic from its delayed body.

M1: the checker drains generic class and function body queues after pass C, until both queues are empty.
Instances requested from an inferred generic field or a generic default have checked bodies before lowering.

### Declaration reads and block results

M2: the checker distinguishes an in-progress read in an inferred block result from a read outside its return expressions.
An outside read uses C24 row 4 and the remedy "annotate the result".
A read in a return expression retains the cycle site and its TypeScript rejection class.
Stock TypeScript accepts the four outside-read probes: free recursion, static recursion, a nested lambda, and a nested function-value read.
The checker reports one C24 diagnostic for each probe.
A transitive declaration decision keeps its own return context; the additional nested-dependency probe also gives one C24 diagnostic.

M3: the parameter decision carries its declaration scope and name.
A read resolves the scope before it compares that declaration identity.
Nested parameters and locals with the same name do not read the parameter under decision.

M4: an inferred block frame retains the source expressions of its literal returns until its result type is known.
The checker selects the first nonliteral, non-null result; it checks the other returns against that type.
If every return is a literal, the first literal supplies its C4 type.
Each literal uses the ordinary expression checker once, with the selected context.
This also preserves negative literals, large integers adopted by floats, and nonliteral constants that fold to literals.
A reference result with a null return becomes nullable.
The remaining C7 mismatch message states that a return does not fit the inferred result type.

N1: an awaited read of a rejected module binding retains poison and reports no callee diagnostic.
N2: the checker sorts its final diagnostics by initializer module order, line, and column.
Tests that require a particular rejection select its source position instead of its former emission order.

### Default functions and default facts

M5: a lowered default has one cached function identity, a receiver, and the preceding parameter values.
The lowering publishes the identity before it lowers the body, so a recursive default calls an existing identity.
Call sites call that function instead of inlining the default expression.
`CallParam` carries the checked parameter's default exception fact.
Each helper call has the required call, lifetime, and exception sites.
The interpreter, dev JIT, and ship C consume the same function and call forms.
The dev declaration reads the receiver from the LIR parameters, including receivers on internal helpers.
The LIR test checks shared identities and the receiver plus preceding arguments without a native compile.

M6: the C14 scan stores body summaries and indexed default summaries separately.
A call requests a default summary only for an omitted argument.
A supplied argument does not add the default's initializer reads.
The summary scan retains its finite call graph; it does not expand recursive default expressions.

N3: the all-structs layout test includes S156Fields in its existing C probe.
It compares inferred and annotated fields against that same result and runs the C probe once.
The initializer test states its measured cost in its module comment.
The direct test-binary measurement excludes Cargo compilation and lock waits.

The LIR fact test previously expanded defaults at each call and overflowed on the recursive-default corpus probe.
It now counts helper-call sites separately and traverses each default body once.
The HIR facts supply the expected receiver, preceding arguments, and default exception sites.
The corrected test compares every accept entry against its independently lowered LIR.

### Corpus and retained open items

The expanded a329 includes all six builtin operations at each inferred declaration position.
It includes both generic-instance probes, both recursive-default forms, the recursive lambda default, and the receiver default.
It also includes the scope-shadowing probes, supplied-default initializer effects, and contextual block returns.
Stock TypeScript accepts the entry with strict checking, ES2022, and the repository prelude.
Node supplies the regenerated a329 golden, including `22,22,0,3` for the recursive and receiver defaults.
No existing expected golden changes.

The round records §156.3 items 1 and 2 without a fix:
- An annotated block lambda with `while (true) { return x; }` fails lowering with a reachable-fallthrough message.
- A function value with a default still rejects an omitted argument with the false required-count message.

### Round 7 validation

- `cargo test --offline --locked -p subscript-compiler`: passed, including the 25 initializer tests and the rejection-site total test.
- `cargo test --offline --locked -p subscript-codegen`: passed, including the interpreter, JIT/C golden comparison, layout, and LIR fact tests.
- a329 matches the Node golden on all three tiers. No existing expected or LIR golden changes.
- `cargo fmt --check`: passed.
- `cargo clippy --offline --locked --workspace --all-targets`: passed with no new warning.
- Library warning counts remain compiler 5, runtime 18, and codegen 13.
- `cargo run --offline --locked -p subscript-compiler --bin generate-api-reference`: passed; only the corpus index changes.
- Every changed Rust file stays below 2,000 lines. The largest file has 1,983 lines.

An earlier codegen run stops when the native-library test process receives SIGKILL before its test output.
The final complete codegen run passes, including that test. No golden changes resolve a test failure.

`tools/hygiene.sh` and `git diff --check` pass after the round 7 changes.


## Round 8 fixes

Parameter decisions reserve a name without a poison local. Each parameter enters the scope once, with its decided type.
Each decision publishes its parameter state to the stored signature before another decision reads it.
Static methods publish the same state to their callable signature.
Deferred frames receive the completed parameter scope and remove its completed name reservations.
Constructor decisions keep the stored signature readable.

A lambda decision uses the declaration's `this` rules. It does not inherit the enclosing receiver.
The field-initializer check rejects `this` inside a lambda during the decision and during the ordinary body check.
The four receiver probes compare their diagnostics with the annotated forms.

A direct call defers arguments only when its AST span is the whole initializer.
The signature must have no source type parameters. The signature carries this fact in `FnSig::generic`.
This fact distinguishes a generic function from a non-generic method on a generic class.
Every other call checks supplied arguments during the decision.
A supplied argument that targets an in-progress parameter reports the rule 3 cycle.
The tests cover the measured direct, conditional, array, binary, omitted-argument, and paired-call forms.
They also cover instance methods, static methods, generic calls, constructors, and reads of earlier decided signature parameters.

A function-value signature dependency carries its own C24 row 4 divergence and the remedy `annotate the parameter`.
A recursive block lambda carries a separate divergence with a lambda fragment and the remedy `annotate the result`.
A block-return mismatch points at the incompatible return expression.

Async default-function calls receive the method receiver.
The generator call path already passes the receiver through ordinary method call resolution.
Instance generator declarations remain outside the decided surface. The test uses an accepted static generator method.
The new receiver tests check two shared async defaults and two shared generator defaults.
Measured cost: two checker/LIR tests, 0.01 s wall time, with no native compile.
The initializer tests contain 31 checker/HIR tests. Their focused round 8 run takes 0.03 s wall time.

### Remaining decision-state branches

Every remaining `deciding_type` branch controls check order. None changes a declaration rule.

- `initializer.rs`: the direct-call predicate applies rule 3. Initializer entry and completion save and restore the decision state.
- `initializer.rs`: a decision transfers pending slots; the completion pass checks them once in their stored frames.
- `expr/lambda.rs`: a lambda with an annotated result defers its body under rule 3.
- `generics.rs`: function and method instances queue their bodies during a decision, then check each body in its stored context.
- `generics.rs`: class instances queue their bodies until signatures are ready and the decision ends.

Generic parameter decisions run during instantiation on both paths. No decision-state branch skips their declaration rules.

### Measurements and corpus

Stock TypeScript 5.9.2 accepts the direct calls, omitted-argument calls, paired calls, and both recursive function-value probes.
It rejects the conditional, array, and binary supplied-argument forms with TS7022.
It also rejects supplied-argument cycles in the conditional instance method, generic function, and constructor with TS7022.
It rejects the incompatible `string` return from the dependent numeric default with TS2322.
All probes use strict checking, ES2022, and the repository prelude.

The a329 entry includes the dependent parameter, lambda, constructor, method, and recursive-constructor probes.
It also includes the accepted direct calls and the async receiver probe.
Node 24.18.0 supplies its regenerated output golden. The final four lines are:

```text
4,2,3,2,6
true,3,3,1,1,3
16
16
```

The new coroutine forms put a329 in the existing coroutine snapshot selection.
That selection has no a329 text record. The dedicated default-function assertions and the full HIR/LIR fact gate cover its form.
The interpreter and native tier gates compare its output with the Node golden.
The snapshot test excludes this new entry, as it excludes other entries with dedicated form assertions.
The existing LIR golden remains byte-identical. The focused snapshot test passes.

### Restored reject assertions

`corpus_reject.rs` restores every existing assertion from HEAD. Only the r351 and r352 table entries differ from HEAD.
The complete compiler command now reports three failures:

- `every_reject_entry_fails_with_its_rule_code_at_the_offending_line`: r69 reports the stack-frame diagnostic at 12:9 first; the test expects line 13.
- `frame_and_synthesized_aggregate_rejections_are_checker_diagnostics`: r69 reports 12:9 first; the test expects the closure diagnostic at 13:27.
- `r31_explicit_symbol_dispose_call_uses_s016`: the first code is S100; the test expects S016.

Rule 12 sorts the diagnostics by source position. The round retains these first-diagnostic assertions and reports the failures as requested.
It does not change an expected diagnostic or select a later diagnostic to pass a test.


### Round 8 command results

- The complete compiler command fails on the three restored first-diagnostic assertions listed above.
- `cargo test --offline --locked -p subscript-codegen` passes in full, including the Node-output comparison on all three tiers.
- The existing LIR text golden passes without a change. No existing output golden changes.
- The compiler command with only those three tests excluded passes, including the rejection-site total test, corpus gates, and doc tests.
- `cargo fmt --check` passes.
- `cargo clippy --offline --locked --workspace --all-targets` passes. Library warning counts remain compiler 5, runtime 18, and codegen 13.
- `cargo run --offline --locked -p subscript-compiler --bin generate-api-reference` passes. Only the generated corpus index differs from HEAD.
- `tools/hygiene.sh` and `git diff --check` pass.
- All 69 changed or new files are within the authorized file scope. The largest changed Rust file has 1,987 lines.

The round does not commit and does not run `tools/gate.sh`.

The all-pass command condition remains unmet because the complete compiler command reports the three listed failures.

## Round 9: declaration diagnostic order

Rule 12 now uses a stable sort by §137 module order and the start of the containing top-level declaration.
The sort preserves emission order inside each declaration.
Deferred expression diagnostics use their source slot positions to select the containing declaration.
The module-order test checks a deferred lambda in an imported module and two declarations on one source line.
The two r69 assertions and `r31_explicit_symbol_dispose_call_uses_s016` pass unchanged.
The complete `corpus_reject` target passes all 45 tests.

### Coroutine snapshot selection

Round 8 added the exact exclusion `if id == "a329-initializer-inference" { continue; }`.
That exclusion did not exist before round 8. Round 9 removes it and adds the a329 snapshot record.
The addition contains 1,739 lines and 192,657 bytes. Every existing snapshot record remains byte-identical.

The existing selection includes an entry if any lowered function is a generator or async function.
It also includes these explicit measurement entries:
`a145-emitted-identifiers`, `a147-switch-body-scope`, `a148-switch-using-scope`, `a149-suspension-state`,
`a150-receiver-address-invalidation`, `a151-lambda-env-outlives-block`, and `a171-static-array-callbacks`.
All other entries without a coroutine function stay outside this text snapshot.

The existing exclusions run before that selection:

| Entry | Existing selection reason |
|---|---|
| a161-counted-handle-stores | Dedicated ownership, verifier, interpreter, and tier-differential assertions. |
| a162-async-copy-sites | Dedicated ownership, verifier, interpreter, and tier-differential assertions. |
| a166-resume-parameter-interference | Dedicated ownership, verifier, interpreter, and tier-differential assertions. |
| a180-for-of-generator-only | Dedicated ownership, verifier, interpreter, and tier-differential assertions. |
| a181-operation-in-every-owner | Dedicated ownership, verifier, interpreter, and tier-differential assertions. |
| a276-generator-result-narrowing | Dedicated origin assertion under compiler.md §124. |

Round 9 changes none of those exclusions or their stated reasons.

The first complete compiler run found one more order regression in `each_mirror_scope_kind_yields_to_the_module_scope`.
`resolve_fn_sig` resolved the result before the parameters. Round 9 restores the parameter-first resolution order.
The expected diagnostic remains unchanged. This keeps the original signature emission order without a position sort inside the declaration.

### Round 9 checks

- `cargo test --offline --locked -p subscript-compiler` passes: 983 tests across 55 test groups, including doc tests.
- The initializer test target passes 32 tests in 0.02 seconds, with no native compilation.
- `cargo fmt --check` passes.
- `cargo clippy --offline --locked --workspace --all-targets` passes. Library warning counts remain compiler 5, runtime 18, and codegen 13.
- `cargo run --offline --locked -p subscript-compiler --bin generate-api-reference` passes. Only the generated corpus index differs from HEAD.

The round changes no expected diagnostic and no existing output golden.
It does not commit and does not run `tools/gate.sh`.

- `cargo test --offline --locked -p subscript-codegen` passes after the signature-order fix: 725 tests across 51 test groups.
- The full codegen run checks the a329 Node output golden on all three tiers and includes its coroutine text snapshot.
- `tools/hygiene.sh` and `git diff --check` pass.
- All 69 changed or new repository files stay within the authorized scope. The largest changed Rust file has 1,987 lines.


## Round 10: recursive block returns and contextual return checks

### Parameter diagnostic classes

`GenericCallbackParameterAnnotationMissing` reports C24 row 4 through its own divergence entry and witness.
The checker selects this site when a generic callee parameter supplies the callback context.
The named and pattern sites retain `TscRejects` for parameters with no annotation and no such context.
Stock TypeScript 5.9.2 accepts the `each([1, 2], (x) => {})`, `apply(1, (x) => x + 1)`, and destructured callback probes.
The unit test checks both generic callback forms and the ordinary missing-annotation class.
It also checks explicit type arguments on a function, an instance method, and a static method.
The argument checker reads the callee signature's generic fact for those calls. Stock TypeScript accepts all three probes.

### Block result form

Each inferred lambda frame carries its recursive declaration identity and deferred return slots.
A slot holds the source expression, lexical scopes, and narrowing facts at that return.
The first non-literal, non-null, non-recursive return sets the result type.
The checker checks each remaining return once with that result as its context.
A synthetic prefix stays with its return statement. Capture and async-origin facts reach the lambda frame.
The callback-return test checks consecutive lambda identities to detect a discarded second check.
The scope test checks returns in two different blocks.

A bare self call does not set the result. The checker accepts the module, parenthesized, static-field, default, and local forms.
For module and field self calls, the return check reads the provisional signature and then restores the declaration state.
For a local or default self call, `hir::Callee::SelfLambda` names the current lambda without a mutable capture.
LIR lowering reads that identity from the lambda-function map and uses the existing static closure call convention.
The closure receives the current const captures. All three tiers print `7` in the local-capture test.
A malformed self-call identity fails lowering before a backend reads it.

If every return is a bare self call, the checker reports C24 row 4 with the remedy "annotate the result".
The old unit assertion now checks that class. Stock TypeScript accepts this probe.
The product return `n * fact(n - 1)` retains the cycle site; stock TypeScript reports TS7023.
The `pick` probe checks its empty-array return with the inferred `i32[]` context.
Node prints `0,0,0` for the three recursive accept probes and `2,0` for `pick`.
The a329 output golden contains those lines. Only the a329 LIR snapshot record changes in this round.

### Default read routes

The initializer-effect default table carries a label with the function and parameter names.
The C14 diagnostic now reports `g` followed by `f (default of x)`.
The supplied-default exception remains unchanged. The route test forbids the old `[default]` label.

### Check cost and open cycle instances

A lambda with no parameter initializer skips the unused `FnCtx` clone in `decide_lambda_parameters`.
The deferred-work frame clone remains because a deferred unit needs its declaration context.
The measured debug program contains 2,250 integer globals and 2,250 block-lambda globals, plus an empty entry function.
For each index `i`, its declarations are `const valueI = I;` and `const lambdaI = (n: i32) => { return n + valueI; };`.
One isolated run takes 22.702 s before the clone change and 22.363 s after it.
No build or test overlaps that timing pair. The measurement does not establish a change in scaling.
The earlier non-isolated pair takes 22.132 s and 27.501 s; the second run overlaps build and test work.
The round makes no speed claim from that pair. The contract's check-time item remains open.

The round measures these §156.3 instances again with stock TypeScript 5.9.2 and the current checker:

| Default expression and callee context | tsc | Checker |
|---|---|---|
| `k = f(n - 1, 0) as f64` | Accepts | S100 cycle, no divergence block. |
| `k = h(f2(n - 1, 0))`, with `h: (n: i32) => i32` | Accepts | S100 cycle, no divergence block. |
| `k = g(f(n - 1, 0))`, with inferred lambda `g` | Accepts | S100 cycle, no divergence block. |

The round records these instances without a fix, as the handoff requires.
The inferred initializer tests cost 0.02 s for 36 checker/HIR tests, with no native compile.
The parameter-default tests cost 0.42 s for three checker/LIR tests and one tier test, with one native compile.


### Round 10 command results

- `cargo test --offline --locked -p subscript-compiler` passes: 987 tests across 55 test groups.
- `cargo test --offline --locked -p subscript-codegen` passes: 727 tests across 51 test groups.
- The full codegen command checks the a329 golden on all three tiers and checks its LIR snapshot.
- `cargo fmt --check` passes.
- `cargo clippy --offline --locked --workspace --all-targets` passes with no new warning.
- Library warning counts remain compiler 5, runtime 18, and codegen 13.
- `cargo run --offline --locked -p subscript-compiler --bin generate-api-reference` passes.
- `tools/hygiene.sh` and `git diff --check` pass.
- All 71 changed or new files stay within the authorized scope. The largest changed Rust file has 1,991 lines.

The round preserves the earlier working tree. It changes no existing output golden.
It does not commit and does not run `tools/gate.sh`.

## Round 11: owner scope cut and generic parameter checks

The owner removed block-result inference from §156 rule 6.
This round supersedes the block-result forms and acceptance claims in rounds 7–10.
An unannotated block with no contextual result reports the original `BlockLambdaReturnAnnotationMissing` site, message, and C24 row 4 divergence.
The checker rejects the block before its body can request an inferred result.
An annotated block, an expression lambda, and the pin's contextual result checks keep their behavior.

The round removes these forms and consumers:

- `check/expr/block_result.rs`: return selection, recursive-call exclusion, deferred return slots, contextual return completion, and provisional signatures.
- The frame's inferred-result flag, recursive binding, and deferred-return table.
- The checker's block-result decision state and initializer recursive-binding state.
- `hir::Callee::SelfLambda`, its LIR closure dispatch, and the lambda-function identity map used by that dispatch.
- The self-call branches in effect scans, raise-site scans, operation signatures, and LIR fact checks.
- `BlockLambdaReturnTypesDiffer`, `BlockLambdaResultAnnotationNeeded`, and `LambdaInferredResultFallsThrough`, including their witnesses.
- The block-result inference tests and the corresponding a329 declarations, calls, and output lines.

The six handoff probes report one S100 diagnostic each, with the original block-result annotation message and divergence.
They cover callback returns, static recursion, a missing return value, narrowed nullable returns, and local and module mutable recursion.
Tests that check deferred captures or parameter scope use annotated blocks.
They retain their original non-result-inference assertions.

### Generic-class defaults

A method default decision compares the inferred declared type against the class's opaque parameter identities.
The check also reads contained types and generic-class arguments.
It reports `GenericClassDefaultParameterAnnotationNeeded`, S100, C24 row 4, with the remedy `annotate the parameter`.
The parameter-form shape allowance covers this identity check; constraint projection loses the identity that rule 6 needs.
The existing opaque-template check reports this rejection before concrete instantiation can accept a supplied argument.
Fields and defaults of generic functions remain outside this method restriction.
The accepted controls include `b = this.a`, `n = this.a.length`, an annotated method default, and `other = k` in a generic function.

The r352 entry now rejects `Cell<T>.pair(other = this.v)`.
Stock TypeScript 5.9.2 accepts both the Cell and array-default probes, and the accepted controls.
The bare generic parameter probe `g((x) => 1)` reports TS7006 under stock TypeScript 5.9.2.
The checker gives that probe the ordinary missing-parameter annotation site, with `TscRejects`.
The generic callback context flag applies only to function-typed callee parameters, on inferred and explicit call paths.

### Corpus and retained open items

Node 24.18.0 supplies the regenerated a329 output golden through the repository Node runner.
The lowering supplies its regenerated LIR record: 1,583 lines, reduced from 1,879.
A record-by-record comparison confirms that every other LIR record stays byte-identical.
No existing output golden changes.
The corpus index comes from `generate-api-reference`.

The round retains the owner-recorded §156.3 literal-type class without a fix.
The checker widens module constants and readonly fields; stock TypeScript retains their literal types.
The round retains the owner-recorded debug stack-depth limit without a fix: about 130 unannotated forward references.
These are the contract's existing measurements. This round does not repeat or extend them.
The earlier annotated-block fallthrough and function-value default limitations also remain open.

### Round 11 checks

- `cargo test --offline --locked -p subscript-compiler` passes: 984 tests across 55 test groups.
- The initializer target passes 33 checker/HIR tests in 0.02 seconds, with no native compile.
- The total rejection-site test passes, including the restored block site and the generic-class default site.
- `cargo test --offline --locked -p subscript-codegen` passes: 725 tests across 51 test groups.
- The codegen gates compare a329 against its Node golden on the interpreter, dev JIT, and ship C tiers.
- The complete LIR snapshot gate passes. Only the a329 record changes.
- `cargo fmt --check` passes.
- `cargo clippy --offline --locked --workspace --all-targets` passes with no new warning.
- Library warning counts remain compiler 5, runtime 18, and codegen 13.
- `cargo run --offline --locked -p subscript-compiler --bin generate-api-reference` passes.
- All 69 changed or new working-tree files stay within the authorized scope.
- The largest changed Rust file has 1,987 lines.

The round preserves the earlier work except for the authorized block-result scope cut.
`hir/effects.rs` returns to HEAD after its only added SelfLambda branch is removed.
The untracked block-result module and former r352 file are removed.
The round does not commit and does not run `tools/gate.sh`.

`tools/hygiene.sh` and `git diff --check` pass after the round 11 changes.

## Round 12: generic argument context and decision ownership

### Generic argument context

Every parameter of a generic callee supplies the rule 6 context fact, including a bare type parameter.
The inferred call path reads the generic template. The explicit call path reads the resolved callee signature.
The checker retains contextual typing when the signature supplies a concrete function type.
The named and pattern annotation rejections retain their separate generic-context divergence.

Stock TypeScript 5.9.2 accepts the `firstOr(handlers, (x) => x + 1)` and `pair` probes.
It reports TS7006 for `g<T>(v: T)` called with `g((x) => 1)`.
All three checker rejections now carry C24 row 4. The last case retains the contract's reverse cost.

### Function-value decisions

The decision form carries the current initializer root span.
A direct function-value read and its direct alias defaults retain the signature-dependency fact.
A nested array, member, or other initializer establishes its own decision context.
The `onA` default and `handlers = [onA, onB]` report the rule 3 cycle without a divergence.
Stock TypeScript reports TS7022 for both `count` and `handlers`.

The `k = f` divergence no longer claims that a parameter annotation resolves its recursive function type.
Its remedy removes the recursive function value from the default.
The executable remedy witness uses `k = 0`; both the checker and stock TypeScript accept it.
The direct alias `const h = f` with `k = h` retains the same signature-dependency divergence.

### Rejected block bodies

An unannotated block with no contextual result reports the existing rule 6 rejection, then checks its body with a poisoned result.
The local mutable-capture probe `noret` now reports S009 after the block-result rejection.
A return expression adds no result diagnostic under that poisoned lambda frame.
Tests cover both a body with no return and a body that returns an unknown identifier.
The deferred lambda form carries a result type directly. The block path no longer resets the frame through an optional result.
The lambda closure body now uses canonical rustfmt layout.

### Signature and default identities

Argument deferral and generic callback context read the receiver's resolved method signature.
They no longer search unrelated classes by method name.
Tests cover the same method name on two classes, generic and ordinary signatures, direct default calls, and static method owners.
A symbol-to-owner map records each static method once at signature creation and generic instantiation.
Parameter decisions use that map without formatting each class member on each call.

The default-function table uses the resolved function or method identity and the parameter index.
It no longer compares whole checked expressions for each omitted argument.
The test checks two generic function instances with identical default expressions and one constructor.
Each declaration owns one helper, and repeated calls share that declaration's helper.
Existing receiver, recursive-default, async, and generator tests retain their helper-count checks.

### Check-time measurement

The generated debug program contains 4,500 module declarations and one empty entry function.
For each index from 0 through 2,249, it declares one integer global and one unannotated block-lambda global.
Each block returns its annotated `i32` parameter plus its corresponding integer global.
The CLI checks the same source before and after this round. Each run reports 2,250 rule 6 rejections and exits 1.

| Run | Wall time | User time | System time |
|---|---:|---:|---:|
| Before round 12 | 24.80 s | 23.92 s | 0.05 s |
| After round 12 | 24.62 s | 23.83 s | 0.02 s |

No build or test overlaps either measurement. This single pair establishes no scaling change or speed improvement.
The contract's check-time item remains open.

### Retained §156.3 items

Stock TypeScript accepts `const handlers = [() => handlers.length]`, `const f = () => f`, and the `idle`/`running` pair.
The checker still reports the rule 3 cycle without a divergence for all three probes.
The new test records these results without a fix.
The earlier cast, function-value call, and inferred-lambda call cycle instances remain open.
The literal-type difference, debug forward-reference stack limit, default-before-required case, annotated fallthrough, and function-value default arity remain open.
This round does not fix any §156.3 item.

### Round 12 command results

- `cargo test --offline --locked -p subscript-compiler` passes: 989 tests across 55 groups, including doc tests.
- The initializer target passes 38 checker/HIR tests in 0.02 seconds, with no native compile.
- `cargo test --offline --locked -p subscript-codegen` passes: 726 tests across 51 groups.
- The parameter-default target passes three checker/LIR tests in 0.01 seconds, with no native compile.
- The codegen gates compare a329 with its Node golden on the interpreter, dev JIT, and ship C tiers.
- The LIR snapshot gate passes. This round changes no output golden or LIR snapshot.
- `cargo fmt --check` passes.
- `cargo clippy --offline --locked --workspace --all-targets` passes with no new warning.
- Library warning counts remain compiler 5, runtime 18, and codegen 13.
- `cargo run --offline --locked -p subscript-compiler --bin generate-api-reference` passes.
- All 70 changed or new working-tree files stay within the authorized scope.
- The largest changed Rust file has 1,987 lines.

The round preserves the earlier working tree. It does not commit and does not run `tools/gate.sh`.

`tools/hygiene.sh` and `git diff --check` pass.

## Phase Review

| Pass | CRITICAL | MAJOR | MINOR | Result |
|---|---:|---:|---:|---|
| 1 | 1 | 5 | 4 | Fixed in round 7: deferred builtin arguments, pass C generic instances, block-lambda self reads, scope-first names, return unification, default lowering (rule 9) |
| 2 | 0 | 5 | 5 | Fixed in round 8: parameter-scope types, decision context, constructor signature, the measured tsc argument rule, async-method receivers |
| 3 | 0 | 3 | 3 | Fixed in round 10: generic-callback split, direct self-call returns, contextual returns; the cycle residual recorded in §156.3 |
| 4 | 0 | 6 | 4 | Owner decision 2026-10-04: block-lambda result inference removed (rule 6); literal types recorded in §156.3; round 11 removed the code and added the generic-class default rule |
| 5 | 0 | 1 | 7 | Fixed in round 12 (generic-callback split for every generic callee) and round 13 (C24 row 4 why text); residuals recorded in §156.3 |

Round 8 also restored the first-diagnostic reject assertions; round 9
changed rule 12 to declaration order after they failed.

Final gate: `gate full 15fdbd27 dirty:69 debug 2312/0/3 release 2309/0/3 skips 2/0 clippy 5/18/13 goldens-moved 1 exit 0`.
The moved golden is `codegen/tests/lir-goldens/corpus.txt`: the `a329`
record only, with no line of an existing record changed.

Status: COMPLETE.
