# A generic instance chain that grows without bound

Contract: `compiler.md` §140. Current amendment: `89a3a696550796a56d53ec6de862e8eb79bee52c`.
Red baseline: `fdf384f8b9e77401c9cf1b8232bf97b5bfe39637`.

## Stack-overflow Red at `fdf384f8` (re-measured)

Entry: `corpus/reject/r288-growing-instance-chain.ts`.
The CLI was built at the pin with `cargo build --offline --locked -p subscript-cli`.

`subscript check corpus/reject/r288-growing-instance-chain.ts` aborts with signal 6:

```text
thread 'main' has overflowed its stack
fatal runtime error: stack overflow, aborting
```

`tsc` 5.9.2 accepts the entry with strict mode, ES2022, the language prelude, and no ambient package types.
`node` exits 0 and prints:

```text
3
```

## Result before the third rule text

The checker keeps each active request's template identity, argument types, and dependency on a type parameter before substitution.
A proper expansion that retains the parameter dependency reports S100 at the request site with the C19 divergence block.
A constant request breaks that dependency and stays accepted.
Deferred class bodies keep the chain from the signature pass.
Static and instance methods have separate template identities.
The opaque check uses the same instantiation entry points.

The compiler tests cover functions, methods, static methods, class shapes, deferred bodies, mutual recursion, and concrete requests in the opaque check.
Each rejected shape asserts one diagnostic, its full message, its request site, and the rendered divergence block.
Ordinary recursion, unrelated arguments, and constant arguments stay accepted.
The interpreter, JIT, and C AOT produce `3 / 3 / 3 / 3 / 3 / true` for the finite controls.
The six three-engine tests take 0.55 seconds in debug and compile six C programs.

## Verification before the third rule text

- Pinned `cargo fmt --check`: exit 0.
- All-target workspace build: exit 0, warnings 0.
- Workspace clippy: exit 0, warnings in changed files 0.
- `tools/hygiene.sh`: exit 0.
- Compiler rejection tests: 14 pass.
- Opaque generic tests: 18 pass.
- Reject corpus tests: 39 pass.
- Divergence table tests: 5 pass.
- Compiler module identity tests: 21 pass.
- Codegen module identity tests: 7 pass.
- Three-engine control tests: 6 pass.
- Generated documents come from `generate-api-reference`.
- `divergence.rs`: 1,289 lines. `check/mod.rs`: 1,386 lines.
- Existing `.expected` files do not change.


## Constant-argument controls: Red

The earlier working-tree Red contains the concrete-containment instance-chain check.
These saved Red results describe that working tree, not the reachable baseline binary.
Each test was added before the implementation changed.
`cargo test --offline --locked -p subscript-codegen --test growing_instance_chain` exits 101.
The four new accepted controls report these errors:

```text
fp1: generic template `f`: the chain of instances grows without bound from `f<i32>` to `f<W<i32>>`
fp3: generic template `f`: the chain of instances grows without bound from `f<i32>` to `f<i32[]>`
fp2: generic template `f`: the chain of instances grows without bound from `f<i32>` to `f<W<i32>>`
fp4: generic template `Box`: the chain of instances grows without bound from `Box<i32>` to `Box<Box<i32>>`
```

The tests require `3`, `3`, `4`, and `true`, respectively.
The test result is 1 passed and 4 failed.

The message tests also precede the implementation change.
`cargo test --offline --locked -p subscript-compiler --test growing_instance_chain` exits 101.
The test result is 11 passed and 2 failed:

```text
actual: generic template `nest`: the chain of instances grows without bound from `nest<T>` to `nest<W<T> (main.ts)>`
expected: generic template `nest`: the chain of instances grows without bound from `nest<T>` to `nest<W<T>>`
actual: generic template `N`: the chain of instances grows without bound from `N<T>` to `N<W<T> (main.ts)>`
expected: generic template `N`: the chain of instances grows without bound from `N<T>` to `N<W<T>>`
```


## Constant-argument controls: result

The four accepted controls produce `3`, `3`, `4`, and `true` on the interpreter, JIT, C AOT, and Node.
`tsc` 5.9.2 accepts each one.
`a304` carries the first control and its new `3` golden.
C19 lists `a304` and states the parameter-dependency condition.

The additional parameter-forwarding control starts with a constant request to another template.
The next request forwards that template's parameter.
The Red CLI reports S100 from `f<i32>` to `f<W<i32>>`, and exits 1.
The corrected interpreter, JIT, C AOT, and Node print `4`.
`tsc` 5.9.2 accepts this control.

Opaque checks of separate templates make distinct `W<T>` instances whose opaque arguments have separate identities.
These instances share one source declaration, so their messages have no file label.
A test with two distinct `W` declarations retains the file label.
The two message tests now report `nest<W<T>>` and `N<W<T>>`.

## Per-position edges: Red and pin measurement

The four new tests precede the edge implementation.
The previous working-tree checker reports S100 for each finite shape:

```text
f1: generic template `f`: the chain of instances grows without bound from `f<i32, i32>` to `f<i32, W<i32>>`
f4: generic template `f`: the chain of instances grows without bound from `f<i32, i32>` to `f<i32, W<i32>>`
f6: generic template `f`: the chain of instances grows without bound from `f<i32, i32>` to `f<i32, P<i32, i32>>`
f8: generic template `N`: the chain of instances grows without bound from `N<i32, i32>` to `N<i32, W<i32>>`
```

The test command exits 101: six pass and four fail.
The required outputs are `3`, `3`, `3`, and `true`.

A separate source archive at `fdf384f8` builds the CLI with `cargo build --offline --locked -p subscript-cli`.
The pin's CLI rejects no type before r288 aborts with signal 6:

```text
thread 'main' has overflowed its stack
fatal runtime error: stack overflow, aborting
```

The pin's CLI accepts a304 with exit 0.
Thus a304 is a control at this pin, not a Red acceptance entry.
Its interpreter, JIT, and C AOT output remains `3`.
No existing `.expected` file changes.

The S011 array probe reports two errors before the priority check:

```text
error[S011]: unions are limited to `Ref | null`; `i32 | null` is not a reference type union
error[S100]: generic template `N`: the chain of instances grows without bound from `N<T>` to `N<T | null[]>` through argument `T | null[]`
```

## Per-position edges: implementation and tests

Each request carries edges per argument position, its source site, and the ordered names of its template parameters.
The checker composes the edges along the resolved chain and tests each diagonal path for an expanding edge.
The recorder matches every type form without a catch-all arm.
Rejected type forms make no instance requests.
Deferred class bodies retain the resolved chain from the signature pass.
Before rule 4a, nested opaque bodies are checked so a swapped expansion reaches the second request.
Reports of rotations of the same source-site cycle use one request site.
Argument source ranges give S011 precedence over an opaque growth report.

Compiler instance-chain tests: 21 pass.
Each growth test requires exactly one S100 with the full message and the C19 divergence block.
Coverage includes all required wrappers, signatures, fields, constructors, opaque templates, cross-module requests, and resolved call receivers.
The swapped-growth test reports `f<A, B>` to `f<W<A>, W<B>>` at depth two.
The two invalid-union tests require S011 only.
Opaque generic tests: 18 pass. Reject corpus tests: 39 pass.
Three-engine tests: 11 pass in 0.95 seconds.
The four new finite shapes output `3 / 3 / 3 / true` on each engine.
Plain swaps and the constant nested method return also run on each engine.

The opaque recursion control in `opaque_generics.rs` now calls `g<T>` instead of `g<T[]>`.
Section 140 rejects the old expansion; equal recursion keeps the section 135 body-check control valid.
Its unknown-name control still requires one S016.

Removed: both `derived` fields and their call-site plumbing, `argument_mentions_parameter`, and the concrete-containment helper `type_contains`.
Before rule 4a, `instance_body_is_checked` was removed to reach a second swapped request.
The current amendment restores it and leaves the second request to the program instance.
Its `root` fact still has a reader in the constraint check.

## Corpus measurement before the loader correction: stopped

The temporary harness runs once and is deleted.
It uses `codegen/tests/corpus/mod.rs` for accept, warn, and trap.
It copies the example discovery and mirror loaders from `examples/tests/gate.rs`.
The accept loader is insufficient for the trap category.
The actual trap loader, `codegen/tests/support/trap_corpus.rs`, adds the interop mirror for `SGPUProbeBlendState`.
The temporary harness omitted that mirror selection.

```text
accept: 294 accepted
warn: 5 accepted
REJECTED trap/t72-narrowing-boundary-getter:
S016: unknown type name `SGPUProbeBlendState` (line 8, column 14)
S016: unknown class `SGPUProbeBlendState` (line 8, column 47)
```

This result does not establish a compiler regression: the harness omitted the required ambient declaration.
The handoff requires a stop on any rejected source, so there is no second measurement.
The remaining trap entries and examples are not measured.
The stopped task had no full-gate verdict.
At that stop, Phase Review and the final validation steps remained outstanding.
That task stopped before completion.

## Changed files

- `compiler/src/check/bodies.rs`
- `compiler/src/check/body_check_cost.rs`
- `compiler/src/check/instance_chain.rs`
- `compiler/src/check/generics.rs`
- `compiler/src/check/opaque.rs`
- `compiler/src/check/mod.rs`
- `compiler/src/check/pipeline.rs`
- `compiler/src/check/type_rules.rs`
- `compiler/src/check/tyres.rs`
- `compiler/src/check/expr/call.rs`
- `compiler/src/check/expr/entry.rs`
- `compiler/src/divergence.rs`
- `compiler/tests/growing_instance_chain.rs`
- `compiler/tests/opaque_generics.rs`
- `compiler/tests/corpus_reject.rs`
- `codegen/tests/growing_instance_chain.rs`
- `corpus/reject/r288-growing-instance-chain.ts`
- `corpus/accept/a304-constant-instance-chain.ts`
- `corpus/accept/a304-constant-instance-chain.expected`
- `specs/blocks/collisions.md` (C19 only)
- `generated-docs/corpus-index.md`
- `specs/tracking/s140-growing-instance-chain.md`

## Corpus measurement with the category loaders

The temporary harness calls the source loaders that the category tests use.
Accept uses `codegen/tests/corpus/mod.rs`: `entry_ids` and `entry_sources`.
Warn uses `compiler/tests/corpus_warn.rs`: `accept_sources` and its mirror helpers.
Trap uses `codegen/tests/support/trap_corpus.rs`: `trap_ids` and `trap_sources`.
Examples use `examples/tests/gate.rs`: `discover_examples`, `discover_gate_programs`, and `source_files`.
The private functions come from temporary source copies; their loader bodies do not change.
The example set includes twelve numbered examples, one gate program, and the two host examples.
The harness runs once and is deleted.

```text
accept: 294 accepted
warn: 5 accepted
trap: 71 accepted
examples: 15 accepted
```

All 385 source sets are accepted with the loaders' ambient mirrors.
There is no rejection and no second measurement in this task.

## Final validation with the category loaders

A fresh no-context Phase Review reports no CRITICAL, MAJOR, or MINOR finding.
Pinned `cargo fmt --check`: exit 0.
`cargo build --offline --locked --workspace --all-targets`: exit 0, warnings 0.
`cargo clippy --offline --locked --workspace --all-targets`: exit 0, warnings in changed files 0.
Generated documents come from `generate-api-reference`: exit 0.
Existing `.expected` files do not change.

`tools/hygiene.sh`: exit 0.
`tools/gate.sh full` runs exactly once in the final task and exits 0.

```text
gate full fdf384f8b9e77401c9cf1b8232bf97b5bfe39637 dirty:20 debug 2126/0/3 release 2123/0/3 skips 2/0 clippy 3/18/13 goldens-moved 0 exit 0
```

Gate report: `target/gate/20261001T025133Z-full.md`.
No existing golden changes. The final task is complete.

## Opaque body cost: rule 4a

Amendment: `89a3a696550796a56d53ec6de862e8eb79bee52c`.
`instance_body_is_checked` is restored in function, method, and class instantiation.
Each opaque root checks its body; nested instances with opaque arguments resolve signatures and shapes only.
A fixed concrete request still checks its body and uses the growth guard.
The direct, class-signature, field, constructor, and S011 tests keep their outcomes.
The mutual, cross-module, receiver-method, and swapped-growth tests make program instances and still require one S100.
The swapped program instance rejects at depth two.
Its first opaque request has no expanding diagonal path, and its nested body is excluded.
No extra fact is needed for the program-instance chain.
Unused nested-body growth is now accepted, so this fix changes which unused programs are rejected.
A separate test requires that amended outcome for unused mutual, swapped, and receiver-method shapes.

The body counter and its instrumentation exist only under `cfg(test)`.
The counter records actual `check_function` body checks, including both checker runs.
The cost test uses 30, 60, and 120 helpers that call the next three helpers with their own parameter.
Its empty main makes no program instance.
The contract gives two checks per opaque root and two checks of main.

The initial exclusion-free Red counts 932 checks for 30 templates and 3662 for 60.
At 120, the test thread aborts with a stack overflow.
The assertion now follows each measured size, so the firing control fails before that deep case.
An isolated copy removes the exclusion once for that control; the repository keeps the restored exclusion.

```text
30 templates: 932 body checks
assertion `left == right` failed: 30 templates
  left: 932
 right: 62
test result: FAILED. 0 passed; 1 failed
firing control exit: 101
```

With the exclusion restored, the test passes:

```text
30 templates: 62 body checks
60 templates: 122 body checks
120 templates: 242 body checks
```

## Cost probes before and after the exclusion

Each number is the median of three debug CLI checks of the same source.
The 120-helper and List probes use the review's original source.
The 30- and 60-helper probes use the same helper body with fewer declarations.
The helper body constructs `Box<T>` and calls the next three helpers with `T`.
The source main calls `g0<i32>`.
The List probe has forty methods with a loop and 300 generic functions that each call two methods.
Its main calls the first generic function with a concrete List instance.

The before binary uses an isolated copy of the checker with nested opaque bodies enabled.
The after binary uses the restored exclusion at the amendment.
The temporary probes and isolated build are deleted after measurement.

| Probe | Before, seconds | After, seconds |
| --- | ---: | ---: |
| 30 helpers | 0.133 | 0.033 |
| 60 helpers | 0.551 | 0.062 |
| 120 helpers | 3.049 | 0.123 |
| List, 40 methods and 300 functions | 2.945 | 0.297 |

## Corpus measurement after rule 4a

The harness calls the category loaders' source functions with their ambient mirrors.
Accept uses `entry_ids` and `entry_sources`; warn uses `accept_sources`; trap uses `trap_ids` and `trap_sources`.
Examples use `discover_examples`, `discover_gate_programs`, and `source_files`.
The loader bodies come from the test sources without changes.
The measurement runs once, and the harness is deleted.

```text
accept: 294 accepted
warn: 5 accepted
trap: 71 accepted
examples/hot-reload/demo.ts: no errors
examples/rust-host/logic.ts: no errors
examples: 15 accepted
```

All 385 source sets check with no errors.
The fifteen examples include twelve numbered programs, one gate program, and both host examples.
The explicit host-example results remove the earlier count ambiguity.
A fresh no-context Phase Review reports no open CRITICAL, MAJOR, or MINOR finding after it verifies the firing control.

## Open advisory finding

Contrived: `C<T>.wrap(): C<W<T>>` can report S100 at both the return annotation and a matching `new` request.
The verification review measured two reports.
Rule 2 allows a report at each request site, so this advisory finding requires no fix.

## Final validation at the rule 4a amendment

Pinned `cargo fmt --check`: exit 0.
All-target workspace build: exit 0, warnings 0.
Workspace clippy: exit 0, warnings in changed files 0.
Generated documents come from `generate-api-reference`: exit 0.
The instance-chain tests pass all 22 cases; the opaque generic tests pass all 18 cases.
All changed Rust files remain below 2,000 lines.
No existing `.expected` file changes.

`tools/hygiene.sh`: exit 0.
`tools/gate.sh full` runs exactly once at the rule 4a amendment and exits 0.

```text
gate full 89a3a696550796a56d53ec6de862e8eb79bee52c dirty:22 debug 2128/0/3 release 2125/0/3 skips 2/0 clippy 3/18/13 goldens-moved 0 exit 0
```

Gate report: `target/gate/20261001T033337Z-full.md`.
No existing golden changes. The cost-regression task is complete.
