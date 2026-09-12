<!-- §34 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 34. R11 — parameter-position handle-element pairs

Owner decision 2026-08-01 (downstream request R11, blocking its
encoder area). The last cell of the pair matrix: §27 collapses
parameter pairs with scalar elements, §31 struct-level pairs with
handle elements; parameter pairs with handle elements did neither —
and mirrored the R7.2-class misleading split (leaked count, array
pointer as `H | null`).

### 34.1 The rule

Adjacent parameters `size_t <name>Count, const H* <name>` with a
registered-handle `H` collapse to `<name>: H[]`, count elided, §27
input-direction semantics; a mutable (`H*`) parameter pair fails
loud as input-only (§31.1's rule at parameter position). And the
class closes: the **"every emitted array field is a collapsed
pair" audit extends to parameter positions** — no pair position
anywhere, struct or parameter, any element kind, may mirror as a
bare count beside a nullable-value pointer.

### 34.2 Corpus

`a107` (accept): the queue-submit shape — fixture-created handles
submitted as an array beside a leading handle parameter; checker
returns the count and per-element identity evidence. Bindgen unit
tests: the collapse on the verbatim evidence signature; mutable
handle parameter pair fails loud; the extended audit.

### 34.3 Exit criteria (pre-registered)

1. `a107` byte-identical under both tiers.
2. The evidence signature mirrors
   `(queue: SGPUQueue, commands: SGPUCommandBuffer[])` — no count,
   no `| null` — bindgen-unit-tested.
3. The parameter-position audit holds; a mutable handle pair at
   parameter position fails loud.
4. No existing golden moves; full gate, `tsc` gate, zero-warning
   sweep, and the generated-docs gates green.
