<!-- §297 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 297. An invalidation list names only values that dominate it

*(Added 2026-11-15.)* Origin: §273.4 item 1. On 2026-11-15 the owner
told Claude to do each item of a list of open runtime defects, in an
order that Claude selects. This item is one of them.

Problem, read at `51cc055e`. An instruction of the LIR can carry an
invalidation list (`invalidates` in `compiler/src/lir.rs`): the array
values whose element addresses the instruction can make stale, for
example by a call that can grow the array. The LIR verifier
(`verify_address_invalidation` in `codegen/src/lir/verify_dominance.rs`)
reads the list, and rejects the use of an address after an
instruction that invalidates its base. §273.4 item 1 records that in
`a341`, function `arrays`, block `b10`, the list of an instruction names
a value that does not dominate the block, before and after §273.

A value that does not dominate the instruction is not a value at that
instruction. The verifier then compares the base of an address against
a value that can stand for another array, or for no array, on that
path. So the check of an address whose base is the value that does
dominate there (for example a merge parameter of the array) does not
see the invalidation. The verifier does not check the list itself.

### 297.1 Rules

1. Each value in the invalidation list of an instruction dominates that
   instruction.
2. The lowering builds the list from the values that are in scope at
   the instruction. Where a suspension or a loop gives an array a new
   version (§273 rule 1), the list names the version that reaches the
   instruction.
3. The LIR verifier rejects a list that names a value that does not
   dominate its instruction, with a message that names the function,
   the block, and the value. A test builds the violating form (core
   principle 9).
4. No program changes its output.

### 297.2 Acceptance

1. Red first, at the contract commit: with rule 3 only, the verifier
   rejects `a341`, and the report names the function, the block, and the
   value. (Measured: the verifier rejected 86 corpus entries, `a341`
   among them.) The tracking note records the text and the cause, found from
   the lowering.
2. After the change of rule 2, the verifier accepts each corpus entry.
3. A unit test of the verifier builds an instruction whose list names a
   value from another branch, and the verifier rejects it. A control:
   the same list with a dominating value is accepted.
4. If the LIR text snapshot changes, the changed lines are invalidation
   lists only, and the report names them. No corpus golden changes.
5. The full gate passes.

### 297.3 Phase Review

One fresh review of the implementation. No CRITICAL finding, and no
MAJOR finding. The review read the soundness of the narrower list: the
base of an address dominates the address, and the verifier reads the
list of an instruction only on a path from the address to its use, so
the address and its base dominate that instruction and stay in the
list. The §273 pass renames the list and the address bases in one walk.
Each of the 222 changed snapshot lines only removes values.

1. MINOR, contrived, fixed: rule 3 did not check the list of a
   `Suspend` terminator.
2. MINOR, realistic, fixed: no lowering-level test held a list after a
   branch, a loop, or a §273 version. A test now lowers source with
   each, and derives the expected list from the definitions.
3. MINOR, realistic, fixed: the message of rule 3 read as an operand
   dominance failure. It now names the invalidation list.

### 297.4 Open

None.
