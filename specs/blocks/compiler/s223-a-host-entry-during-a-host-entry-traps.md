<!-- §223 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 223. A host entry during a host entry traps

*(Added 2026-11-06.)* Origin: §221.4 item 1. On 2026-11-06 the owner
selected this section and chose to forbid the call.

Problem, read at `08a806fc`: §18 states no rule for a call of an export
while a foreign call runs. C can call `subscript_export_<name>` from a
foreign function. That export can move an array while a `@ValueType`
method on an element of that array waits for the foreign call to return.
`this` then reads old storage, with no diagnostic. §221 rule 10 gives the
reach no edge to an export, so the build does not reject this form. An
edge from each foreign call to each export rejects each element method
that calls C if any export moves that array.

### 223.1 Rules

1. A host entry is `subscript_init`, each `subscript_export_<name>`,
   `subscript_rt_ctx_async_step`, and `subscript_rt_ctx_async_step_budget`.
2. If a host entry of a Context runs, a call of a host entry of the same
   Context traps. Script code calls its own functions directly, so this
   call comes only from C during a foreign call, or from a callback that
   C calls during a foreign call.
3. The inner call runs no script code. It sets the trap kind
   `SUBSCRIPT_RT_TRAP_HOST_ENTRY_REENTRY` and a message that names the
   inner entry, and it returns.
4. When the foreign call returns, the outer entry stops with that trap.
   This is the path of a trap that a callback sets during a foreign call.
5. A callback that C calls during a foreign call stays allowed. §221
   rule 10 covers it.
6. The development tier has no change. Its host calls an export through
   `&mut` of the reload session, so a call during a foreign call has no
   route.
7. Each host entry reads one flag and writes it twice. A foreign call has
   no added cost.
8. The comment on the host calling convention in the C header states this
   rule. `docs/tutorial-c-cpp.md` states it.
9. §221 rule 10 cites this section. The reach needs no edge to an export.

### 223.2 Acceptance

1. Red first, at `08a806fc`: a test host calls an export from a foreign
   function during a foreign call. The inner export runs. The tracking
   note records the output.
2. After the change, the same test gives the trap of rule 3. The inner
   export runs no script code, and the outer entry stops.
3. Controls: the same foreign function calls the export after the outer
   entry returns, and the export runs. A callback during a foreign call
   runs.
4. A call of `subscript_rt_ctx_async_step` during a foreign call traps.
5. If the outer entry does not stop when the foreign call returns, report
   it and stop. Rule 4 then needs a contract change.
6. No golden moves.

### 223.3 Phase Review

*(Added 2026-11-06.)* A fresh review of `acd866ef` and the implementation
gave one MAJOR and three MINOR findings.

1. MAJOR, realistic, fixed: `codegen/tests/cemit.rs` had 2,003 lines.
2. MINOR, realistic, open: §223.4 item 1.
3. MINOR, realistic, fixed: no test called `subscript_init` or an export
   with parameters during a foreign call.
4. MINOR, fixed: the trap defines of the header were not in numeric
   order.

### 223.4 Open

1. *(Closed by §224.)* A callback can outlive its registration (§13, §14). If the host calls
   it from its own loop, no host entry runs. A foreign call in that
   callback can then call an export with no trap. Rule 1 does not list a
   callback as a host entry.
