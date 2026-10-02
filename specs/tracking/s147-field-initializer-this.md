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
The r126 file and table row retire.
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
The golden and interpreter corpus sweeps compare a316 and a317 with their goldens.
The compiler suite includes the JS corpus gate, TypeScript corpus gate, and §143 matrix.
The matrix has 14 passing tests.
The 44 reject-suite tests pass. Additional tests cover writes, casts, updates, setters, arguments, lambdas, indices, and sources.

`cargo fmt --check`, `git diff --check`, and `tools/hygiene.sh` pass.
`tools/gate.sh` was not run. The coding agent did not edit `collisions.md`.

## Landing gate

The orchestrator amended C9 to the §147 rule.

```text
gate full a033991d0d8e382aae8a7624f044dcc828b8990e dirty:24 debug 2197/0/3 release 2194/0/3 skips 2/0 clippy 2/18/13 goldens-moved 0 exit 0
```

## Review fixes

The pre-change CLI accepts r313, r314, and r315. It rejects both calls in a317 with S100.
No existing descriptor default in the corpus, examples, or tests reads `this`.
The r94 method uses `this`; it is outside a default and stays rejected.

Descriptor defaults now reject every `this` with S100 and §147 rule 3a.
TypeScript 5.9.2 accepts r313 and r314. It rejects r315 with TS2532.
The TS2532 arithmetic site carries no TypeScript-accepts block.

The checker accepts `this.cb()` and `(this.cb)()` for an earlier initialized function field.
TypeScript accepts a317. Node v24.18.0 prints `3 3`, which matches its new golden.
The a316 golden stays identical. No existing golden changes.
Indirect `this` forms name the value rule; nested descriptor uses also have regression tests.

The duplicate a316 test in `codegen/tests/golden.rs` retires.
The document generator updates the corpus index. The C9 diagnostic explanation also names rule 3a.
The orchestrator must add r313–r315 and a317 to C9's entry list.

## Phase Review fix round landing gate

```text
gate full 6a821b4e5e9ba5ed7d3668802c123eea3068698f dirty:16 debug 2198/0/3 release 2195/0/3 skips 2/0 clippy 2/18/13 goldens-moved 0 exit 0
```

## Phase Review result

One review pass found MAJOR 1 (pre-existing: a `@Descriptor` default read
`this` freely, reading a field with no value and exposing the partial
instance) and MINOR 3 (an earlier function field call was rejected; a
test duplicated the corpus tests; two process sentences in this note).
All are fixed; rules 1a and 3a state the contract changes. §147 is
COMPLETE.
