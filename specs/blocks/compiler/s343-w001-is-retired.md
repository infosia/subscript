<!-- §343 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 343. W001 is retired

*(Added 2026-11-21.)* Origin: the Phase Review of §342 and the owner
decision of 2026-11-21 that `Context.free` stays.

Problem, measured at the §342 landing:

1. **The premise of W001 is false in the default mode.** W001 states
   that the Context grows per iteration until a collection
   (`warnings.md` §2). In mode 1 a loop that allocates has a poll at
   its backedge (§342 rule 11a). The `tree` workload without its
   `free` function keeps live bytes under three trees in each tier
   (§342.4a).
2. **The warning asks for the manual level.** Its two mutes are a
   `Context.free` of the binding and a `Context.collect()` in the
   function. A program at the automatic level needs neither, and
   `Context.free` at a wrong point is undefined (`collisions.md` Q6).
3. **The compiler does not know the level.** The host sets the mode at
   run time (§342 rule 17). A text that names mode 0 fires on each
   program of the default level that allocates in a loop.

### 343.1 Rules

1. **The compiler does not report W001.** The analysis of the
   unreleased loop allocation, its two mutes, and its message go.
2. **The number stays reserved.** No later warning takes the code
   W001 (`warnings.md` §2: codes are stable). `WarnCode` has no W001
   variant; the full-enum tests list W002, W003, and W004.
3. **W003 keeps its own rule.** W003 fires on a callback-info
   aggregate that a loop body constructs with a userdata slot that
   holds an allocation of the same iteration. The part of the W001
   analysis that W003 uses stays, under W003's name. W003 does not
   change its results.
4. **W002 and W004 do not change.**
5. **The corpus entry goes.** `corpus/warn/w01-loop-allocation-unreleased.ts`
   is removed from the firing entries. Its program becomes a test of
   the non-firing net: `check` reports no warning and exits 0.
6. **The sample of the warning form is W002.** The CLI rendering test
   (`warnings.md` §6 item 3) and the C tutorial show a W002 report.
   The three tutorials state three codes. The escape rule of §115.2
   (a thrown object leaves the iteration) had W001 as its only
   reader, and its test goes.

### 343.2 Acceptance

1. The program of the removed entry: `subscript check` prints
   `no errors` and exits 0, and `--deny-warnings` exits 0. Red at the
   §342 commit: one W001 report, and `--deny-warnings` exits 1.
2. Firing control in the same shape: the same loop with a callback
   registration of the fresh object reports W003 (`w03`).
3. `corpus/warn/` harness: `w02` to `w05` report their pinned code
   and line.
4. Zero warnings over `corpus/accept/` and `examples/*.ts`.
5. `tools/gate.sh full` passes.

### 343.3 Sections this one amends

- `warnings.md` §1, §2 (W001 and the W003 reference to it), §3, §4,
  and §6.
- `collisions.md` Q6 and Q7 do not change.

### 343.4 Phase Review

Form. `compiler/src/warn.rs` has no W001 analysis. `WarnCode::ALL` is
W002, W003, and W004. The functions that W003 uses did not change;
`is_reference_allocation` joined `is_reference_new_allocation`, which
gives the same result, because the Map and Set branch of the first
could not apply under the guard of the second. The program of the
removed entry is an inline test program in `cli/tests/commands.rs` and
in `compiler/src/warn/tests.rs`: an accept entry needs a golden, a
three-tier run, and an index row, and the net needs one silent program.

Measured.

| Item | Result |
|---|---|
| The program of the removed entry, CLI built from the §342 commit | one W001 report; `--deny-warnings` exits 1 |
| The same program, this section | `no errors`; exit 0 with and without `--deny-warnings` |
| `corpus/warn/` | `w02`, `w03`, `w04`, `w05` report their pinned code and line; the `w03` pin did not move |
| Size | 138 lines added and 626 removed in the first implementation |

Phase Review (a subagent with no context): no CRITICAL, no MAJOR, 4
MINOR, each closed.

| Finding | Close |
|---|---|
| The CLI sample ran a use after `Context.free` and pinned its output (Q6: undefined) | the free and the use are in a function that `main` does not call |
| The C tutorial gave an exit code for `run` on a program that reads after the free | the sentence gives `check` and `build`, and names Q6 for `run` |
| `Expr::flow_leaves` and one parameter of `walk_statements` had no reader | removed |
| History wording in a test comment | corrected |

`tools/hygiene.sh`: exit 0. `tools/gate.sh full` on the tree before
this record: exit 0, debug 3905 passed, release 3886 passed, 0 failed,
5 ignored in each. No golden moved.

