# Generator counts (§314)

The contract pin is `166c3006feeea635767c378a9063683f6d69c030`.
The candidate uses inline generator counts and meets rule 2 within the public C ABI.
The interpreter keeps its description path.

## Registration condition

A mirror result can supply an unregistered generator at the pin.
The checker accepts `declare function hostGenerator(): Generator<i32>;`.
A host that copies the internal layout can supply this frame.
The public C header has no function that makes, resumes, or counts a generator.

Rule 2a closes this source.
The checker rejects a builtin `Generator` annotation during mirror type resolution.
This check covers parameters, results, aliases, callback signatures, class fields, constructors, and methods.
Test `mirror_generator_types_are_rejected_in_every_position` covers these positions and nested types.

Each script generator creator installs its marker before it returns.
The C creator calls `subscript_rt_object_describe` in `codegen/src/cemit/emitter.rs`.
The JIT creator calls the same function in `codegen/src/lower/func.rs`.
Completion and close keep the marker until the last holder releases the frame.
A successful reload keeps the Context and old JIT modules in `ReloadSession::reload`.
It changes the function table and epoch; it does not clear the marker.
The audit also checked `hostBox(): object` cast to a script class with a generator field.
The host built the object and an unregistered frame from their internal layouts.
The pin printed `42`, then `1`; the candidate printed `42`, then `3`.
Both runs exited 0 and reported trap kind 0.
TypeScript 5.9.2 accepted the fixture with the repository options and prelude.
Rule 2b places this case outside the public C ABI and outside rule 2.
An object that a script makes and gives to the host keeps its registered generator when the host returns it.

## Red results

Each test ran against the pin source from a separate `git archive HEAD` copy.
That copy used its own Cargo target directory.

- The mirror test failed for the result, parameter, and held-type forms.
- The pin accepted reject entry `r519-mirror-generator`.
- TypeScript 5.9.2 accepted `r519` with exit 0 under the repository options and prelude.
- The C emission test found a `subscript_rt_counted_value` call and failed.
- The JIT lowering test found the same runtime import call and failed.

The new divergence cites C24.
Its TypeScript example returns a generator from a mirror.
Its accepted alternative wraps a scalar host function in a script generator.
The total rejection test checks the example and its measured TypeScript class.

## Measurement

The method follows the measurement at `bdfb6ec1`.
Each program runs 10,000,000 calls on the ship tier.
Each cell gives the median of five runs.
Each process runs alone. No benchmark processes overlap.
The timer measures process start through process exit.
Each run prints `10000000` and exits 0.
The median includes all five samples; the first launch takes more time.

The host uses arm64 and Apple clang 21.0.0.
Both compiler copies use Cargo release builds.
The pin copy resides outside the work tree and uses its own target directory.
Each emitted C program links the runtime from its own compiler copy.
The C options are `-std=c11 -O2 -fwrapv -ffp-contract=off`.
The linker uses `-Wl,-dead_strip`.

The sources are in `benchmarks/workloads/subscript/generator-count/`.
`for-of.ts`, `lambda.ts`, and `default.ts` reproduce the three contract forms.
`pick.ts` calls `pick(g)` and discards the result.

The control removes one balanced count pair from a temporary C copy.
It removes the release's pending-trap check with that release.
At the pin, the control removes two description calls.
After the change, it removes their two inline count blocks and the runtime fallback.
Every other operation stays in place.
The position ids identify the pair: 10/13 for `for…of`, 11/14 for lambda, and 7/8 for default.
For `pick`, the control removes positions 2/3 inside `pick`.
These controls measure count cost; they do not change the compiler or the repository's generated code.

Times use milliseconds. Pair cost uses nanoseconds per loop call.
A negative cost records measurement noise.

| Form | Pin | Pin control | Pin pair ns | After | After control | After pair ns |
|---|---:|---:|---:|---:|---:|---:|
| `for…of` | 1103.5 | 919.5 | 18.4 | 138.8 | 138.4 | 0.0 |
| lambda capture | 926.3 | 726.5 | 20.0 | 121.2 | 121.5 | -0.0 |
| omitted default | 904.8 | 697.3 | 20.8 | 117.5 | 118.9 | -0.1 |
| `pick` | 430.6 | 223.4 | 20.7 | 54.3 | 41.0 | 1.3 |

The three extra pairs fall below 0.2 ns per call after the change.
The pair inside `pick` falls from 20.7 ns to 1.3 ns.
The full loops improve because all direct generator counts take the inline path.
Generator containers retain the description path.

## Runtime boundary

The generated retain tests null and saturates the holder count at `UINT32_MAX`.
The generated release decrements a count above one.
At one or zero, it calls `subscript_rt_async_release`.
The runtime retains frame cleanup and held-value releases.
The runtime tests cover the last holder, two holders, and a registered frame with zero holders.
Each test runs with development and ship allocation modes.

## Verification

- `cargo test -p subscript-codegen --tests`: passed.
- `cargo test -p subscript-runtime --tests`: passed.
- `cargo test -p subscript-compiler --tests`: passed.
- `cargo test -p subscript-compiler --lib`: passed; 681 tests.
- `cargo test -p subscript-cli --tests`: passed.
- `cargo test -p subscript-compiler --test lir`: not required; no HIR or LIR record changed.
- `node_modules/.bin/tsc -p tsconfig.json`: passed.
- `cargo clippy --offline --locked --workspace --all-targets`: passed. The library warning counts match the compiler/runtime/codegen baseline of 4/10/13.
- `cargo fmt --check`: passed.

The documentation generator ran with `cargo run --offline --locked -p subscript-compiler --bin generate-api-reference`.
The corpus index is its output.
The test `generated_ai_references_are_byte_identical` passed in both compiler runs.
The phase review found a generated-reference mismatch after the header correction.
The documentation generator updated the reference before the next test run.
The tracking note now uses the current contract pin and applies rule 2b to the host case.
No HIR or LIR record changed.
No expected-output golden changed.
No staging command, commit, or gate-script command ran.

## Phase review fixes (§314-4)

The reject header uses the current HEAD pin and states line 10.
The C24 row stays inside its table.
The divergence alternative wraps `hostNext()` in a script generator.
The existing checker and TypeScript batches check this form.
The reject test lists r519 in `EXPECTED`.
The C emission assertion uses the cast and `generator_layout::HOLDERS_OFFSET`.

The test `generated_last_holder_releases_free_exactly_once` runs one module through each tier.
Each holder replacement creates one known frame and releases the old reference.
The live allocation count stays constant when a last holder ends.
A shared frame survives the first release and disappears after the second release.
The dev test uses `ReloadSession::live_allocations`.
The ship host uses `subscript_rt_ctx_live_allocations`.
No collector runs in this test.
Its measured debug cost is 0.55 s, excluding the Rust build.
The other new tests state their measured costs beside the tests.

The phase review verification passed the runtime tests, compiler tests, and compiler library tests.
The compiler library run passed all 681 tests.
The TypeScript project check and format check passed.
Clippy completed with library warning counts of 4/10/13 for compiler/runtime/codegen.
The complete codegen test run passed, including the new last-holder test.
