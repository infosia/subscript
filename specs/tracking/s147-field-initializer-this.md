# §147: reads of earlier initialized fields

Contract pin: `69a25806`. No commit was made.

## Red and external measurements

A CLI built from the pin rejects `a316-field-initializer-earlier-fields`
with seven S100 diagnostics. Each says:

```text
error[S100]: `this` is only available in constructors and methods
error: 7 error(s)
```

The seven sites cover ordinary, constructor-assigned, value, generic,
static-adjacent, reference-member, and ordered initializer reads.

TypeScript 5.9.2 accepts a316. Node v24.18.0 produces the golden:

```text
4
9 4
6
8 2
13
17
argument
first
second
body
20 21 30
```

The entry supplies an identity `ValueType` decorator for Node.
The compiler still checks and lowers the decorated class as a value class.

Measured reject headers: r306, r307, and r308 report TS2729.
TypeScript accepts r309, r310, r311, and r312. Those four diagnostics carry C9.

## Changes and verdicts

The initializer context carries the class type and earlier initialized field names.
The checker accepts their named reads and rejects every other `this` form with §147 rule 2.
Write indices and assignment sources keep their read context.

Changed verdicts: a316 and the former r126 shape change from reject to accept.
The r126 file and table row retire. The deletion remains unstaged.
The old checker test now rejects a later-field read.
The matrix self-test now rejects a method call instead of an earlier-field read.
All product matrix field-initializer cells still call receiver helpers and remain C9 rejects.
No product matrix verdict changes.

Existing lowering binds the allocated instance before each initializer.
The named read lowers through `ExprKind::Field` to `LoadField` of that instance.
No production codegen change is required.
No existing `.expected` golden changes.

The document generator regenerated all three references; only the corpus index changes.
The LIR text snapshot stays identical because a316 introduces no generator or async function.

## Validation

The compiler and codegen suites pass with `cargo test --offline --locked`.
The a316 test compares dev, ship, and interpreter bytes with the golden.
The compiler suite includes the JS corpus gate, TypeScript corpus gate, and §143 matrix.
The matrix has 14 passing tests.
The 42 reject-suite tests pass. Additional tests cover writes, casts, updates, setters, arguments, lambdas, indices, and sources.

The fresh Phase Review reports no CRITICAL, MAJOR, or MINOR findings.
`cargo fmt --check`, `git diff --check`, and `tools/hygiene.sh` pass.
`tools/gate.sh` was not run. The coding agent did not edit `collisions.md`.

## Landing gate

The orchestrator amended C9 to the §147 rule.

```text
gate full a033991d0d8e382aae8a7624f044dcc828b8990e dirty:24 debug 2197/0/3 release 2194/0/3 skips 2/0 clippy 2/18/13 goldens-moved 0 exit 0
```
