<!-- §81 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 81. R38 — a write through a `@CStruct` copy that nothing reads

*(Owner decision, 2026-09-01.)* Origin: downstream request R38 at
`2f9ed28`. A `@CStruct` parameter, and a local bound by copy from
another place, each hold a copy (C2). A field write through the copy
succeeds and the source never changes. No diagnostic reports it. The
downstream shipped a drag interaction with this shape.

### 81.1 What the request proposed, and what the contract holds

R38 proposed a reject at every assignment rooted in a copy binding.
Rejected: C2 (Q17) states that field writes through a copy are legal,
`a04` pins a write to a copy-bound local, and `examples/e02` writes to
a parameter copy and returns from it. The reject would move both
pins and forbid the C idiom "change the copy, then return or store
it".

R38's alternative, reference semantics for `@CStruct` parameters, is
rejected: a by-value struct parameter crosses the C ABI by value
(invariant 1), and C2's model does not change.

The defect is narrower: in both R38 sites the copy is **write-only**.
`a04` and `e02` read the copy after the write. That is the rule.

### 81.2 The rule

**W004** (`warnings.md` §2): an assignment whose target roots in a
copy binding of `@CStruct` type fires when the binding is write-only in
its function. The definition of a copy binding, of a read, and the
recorded miss live in `warnings.md`; this section does not repeat
them.

Severity is a warning, not an S-code: the condition is a heuristic,
and a heuristic that rejects moves the accepted set on a false
positive. Warnings surface at every `check`, `emit`, `build`, and
`run` (warnings.md §1).

### 81.2a `FixedArray`, and the `a56` pin

*(2026-09-01, found by the first implementation round.)* `a56` writes
a write-only `@CStruct` parameter and a write-only `FixedArray`
parameter on purpose, to pin copy-on-pass in `Map.forEach` callbacks.
That is R38's shape, so W004 fires on it, and no rule keeps `a56` silent
without keeping R38 silent.

Two decisions. **`FixedArray` is a value type and its copy bindings
are in W004**: the same write vanishes the same way. **`a56` gains a
read of each copy** — a self-check that prints only when the copy lost
the write — so the entry pins copy-on-pass in both directions and the
golden does not move. No accept entry is exempted from the
zero-warning sweep.

### 81.2b Review round 1, 2026-09-01

The fresh review of the first implementation found: bindings keyed by
name conflate shadowed locals (MAJOR); no lambda test (MAJOR); a
`for...of` binding of value type was not a copy binding; an
index-rooted field chain was not a place; a compound assignment in
value position (`p.x++`) read nothing; a `this`-write test could not
fail; "may" in the explanation. W004 in `warnings.md` now states the
shadowing exclusion, the `for...of` binding, the index root, the
value-position read, and the capture miss. The `this` test gains a
value-typed parameter beside the `this` write.

### 81.2c Review round 2, 2026-09-01

MAJOR: no test fires W004 inside a method or constructor body, so the
`class.methods` / `ctor` loop is unpinned; the `this`-write test now
keeps its value-typed parameter write-only and asserts one W004 that
names it. MINOR: a `for` step assignment read as value position
(contract now: statement position); a non-place `for...of` subject
rendered as `…` (contract now: callee with `(…)`); the synthetic
`[[for.of#N.subject]]` local could become a candidate (contract now:
excluded); an unreachable `HirChild::Stmt` arm; "fires at every write"
had no two-write test.

### 81.3 Sites

- `compiler/src/warn.rs`: `WarnCode::W004`, `ALL`, `as_str`,
  `explanation`; one pass per function body (including methods,
  constructors, and lambda bodies) that collects copy bindings, then
  scans the body once for reads and write roots.
- `compiler/tests/corpus_warn.rs`: two `EXPECTED` rows.
- `corpus/warn/w04-copy-parameter-write-unread.ts` (the parameter
  form) and `corpus/warn/w05-copy-local-write-unread.ts` (the local
  form, copied from a reference-class field). Both are R38's evidence
  shapes.

### 81.4 Exit criteria (pre-registered)

1. `w04` and `w05` are Red at `29dc118` (zero warnings on a binary
   built from that pin) and fire W004 at the pinned lines after.
2. `accept_corpus_and_examples_have_zero_warnings` stays green:
   `a04`, `a21`, `e02`, and every other accept entry and example are
   silent.
3. Unit tests in `warn.rs` pin each mute: a field read after the
   write, a method call, an argument pass, a return, an assignment
   value, a read before the write in a loop body, a `new` initializer,
   a call initializer, and `this` in a value-class method.
4. `tsc` gate green with `w04` and `w05` included.
5. Full gate, `cargo fmt --check`, `tools/hygiene.sh` green. Clippy at
   the baseline 7 / 18 / 13.
