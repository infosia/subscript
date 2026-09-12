<!-- §17 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 17. P16 — generated API reference

Owner decision 2026-07-25. The language's accepted surface is decided
in `collisions.md` (Q-register) and `stdlib.md`, but those are prose:
they are written by hand and can drift from the checker. That is not
hypothetical — the P12 review found a Q25 entry recording a divergence
that did not exist in the implementation, goldens or tests
(`specs/tracking/p9-stdlib.md`, P12 CRITICAL 1). This phase makes the
reference **derived** instead of written, so drift is impossible in the
generated part and demonstrable in the rest.

It is also the answer to the reader's real question. A TypeScript
developer does not need "what does subscript have"; they need **"which
of what I already know still works, and where does it behave
differently"** — because `tsconfig.json` loads the ES2022 lib (§0.1 of
`stdlib.md`), so the editor completes members this language rejects.
The generated reference is where that gap is stated.

### 17.1 Source of truth

The generator reads the **checker's own tables** — the ambient surface
(`compiler/src/ambient.rs`) and the per-type member tables the checker
consults — never the specs. Whatever the compiler accepts is what the
document says it accepts. Where a table needs an entry the generator
cannot infer (a human-readable summary line), the entry lives beside
the table in the checker, not in a parallel file.

### 17.2 Output

One generated document (Markdown; path the implementer's choice under
a generated-docs directory), carrying a do-not-edit header naming the
generator, with three parts:

1. **Accepted surface** — every ambient function, namespace member and
   type member the checker accepts, with its **subscript** signature
   (sized types, not `number`), grouped by receiver/namespace.
2. **Rejected surface** — every member the checker explicitly rejects,
   with its **S-code**, the **Q-rule** that rejects it, and the
   replacement where the contract names one (`findIndex` for `find`,
   `getOr` for a scalar `get`, `Number.isNaN` for the global). This is
   the part no hand-written document keeps current.
3. **Divergences from ECMA** — members this language accepts but whose
   *result* differs from JS, each naming the Q entry that records it.

### 17.3 A recorded divergence must be demonstrable

Every entry in part 3 carries an **executable witness**: a program
fragment plus the two results (this language's, and JS's). A test runs
each witness through the language and through `node` (already present
for the `tsc` gate) and **fails if they agree** — a divergence that
cannot be demonstrated is a spec error, and that is exactly the error
P12 shipped into `collisions.md`.

This checks one direction only. Divergences that exist but are *not*
recorded are found by adversarial sweeps in a Phase Review, not by a
standing test; the reference states which surfaces have been swept and
when, so an unswept area is visible rather than implied to be clean.

### 17.4 Regeneration is the gate

Running the generator on the current checker reproduces the committed
document **byte-for-byte**; a drift fails the test, exactly as
`bindgen`'s mirror does (§12.2). The document is never hand-edited —
CLAUDE.md core principle 6. A member added to or removed from the
checker without regenerating is therefore a build failure, not a stale
paragraph.

### 17.5 Gate (pre-registered)

Byte-identical regeneration test green; every accepted member in the
document is one the checker accepts and every rejected member is one it
rejects, asserted by construction rather than by review; every
divergence witness demonstrably diverges from `node`; the reject
S-codes in the document match the reject corpus's pinned codes; `tsc`
and the standing gate unaffected (this phase adds no language surface);
benchmarks not required (no runtime change).

### 17.6 Out of scope

The editor's over-promise — `tsserver` completing `Math.imul`,
`arr.find`, `str.substring` because the ES2022 lib declares them — is
**not** addressed here. Narrowing what the editor offers means shipping
a reduced `lib` for authoring while the `tsc` gate keeps the stock one
(§0.1), which is a separate design question. This phase makes the gap
*documented*; closing it is later work.
