# R37 — a named accessor is method sugar

Status: **landed 2026-08-25** against `specs/blocks/compiler.md`
§65 and `specs/blocks/collisions.md` C12. Origin: downstream
request R37. Contract `2bc4a4f`, amended `0840f51`, implementation `f29c4c5`.

## The request

The downstream compiles typed HIR to WGSL and reinterprets TypeGPU,
which reads a binding, an address-space variable, and a vector
swizzle through a property. subscript had no accessor, so the
downstream wrote a call at 49 authored sites and in 3 library
classes.

## Findings on this host, at `f99d4cb`

- The four probes reproduce. One message, "static methods and
  accessors are not decided", covered a static member and every
  accessor.
- Mirror ingestion rejected an accessor through that same shared
  message, so the split was necessary.
- The `$` collision was a live tier divergence, not a latent one. A
  class with the methods `$` and `_` ran on the dev tier and printed
  `1,2`. The ship tier emitted two definitions of `subscript_m0__`
  and the C compiler stopped: "redefinition of 'subscript_m0__'"
  (Apple clang). No corpus entry covered it, and no identifier in
  `corpus/`, `examples/`, or `prelude/` held a `$`.
- `hir::DISPOSE_METHOD_NAME` is `"[[Symbol.dispose]]"`, so a
  reserved HIR method name was already the practice.
- `tsc` 5.9.2 accepted the whole asked accept surface. It also
  accepted `x.v += 1`, `x.v++`, the write used as a value, a static
  accessor, and a write accessor on a value class, so five of the
  seven rejections are narrower pins. It rejected a write through a
  read-only accessor (TS2540) and a field that shares an accessor
  name (TS2300).

## The one divergence from the request

The request asks for one method named `name`. The pair records as
two ordinary methods: `name` (read, no parameters) and `name=`
(write, one parameter, `void`). A class method table holds one
signature per name, and both tiers key a method by its name, so two
signatures under one name collide. An identifier holds no `=`, so
neither name collides with a declared method.

Verified before the contract: no consumer of a method name parses
it. `codegen/src/reload.rs` uses the name in a hash key,
`codegen/src/lower/mod.rs` keys `FnKey::Method(ci, name)` and looks
up by exact name, `codegen/src/cemit.rs` passes the name through
`sanitize`, and `compiler/src/warn.rs` and
`compiler/src/trap_sites.rs` iterate without reading the name.
`compiler/src/api_reference.rs` and `compiler/src/check/json.rs` do
not reflect over methods.

## What landed

The checker collects an accessor pair as two ordinary methods and
rewrites both spellings, exactly as §58 rewrites an index signature.
`ClassSig` gains a checker-side set of accessor names; `hir::ClassDef`
gains nothing. `member_on` rewrites a read, for a read position and a
write target alike; `check_assign` rewrites the statement write and
reports the rule 6 and rule 7 rejections; `check_update` reports the
increment and decrement rejections. A static accessor, a mirror
accessor, and a descriptor accessor each got a message of its own,
because one message covered a static member and every accessor.

Rule 10 changed after the phase review (below). The emitter now holds
one name table for each C namespace it names into: the methods of one
class, the fields of one class, the module's functions, the module's
globals, and the parameters of one function. `sanitize` escapes `$`
as `_dollar_` and `=` as `_set_`, and the table appends the smallest
free `_N` when two distinct HIR names produce one C identifier.

Corpus: `a144-accessor` (accept), `r141`-`r147` (reject). Five reject
entries are `tsc`-clean and record it in the header; `r142` (TS2540)
and `r146` (TS2300) are not. Tests: `compiler/tests/accessor.rs` (12),
the `sanitize` and name-table tests in `codegen/src/cemit.rs`, and one
emitted-C test in `codegen/tests/cemit.rs`. Counts: accept `.ts`
142 -> 143; `.expected` 143 -> 144; rejects 135 -> 142; accept source
files 144 -> 145.

## Gates (this host, at `f29c4c5`)

- `cargo test --offline --workspace`: 59 suites, 1007 passed, 0
  failed, 1 ignored, in both profiles. Wall time on this host: 476 s
  (debug) and 245 s (release).
- `cargo build --offline --workspace --all-targets`: 0 warnings.
- `cargo fmt --check`: exit 0. `tsc` 5.9.2 gate: exit 0.
- `cargo clippy --offline --workspace --all-targets`: exit 0.
  Library counts at the recorded baseline: `subscript-compiler` 7,
  `subscript-runtime` 22, `subscript-codegen` 29.
- No pre-existing golden or `.expected` changed.
- `a144` agrees across dev JIT, ship C-AOT, and the golden. The
  probe of 65 item 3 now compiles on the ship tier.

## Review (fresh no-context subagent)

Four MAJOR and ten MINOR. Every MAJOR was reproduced here before
the fix and again after it. Two of the four were defects in the
contract, so §65 was amended first (`0840f51`) and the
implementation followed.

- MAJOR-1, contract defect. Rule 10 defined escapes alone. `$` and
  `=` escape to legal identifier fragments, so `get v` / `set v`
  beside an ordinary method `v_set_` ran on the dev tier and stopped
  the C compiler: "redefinition of 'subscript_m0_v_set_'". The
  divergence of the finding above was reachable with no `$` at all.
  An escape alone cannot be injective, because a C identifier holds
  only `[A-Za-z0-9_]` and every escape text is a legal source
  identifier. Rule 10 now states the property and the per-namespace
  table. `a144` pins the shape on both tiers.
- MAJOR-2, contract defect. Rule 1 did not state that a write
  accessor declares no return type. The implementation accepted
  `set v(x: i32): string`, which stock `tsc` rejects (TS1095), so
  the program broke invariant 5. Rule 1 now states it, with the
  parameter-default case beside it (TS1052).
- MAJOR-3. A second write accessor passed the rule 3 clash test,
  overwrote the first signature, and reached "internal lowering
  error: define Method(0, \"v=\"): Duplicate definition". Rule 3 now
  covers two accessors of one kind.
- MAJOR-4. The written value took its context from the read
  accessor's return type, so `get v(): u8` with `set v(x: i32)` and
  `a.v = 300` reported S008. New rule 1a requires one type for the
  pair, which closes it at the declaration.
- Ten MINOR, all fixed: the message for two accessors of one kind,
  a defaulted write-accessor parameter, two unreachable branches, a
  spurious second diagnostic on a rejected accessor body, the
  descriptor message, the `sanitize` doc comment, a direct
  `sanitize` test, a comment on the HIR identity test, two silent
  poisoned returns, and one sentence in C12.

Verified with no finding: declaration-order independence in ten
member orders, the receiver evaluated once, the rewrite unreachable
on mirror, descriptor, and template classes, `is_c_keyword` after
the escape, every consumer of an HIR method name, and every
`tsc`-clean header claim.

## Adjacent defect, not fixed here

The emitter builds derived symbols by suffix, and rule 10's tables
hold HIR names only. An async method `x` and an ordinary method
`x_resume` both emit `subscript_m0_x_resume`: the dev tier runs and
prints `1` then `2`, and the ship tier stops with a redefinition
error. The `_resume` suffix predates R37, so this is not a
regression, and no corpus entry covers it. `subscript_export_*` and
the other derived symbols need the same audit. This needs its own
request, its own corpus entry, and its own exit criteria.
