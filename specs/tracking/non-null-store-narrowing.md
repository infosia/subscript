# Tracking: a non-null store and a const global keep narrowing (§304)

Contract: `specs/blocks/compiler/s304-a-non-null-store-and-a-const-global-keep-narrowing.md`.
Origin: §162.4 items 2 to 4. On 2026-11-15 the owner told Claude to
continue with the open items.

## Red

At the contract pin `2e25e716`, the release CLI gives eight S011 for
`a460`, one in each row function (lines 12 to 19). The `node` golden is
eight lines: `2 4 2 2 1 1 1 1`. The JIT and ship C outputs are the same
bytes.

`tsc` 5.9.2: TS18047 once in each of `r512`, `r513`, `r514` (`h.f`),
and `r515` (`GL`).

## Lowering

A narrowed read of a module `const` global lowers to
`NarrowNonNull(Local)` with no `SharedNullNarrowing` site. Before the
change, it lowered to `NarrowNonNull(SharedRead)` with the runtime check
of §124.1 rule 3d. A module `let` global and a static field keep the
shared read. `codegen/tests/lir/const_global_narrowing.rs` pins both.

## Cost

`compiler/tests/fixtures/s161_gen.py`, release, three paired runs for
each binary, 22 rows: the largest ratio to the pin is 1.0135
(`narrowed`, 4000). `loop_edges` is 0.97 at each size.

## Phase Review

Two MAJOR findings, both realistic, fixed; see §304.3. The review
measured the reload defect with `subscript run --watch`: a swap from
`let G` to `const G` with a lambda from before the swap that stores
`null` gave exit status 139.
