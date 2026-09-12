<!-- §75 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 75. Four LIR facts the form carries once

*(Owner decision, 2026-08-30; review findings.)*

### 75.1 A fresh async owner is a bit on the value

**Rule 1.** The kinds of expression and of LIR instruction that produce a
fresh async owner (§70.3) are two tables, each written once next to
its enum: `hir::ExprKind::produces_fresh_async_owner()` and
`lir::InstructionKind::produces_fresh_async_owner()`. The lowering sets
`fresh_owner` on the LIR value when it emits the instruction; the
verifier reads the bit and checks it against the instruction table. No
other function in `codegen/` lists the kinds.

Measured before the rule: three tables (`lir.rs`
`expr_produces_fresh_async_owner`, `operand_is_fresh_owner`,
`fresh_owner_index`); the verifier seeded `ArrayWithCapacity` and every
parameter, the lowering did not.

### 75.2 An embedded header is a class flag

**Rule 2.** `lower_classes` sets `l::Class.is_embedded_header` once,
with the rule of §33.5 rule 9 (a value boundary class whose `T | null`
is a link field or foreign parameter, and that is the first field of a
value boundary class). Every consumer — the store rule, the verifier,
both transcribers — reads the flag.

Measured before the rule: four derivations; `lir_types.rs` required the
first-field condition and the verifier did not, so the verifier accepted
a form the transcribers treated as a plain box.

### 75.3 One boundary-box predicate

**Rule 3.** `lir_types::boundary_box_class(module, ty) -> Option<ClassId>`
answers "is this `T | null` with `T` a value boundary class" once. Six
sites called their own copy; two tested one of the two conditions only
(`lower/func.rs` at `f99d4cb` lines 1695 and 5029). The round that lands
this rule reports, for each of the two, the input on which the old and
the new answer differ, and whether any corpus entry reaches it.

### 75.4 The interpreter checks storage classes; the verifier keeps no copy

**Rule 4.** `verify_local_storage_classes` is deleted. It was the
lowering's classification walk with the result negated, and it did not
fail (core principle 9). The independent witness is the interpreter: at
every `Suspend` it poisons every local of storage class `Activation`
(§68.2 rule 7a), and a `LoadLocal` or `AddressOfLocal` of a poisoned
local is an interpreter error that names the local and the suspend. A hand-built LIR test
constructs the violating form (an `Activation` local read after a
suspend with no intervening store) and checks the interpreter reports
it.
