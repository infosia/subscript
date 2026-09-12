<!-- §77 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 77. Two runtime facts written once

*(Owner decision, 2026-08-30; review findings.)*

**Rule 1.** `TrapKind` has one `message(&self, ...)` per kind in
`runtime/`. Every runtime site that raises the trap and the emitted-check
entry `subscript_rt_trap` produce the message through it. The golden
corpus pins the text.

Measured before the rule: `ffi.rs` `subscript_rt_trap` held literal
copies of messages that `context.rs` also spelled (`index {i} out of
bounds for array length {n}`, `use of a deleted allocation`).

**Rule 2.** Element equality (stdlib §9 `===`, §10 SameValueZero) and
the integer-width read are one module, used by `arrops` and `assocops`:
`value_eq(kind, width, p, q, same_value_zero)` and one `read_uint`. A
unit test enumerates every kind and width and checks the expected
answer for each (`NaN` unequal under `===`, equal under SameValueZero;
strings by content; handles by identity).

Measured before the rule: `arrops.rs` `elem_eq`/`elem_same_value_zero`
and `assocops.rs` `keys_equal` were two tables; `F16` existed in one.
