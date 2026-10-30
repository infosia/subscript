<!-- §211 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 211. A `switch` with no case test is a branch

*(Added 2026-10-30.)* Origin: the §210 Phase Review.

Problem, measured at the pin `c7bb7e55`: this program passes the
checker and the dev tier prints `3`. The ship tier does not compile.

    function f(s: string): i32 { switch (s) { default: return 3; } }

`cl` gives error C2050, because the emitted C is `switch (_disc)` over a
`void*`. In `codegen/src/lir/stmt.rs`, the test `all_constant` is true
for an empty list of tests. So the lowering writes a `Switch` terminator
for a string value, and the branch path for no tests is dead code.

1. A `switch` with no case test lowers to a branch to its default. The
   value is still evaluated once, for its effects, before the branch.
2. The `Switch` terminator holds at least one arm. The LIR verifier
   rejects a `Switch` with no arm.

Corpus: a new accept entry with a `default`-only `switch` over a
string and over an `i32`, with an effect in the scrutinee. The entry is
Red at the pin on the ship tier.

Exit criteria: the entry fails at the pin and passes after the landing
on the dev JIT, the ship C, the interpreter, and the golden, byte-exact.
`tsc` exits 0. A test builds a `Switch` with no arm and finds the
verifier rejection. `tools/gate.sh full` is green.
