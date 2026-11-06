<!-- §224 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 224. A callback outside a host entry is a host entry

*(Added 2026-11-06.)* Origin: §223.4 item 1. The owner selected this
section on 2026-11-06.

Problem, read at `7720d09a`: a callback can outlive its registration
(§13, §14). The host can call it from its own loop, when no host entry of
§223 runs. The host-entry flag is then lowered. A foreign call in that
callback can call an export, and the export runs with no trap. This is
the hazard of §223.

### 224.1 Rules

1. If C calls a script callback while no host entry of its Context runs,
   that callback is a host entry for the length of its run. It raises the
   flag of §223 at entry and lowers it at each return.
2. If C calls a script callback while a host entry runs, the callback
   does not change the flag. §223 rule 5 stays.
3. A host entry that C calls during such a callback traps, as §223 rule 2
   states.
4. A callback that C calls during such a callback runs, as rule 2 states.
5. The cost is one flag read per callback call. A callback outside a host
   entry also writes the flag twice.
6. The development tier binds the same trampolines, so rule 1 applies.
   Its exports do not raise the flag, so each dev-tier callback raises it
   for its run. During a dev-tier callback, a C call of
   `subscript_rt_ctx_async_step` or `_budget` traps. *(Amended 2026-11-06
   after the Phase Review.)*
7. §223.4 item 1 cites this section.

### 224.2 Acceptance

1. Red first, at `7720d09a`: a test host stores a callback, and calls it
   after the outer entry returns. A foreign call in the callback calls an
   export. The export runs. The tracking note records the output.
2. After the change, the export of acceptance 1 gives the trap of §223
   rule 3. The callback stops when the foreign call returns.
3. Controls: the same stored callback with no foreign call runs. A
   callback during a foreign call runs (§223). After the callback
   returns, an export from the host runs, so the flag is lowered.
4. If the trampoline of a callback has no route to its Context, report it
   and stop. Rule 1 then needs a contract change.
5. No golden moves.

### 224.3 Phase Review

*(Added 2026-11-06.)* A fresh review of `14b171a1` and the implementation
gave four MINOR findings and no CRITICAL or MAJOR finding.

1. MINOR, realistic, fixed: rule 6 said that the development tier has no
   change. It binds the same trampolines.
2. MINOR, realistic, fixed: the header comment stated §223 only.
3. MINOR, contrived, fixed: a trampoline comment said that the caller of
   the foreign call unwinds. A stored callback has no such caller.
4. MINOR, realistic, open: only a unit test covers rule 4. No C host
   test calls a callback during a stored callback.
