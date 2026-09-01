# R38 — a write through a `@CStruct` copy that nothing reads

Contract `29dc118` (§81, W004), amended at `4cf986f` (§81.2a:
`FixedArray`, `a56`) and `cda2c0d` (§81.2b: review round 1). Corpus pins
`08369b7`. Landing commit: see the last section.

## The request, and what the contract holds

R38 (2026-09-01, at `2f9ed28`) reported that a field write through a
`@CStruct` parameter, or through a local copied from another place,
succeeds and changes nothing, with no diagnostic. The downstream
shipped a drag interaction with this shape.

R38 proposed a reject at every assignment rooted in a copy binding.
Rejected: C2 (Q17) makes field writes through a copy legal, `a04` pins a
write to a copy-bound local, and `examples/e02` writes to a parameter
copy and returns from it. The defect is narrower: in both R38 sites the
copy is **write-only**. That is W004 (`warnings.md` §2), a warning, not
an S-code: a heuristic that rejects moves the accepted set on a false
positive.

## What moved

- `WarnCode::W004`; one pass per function body (free functions,
  methods, constructors, lambdas) that collects copy bindings —
  value-typed parameters, value-typed locals initialized from a place,
  `for...of` bindings of value type — and scans the body once for
  write roots and reads. A binding with at least one field or index
  write and zero reads fires at every write.
- `corpus/warn/w04-copy-parameter-write-unread.ts` (line 36) and
  `w05-copy-local-write-unread.ts` (line 38): R38's two shapes. Red at
  `08369b7`: `subscript check` reported `no errors` and no warning on
  both; the harness reported `expected W004 at line 36, got []`.
- `corpus/accept/a56-map-aggregate-foreach.ts`: the first
  implementation round stopped, correctly, on this entry. It writes a
  write-only `@CStruct` parameter and a write-only `FixedArray`
  parameter on purpose, to pin copy-on-pass in `Map.forEach` callbacks
  — R38's shape. Two decisions (§81.2a): `FixedArray` copies are in
  W004, and `a56`'s callbacks read their own copy after the write (a
  self-check that prints only when the copy lost the write). The golden
  did not move: `golden` 35 passed at `bfa0cf6`.

## Review round 1

A fresh review found two MAJOR and seven MINOR. MAJOR: bindings keyed
by name conflate shadowed locals — HIR `Local` carries a name and no
binding id, a form fact; v1 excludes any name bound more than once in a
function (recorded miss). MAJOR: the lambda paths had no test. MINOR:
`for...of` bindings, index-rooted field chains, and value-position
compound assignments were outside the pass; a `this`-write test could
not fail; "may" in the explanation; a non-place index renders as `…`.
All are in the contract at `cda2c0d` and closed in round 2 below.

Recorded misses (W004 text): a second write after a read; a write to a
captured copy inside a lambda; a name bound more than once.

## Review round 2

A second fresh review on the round-2 code. MAJOR: no test fired W004
inside a method or constructor body, so the class loop was unpinned;
closed by a method test that keeps its value-typed parameter write-only
and asserts one W004 naming it, plus a constructor test. MINOR: a `for`
step assignment was read as value position (contract: statement
position); a non-place `for...of` subject rendered as `…` (contract:
callee with `(…)`); the synthetic `[[for.of#N.subject]]` local could
become a candidate (contract: excluded); an unreachable
`HirChild::Stmt` arm; no two-write test. All in the contract at
`2932ee4`, closed in round 3. No third review: round 3 changed tests
and three small arms, and this session read each hunk.

Two review rounds, three coding rounds. The class the reviews raised
twice — a test that cannot fail — is recorded here as a pattern to
check at handoff: every "silent" test needs a firing control in the
same shape.

## Form facts the rounds found

- HIR `Local(String)` carries no binding id (§68). W004 excludes any
  name bound more than once in a function body.
- `for (const v of arr)` and `for (const v of arr.values())` both
  lower to `ForOfKind::ArrayValues` with the receiver as subject, so
  the rendered origin cannot tell the two spellings apart. Map
  `values()` keeps `ForOfKind::MapValues` and renders as
  `scores.values(…)`.

## Gates at `f993d60`, arm64

Debug 1198 passed, 0 failed, 1 ignored. Release 1196 passed, 0 failed,
1 ignored; `perf_gate_meets_every_threshold` ok. `cargo fmt --check`
exit 0. `tsc` exit 0 with `w04`, `w05`, and the changed `a56` included.
Clippy 7 / 18 / 13. `tools/hygiene.sh` exit 0. `golden` 35 passed; no
golden or `.expected` moved. `accept_corpus_and_examples_have_zero_warnings`
green over 175 accept files and every example.

Landed `f993d60`. Contract commits `29dc118`, `4cf986f`, `cda2c0d`,
`2932ee4`; corpus `08369b7`, `bfa0cf6`.
