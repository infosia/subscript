# Nullable function values — compiler.md §122

## File-size check

The Rust additions affect these files. Each existing file has fewer than 2,000 lines:

- `compiler/src/check/expr/call.rs`
- `compiler/src/types.rs`
- `compiler/tests/nullable_function.rs` (new)
- `codegen/src/layout.rs`
- `codegen/src/lir/place.rs`
- `codegen/src/lower/func.rs`
- `codegen/src/lower/func/expr.rs`
- `codegen/src/lower/func/value.rs`
- `codegen/src/lower/func/coroutine.rs`
- `codegen/src/cemit/emitter.rs`
- `codegen/src/cemit/body.rs`
- `codegen/src/cemit/graph.rs`
- `codegen/src/cemit/arith.rs`
- `codegen/src/interpreter/memory.rs`
- `codegen/src/interpreter/operations.rs`
- `codegen/tests/nullable_function.rs` (new)

No prerequisite split applies.

## Red

At HEAD `9a3a310`, the a271 checker reports S100 at lines 8, 12, 16, and 33.
Each message states: `type (i32) => i32 | null is not callable` (with backticks around the type).
The dev JIT and ship C entry points return the same four checker diagnostics.
The interpreter cannot start because the checker rejects the source.
The measurement uses `cargo test --offline --locked -p subscript-codegen --test nullable_function -- --nocapture`.
The production sources match HEAD during this measurement.
`node_modules/.bin/tsc -p tsconfig.json` accepts the entry.

A separate build from `git archive 9072742` uses an isolated target directory.
Its checker reports the same four S100 diagnostics for a271.

## Cause and change

`check_named_call` built local and global callee expressions directly from their declared types.
Those paths bypassed `check_ident`, which applies the existing `apply_narrowing` rule.
Both paths now use `check_ident`.
`apply_narrowing` unwraps `Type::Nullable` from path facts without an inner-type list.
The conditional and nullish type rules need no change.

Bare and nullable functions use a 16-byte code/environment pair, aligned to eight bytes.
The null pair contains two zero words.
The JIT uses `Repr::Pair`; ship C uses `SubFn`.
Conversions between matching function signatures preserve the pair.
A null constant receives the destination function type before a join.
Neither tier applies a scalar conversion to a function pair.
Null comparisons test the code word.

The interpreter uses `Value::Null` through constants, conversions, and block arguments.
Its function storage writes two zero words and reads zero code as `Value::Null`.
Non-null function storage retains the function identity.

`HandleKind::NullableFunc` retains its nullable classification and denotes the nullable pair.
Its collector-managed and lifetime-trap facts are false because the pair is not a Context allocation.
The existing environment storage retains captured values for both bare and nullable function values.
The environment copy paths include block parameters and coroutine storage.

## Tests and measurements

- `compiler/tests/nullable_function.rs`: three tests pass.
  They cover signatures, storage layout, all null-flow paths, assignment invalidation, and the capture storage boundary.
  Rejected controls cover missing or opposite guards, escaped flow facts, `f!`, `f?.()`, and captured field stores.
- `codegen/tests/nullable_function.rs`: five tests pass in 1.26 seconds.
  Three tests run a271 separately on the interpreter, JIT, and ship C.
  Two tests cover null/non-null loop joins, globals, explicit collection, and a direct awaited nullable capture.
  Each output test observes both null and non-null cases.
- Node produces the eight lines in `a271-nullable-function-value.expected`.
  All three engines produce those bytes.
- `node_modules/.bin/tsc -p tsconfig.json` accepts the entry.
- The existing managed-word test expects zero scanned words for the nullable pair.
  Its string and reference cases retain positive controls.
- `generate-api-reference` regenerates the corpus index through the existing generator.
- The captured LIR snapshot equals the committed snapshot byte for byte: zero changed lines.
  The snapshot selector does not include a271.
- No existing `.expected` file changes.

## Other changed files

- `codegen/src/cemit.rs`: the nullable function managed-word expectation changes from one to zero.
- `corpus/accept/a271-nullable-function-value.ts`: the new source entry.
- `corpus/accept/a271-nullable-function-value.expected`: the new eight-line output.
- `generated-docs/corpus-index.md`: the generator adds the a271 row.
- `specs/tracking/s122-nullable-function.md`: this measurement record.

## Pre-gate checks

`cargo fmt --all --check` passes with the pinned toolchain.
`cargo build --offline --locked --workspace --all-targets` passes without warnings.
`git diff --check` passes.
All 17 changed Rust files remain below 2,000 lines.
No git write command runs.

## Full gate

`tools/gate.sh full` runs once and exits zero.
The debug tests report 1,808 passed, zero failed, and three ignored.
The release tests report 1,805 passed, zero failed, and three ignored.
Clippy, TypeScript, and hygiene exit zero.

```text
gate full 9a3a3108ae089256556252213b1b9ce140dbacab dirty:21 debug 1808/0/3 release 1805/0/3 skips 2/0 clippy 5/18/13 goldens-moved 0 exit 0
```

## Open items outside §122

- `Map<K, F>.get` and `Map<K, F | null>.get` are rejected with S014.
  Q24 treats a function value as a scalar with no null miss value.
  The class forms are accepted.
- `h.cb(1)` on a function-typed field is rejected with S018 (`no method cb`); `tsc` accepts it.
- The addendum records both results at `9a3a310` as well.

## Review addendum

- `HandleKind::NullableFunc` names the nullable function pair. Its documentation states that no allocation exists.
- Both diagnostic type printers parenthesize a nullable function type: `((i32) => i32) | null`.
- No existing test pins the old nullable function display text.
  `function_signature_and_layout_cover_both_forms` uses the renamed handle kind.
  `nullable_function_display_groups_the_function` checks type display and the unguarded-call diagnostic.
  `nullable_function_capture_diagnostic_groups_the_function` checks the capture diagnostic.
- The interpreter removes callable-versus-callable equality and retains callable-versus-null equality.
- The Rust additions affect `compiler/src/types.rs`, `compiler/src/check/capture.rs`, and `compiler/tests/nullable_function.rs`.
  Each file is below 2,000 lines. No split applies.
- The compiler nullable-function suite passes five tests in 0.01 seconds.
  The codegen nullable-function suite passes five tests in 0.85 seconds.
- `cargo fmt --all --check` passes with the pinned toolchain.
  `cargo build --offline --locked --workspace --all-targets` passes without warnings.
- The addendum full gate runs once and exits zero.
  Debug reports 1,810 passed, zero failed, and three ignored.
  Release reports 1,807 passed, zero failed, and three ignored.
  TypeScript and hygiene pass. No existing golden or LIR snapshot changes.

```text
gate full c1e22ae4926634d9249cdbe097e1b8e20ed0cb04 dirty:22 debug 1810/0/3 release 1807/0/3 skips 2/0 clippy 5/18/13 goldens-moved 0 exit 0
```
