# Tracking: a conditional or spread element takes part in the list type (§305)

Contract: `specs/blocks/compiler/s305-a-conditional-or-spread-element-takes-part-in-the-list-type.md`.
Origin: §248.4 items 1 and 2. On 2026-11-15 the owner told Claude to
continue with the open items.

## Red

At the contract pin `a015e518`, `a461` gives twelve S100: four branch
mismatches, two spread element mismatches, and six argument-count
diagnostics that follow from them.

## Output

The development tier, the ship tier, the interpreter, and `node` give
the same eleven lines (`none`, `one 2`, `none`, `nv 2`, `none`,
`one 3`, `none`, `one 5`, `none`, `one 7`).

## Phase Review

One MAJOR finding, fixed; see §305.3. The review measured the nested
list `[[nvoid], [nvoid, b ? nv : nv2]]`: the pin prints `nv 4`, round 1
gave S100, and `tsc` accepts it.
