# R29 — a class index signature is accessor sugar

Status: **landed 2026-08-15** against `specs/blocks/compiler.md`
§58 and `collisions.md` C10. Origin: downstream request R29.
Contract `0532c58`, implementation `e597362`.

## The request

The downstream authors GPU kernels in subscript and compiles the
typed HIR to WGSL. A kernel body is index-dense
(`out[i] = f(a[i], b[i])`), and the checker rejects `a[i]` on
their generic wrapper classes. Ask: index signatures on ambient
declarations, with `readonly` as the write gate; the named
fallback is permanent `get`/`set` spelling.

## Findings on this host, at `dfef090`

- The report reproduces: `check_index` indexes `Type::Array` and
  `Type::FixedArray` alone and fails a class receiver with S100
  "is not indexable". The handoff's cause analysis is exact.
- The handoff's sketched substrate (an ambient generic
  `declare class`) checks through no route at the pin. Mirror
  ingestion rejects a generic `declare class` at the declaration.
  A non-mirror `declare class` rejects each body-less method. An
  arrow-typed field is not callable.
- The form that checks at the pin is the generic script class
  with method bodies. Its methods check, and an `unreachable()`
  body satisfies a generic return. The handoff's error message
  comes from this form.
- Stock `tsc` accepts a class index signature with a sized-alias
  index type, readonly and mutable, beside named members
  (measured, exit 0).

## What landed

The decision moves the feature to the substrate that checks: a
reference class declaration can declare one index signature
(`[index: I]: T`, `I` = `i32` or `u32`, optional `readonly`) and
must declare the matching `get`, and `set` when writable. The
checker rewrites `a[i]` to the same HIR as `a.get(i)` and the
statement write to `a.set(i, v)`. A readonly write, a compound
assignment, increment, decrement, a write used as a value, a
value-class signature, and a mirror signature each fail with a
named S100. The rewrite is checker-complete: no codegen, runtime,
or prelude change, and the tiers agree by construction.

Corpus: `a136-index-signature` (accept; generic readonly and
mutable wrappers at two element types; the golden pins reads, a
write, and `m[0] === m.get(0)` as `true`),
`r128-readonly-index-write`, `r129-index-signature-no-get`,
`r130-index-compound-assign`. A unit test asserts the HIR of the
sugar equals the HIR of the spelled calls, node for node.

## Red, at the contract pin

The signature member failed with "class member form outside the
decided surface"; every indexed spelling failed with "is not
indexable" and the `i32`-index rule.

## Gates (this host, at `e597362`)

- `cargo test --offline --workspace`: 55 suites, 946 passed, 0
  failed, 1 ignored, exit 0. The same counts in the release
  profile.
- `cargo build --offline --workspace --all-targets`: 0 warnings.
- `cargo fmt --check`: exit 0. `tsc` gate: exit 0.
- Every pre-existing golden and `.expected` file is
  byte-identical; the only new golden is a136's (136 total).
