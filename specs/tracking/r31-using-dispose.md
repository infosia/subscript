# R31 — `using` declarations run dispose at every scope exit

Status: **landed 2026-08-16** against `specs/blocks/compiler.md`
§60 and `collisions.md` C11. Origin: downstream request R31 (with
R30 in one handoff; their pin `dae6e10`). Contract `8834fb3`,
implementation `bac11eb`. Three owner decisions of 2026-08-16
pre-dated the contract and were its starting point: nullable
`using` rejected; a trap does not run dispose; the cleanup member
is `dispose`, with `[Symbol.dispose]` as the hook.

## The request

The downstream's script-driven programs hold 577 `dispose()` call
sites across two suites, hand-held in reverse creation order;
`a27-host-compute.ts` alone ends in a 16-line release tail with
two growing early-return failure paths, and every program that
reads a GPU result crosses at least one `await` between creation
and disposal.

## Findings on this host, at `e8e01d9`

- The pinned `swc` parses `using` declarations and
  `[Symbol.dispose]()` members; both reached the checker as clean
  S100 rejections, so the parser needed no change.
- `tsc` 5.9.2 accepts the hook member with
  `lib: ["ES2022", "ESNext.Disposable"]` (exit 0) and fails it
  with TS2318/TS2550 without the added lib entry.
- `node` v24.18.0 (exit 0) fixed the semantics: reverse
  declaration order at block end; the return expression evaluates
  before the disposals; an early return disposes only live
  bindings; loops dispose per iteration, `break` included; a
  suspended `async` frame disposes at completion.

## What landed

A reference class can declare `[Symbol.dispose](): void`, stored
under the reserved internal method name `[[Symbol.dispose]]`.
`using x = expr` binds an immutable reference to such a class.
The checker rewrites the declaration to a `const` binding and
inserts hook calls at every scope exit; a synthesized local
carries a return value across the disposals. A trap does not run
dispose. Nullable initializers, `await using`, module-level and
`for`-head `using`, hook-less initializer types, and hooks on
value or descriptor classes each fail with a named S100. The
rewrite is checker-complete — no parser, codegen, runtime, or
prelude change — so the tiers agree by construction.
`tsconfig.json` `lib` gains `"ESNext.Disposable"`.

Corpus: `a138-using-dispose` (the golden equals the `node`
measurement shape), `a139-using-async` (`resumed` prints before
`dispose:async`), `r131-using-nullable-init`, `r132-await-using`,
`r133-using-without-dispose`.

## v1 boundary

A `using` inside a lambda body keeps the pre-existing generic
rejection ("nested declarations are not in the decided surface";
probe, this host): the rewrite walks function and method bodies.
The rejection is loud, and the downstream's kernels are
functions. Widen only with evidence.

## Red, at the contract pin

The hook member failed with "computed method names are not
decided"; every `using` statement failed with "nested
declarations are not in the decided surface"; `await using`
failed the same way.

## Gates (this host, at `bac11eb`)

- `cargo test --offline --workspace`: 55 suites, 959 passed, 0
  failed, 1 ignored, exit 0. The same counts in the release
  profile.
- `cargo build --offline --workspace --all-targets`: 0 warnings.
- `cargo fmt --check`: exit 0. `tsc` gate (with the new lib
  entry): exit 0.
- Every pre-existing golden and `.expected` file is
  byte-identical; the new goldens are a138's and a139's
  (139 total).
