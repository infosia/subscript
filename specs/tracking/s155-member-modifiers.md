# §155 member modifiers — round 1

## Red at the contract pin

HEAD: `e6690580`, amended to `aabf66f1`; the amendment changes only the contract text, so the pin results hold. The working tree was clean before the corpus additions.
`cargo build --offline --locked -p subscript-cli` built the pin before any implementation change.
Each result below came from that binary with `subscript check`.

| Entry | Pin result | Measured TypeScript 5.9.2 result |
|---|---|---|
| r345-member-access | No diagnostic | TS2341 |
| r346-constructor-access | No diagnostic | TS2673 |
| r347-abstract-construction | No diagnostic | TS2511 |
| r348-readonly-write | No diagnostic | TS2540 |
| r349-optional-function-type | S100 at line 8, column 54: call expects two arguments | Accepts |

The TypeScript measurement used `node_modules/.bin/tsc`, strict mode, ES2022, and `prelude/lang.d.ts`.

## Implementation

The checker class shape stores accessibility, constructor accessibility, abstract status, and readonly instance fields.
Member reads, method calls, awaited methods, assignment places, and construction consume these facts.
The checker does not read the class AST at a use site.

The body context stores its lexical class.
Arrows retain that context, including nested arrows.
Generic instances share their source declaration position, so access uses the declaring class identity.
Constructor writes also require the direct body, a single function frame, and the `this` receiver.
TypeScript accepts direct `this` readonly writes in constructor parameter defaults.
The direct constructor frame also covers those defaults.

Getter and setter accessibility have separate facts.
A public getter with a private setter accepts a read and rejects a write.
The TypeScript measurement rejected only the write with TS2341.

Five new rejection sites classify the four modifier restrictions as `TscRejects`.
The optional function-type parameter site uses S012 and the existing C7 `OptionalParameter` divergence.
Its error type prevents a second diagnostic at a call or argument check.
The shared TypeScript fragment now includes both declaration and function-type parameters.

The total witness table includes every rejected form from the Problem section and the accepted corpus as a control.
Nested function declarations already fail at `LocalFunctionDeclaration` before their bodies reach the checker.
The nested-function readonly witness retains that existing site and measures TS2540.
Its lexical-access control measures TypeScript acceptance and retains the existing declaration divergence.
This round does not add nested function declarations to the language surface.

## Corpus and checks

The new reject entries report S100 for r345–r348 and S012 at the declaration for r349.
The r349 entry reports exactly one diagnostic.

`a328-member-modifiers` covers private and protected access through another receiver, arrows, static methods, and constructors.
It also covers direct readonly constructor writes, with and without a field initializer, and abstract class static access.
Node v24.18.0 produced its new golden with TypeScript 5.9.2.
The interpreter, dev JIT, and ship C AOT matched these bytes:

```text
10
10
15
3,3
7
8
```

Ten checker unit tests cover the restrictions and their controls.
The golden corpus tests compare all three tiers to the new Node-derived golden.
No existing `.expected` file changed.

`cargo test --offline --locked -p subscript-compiler` passed.
`cargo fmt --check` passed.
`cargo clippy --offline --locked --workspace --all-targets` passed.
Its library warning counts were compiler 5, runtime 18, and codegen 13.
The `tools/gate.sh` baseline is 7, 18, and 13. No new warning came from this change.
`cargo run --offline --locked -p subscript-compiler --bin generate-api-reference` regenerated the corpus index.
`tools/hygiene.sh` passed. The gate script did not run.

`cargo test --offline --locked -p subscript-codegen` passed.
The final total test measured 1,561 witnesses and 468 variants.
Its costs were TypeScript 0.592 s, checker 0.509 s, and total 1.391 s.
The ten-test checker batch took 0.01 s. The separate three-tier test took 0.56 s before its round 2 removal.
All changed Rust files stay below 2,000 lines.
All changes stay inside the handoff file scope. No commit was made.

## Round 2 — review fixes

The separate `codegen/tests/member_modifiers.rs` test repeated the golden corpus checks for `a328` across all three tiers.
This round removes it under core principle 15. The golden corpus tests retain that coverage.

The expected-error headers for r345–r349 now name each rejected form.
The abstract construction diagnostic now states that the named class is abstract and rejects `new` of that class.
The checker unit test and both rejection witness targets require the new message.

`cargo test --offline --locked -p subscript-compiler` passed.
`cargo test --offline --locked -p subscript-codegen` passed, including the golden corpus checks.
`cargo fmt --check` passed after the diagnostic assertion format fix.
`cargo run --offline --locked -p subscript-compiler --bin generate-api-reference` passed.
`git diff --check` passed. The gate script did not run. No commit was made.

## Round 3 — review fixes

The contract pin is `aabf66f1`.
A separate archive build used `cargo build --offline --locked -p subscript-cli` before the rule 6 fix.
Its binary accepted `r350-value-type-constructor` with no diagnostic.
TypeScript 5.9.2 rejected that entry with TS1238 and TS1270 at line 8.
The corpus header names TS1238.

A call through a field with `Type::Error` now retains the declaration diagnostic without a second method diagnostic.
The unit test and total witnesses cover an uninitialized field and an initialized field.
Their mutable function-type controls accept a call with both arguments.

`ValueTypeRestrictedConstructor` checks the constructor accessibility at the decorator.
It reports S100, carries `TscRejects`, and names the class and the modifier.
The total witnesses cover private and protected constructors with bare decorators and alignment options.
Each witness has a public constructor control.

Assignment places use the checked receiver and check the member modifiers once.
A direct receiver fact distinguishes `this` from `(this)`.
The latter gives the readonly rejection in a constructor.
A static `this.x` write retains `InstanceMemberInStaticMethod` and has no readonly diagnostic.
A readonly target in a constructor arrow uses the enclosing frame receiver type for the receiver check.
The modifier check retains the arrow frame and rejects the write.

Readonly array, object, and default assignment targets now give the readonly site before the destructuring assignment site.
A readonly `??=` target gives the readonly site before the assignment operator site.
These diagnostics carry no divergence block.
Mutable targets retain their existing block sites.
A direct constructor target retains the operator site for an unsupported operator.

Class-body access compares ClassIds or the template keys from `instance_arguments`.
It does not compare declaration positions.
`rejection.rs` has 1,986 lines, so this round needs no topic split.
All new helper items share the scope of their consumers.

The §155.3 item 1 remains open.
The checker accepts a compound assignment through a private getter and a public setter.
TypeScript 5.9.2 reports TS2808 at both accessors in that declaration.
This round records that result and does not fix it.

The compiler suite passed, including 12 member modifier unit tests.
The total test measured 1,584 witnesses and 468 variants.
Its costs were TypeScript 0.693 s, checker 0.536 s, and total 1.560 s.
`cargo fmt --check`, `git diff --check`, and `tools/hygiene.sh` passed.
`cargo clippy --offline --locked --workspace --all-targets` passed with no new warning.
Its library warning counts remain compiler 5, runtime 18, and codegen 13.
The API reference generator updated the corpus index through the generator.
No existing `.expected` file changed.
The round 1–2 files and the r349 collision line remain in the working tree.
The gate script did not run. No commit was made.

`cargo test --offline --locked -p subscript-codegen` passed in full.
Its golden corpus batch passed all 35 tests, including the JIT, C AOT, and golden comparisons.

## Phase Review

| Pass | CRITICAL | MAJOR | MINOR | Result |
|---|---:|---:|---:|---|
| 1 | 0 | 2 | 4 | Fixed in round 3; TS2808 recorded in §155.3 |
| 2 | 0 | 0 | 3 | Contrived write-form split recorded in §155.3; two record fixes |

Pass 1 MAJORs: a call of a function-typed field with an optional
parameter gave a false S018; a `@ValueType` class with a non-public
constructor was accepted (rule 6 added).

Final measured cost of the total test, warm: 1,584 witnesses, 468
variants, tsc 0.588 s, checker 0.501 s, cargo `finished in 1.53s`.

Final gate: `gate full aabf66f1 dirty:34 debug 2271/0/3 release 2268/0/3 skips 2/0 clippy 5/18/13 goldens-moved 0 exit 0`.
The first run failed on the `r350` `tsc:` header (the handoff named
TS1238 only; `tsc` gives TS1238 and TS1270); the header was corrected
and the gate re-run once.

Status: COMPLETE.
