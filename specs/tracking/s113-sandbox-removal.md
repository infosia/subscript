# §113 — the sandbox profile is removed

Owner decision 2026-09-21. Contract: `specs/blocks/compiler.md` §113.
Plan Rev 4, phase P27.

## The decision

The owner removes the sandbox profile (§109) for two stated reasons:
the guarantee does not hold, and the cost to maintain it is large.
The owner also removes every host-set runtime limit: the interrupt,
the allocation quota, the stack budget, and
`subscript_rt_ctx_charged_bytes`, with trap kinds 25 to 27.

## Measured at `8f0a0c2`, before the removal

| Fact | Value |
|---|---|
| Tracked files that name the profile | 97 |
| Lines that name it, outside `specs/`, `docs/`, `generated-docs/`, `README.md`, `llms.txt`, `CLAUDE.md` | 641 in 74 files |
| `cli/src/compile_child.rs` | 1,287 lines |
| `compiler/src/check/profile.rs` | 1,010 lines |
| `runtime/tests/quota_peak.rs` | 1,493 lines |
| §109 contract text | 786 lines |
| Corpus entries with the `profile` header | 15 |
| Default-profile twins | 9 (`a235` to `a243`) |

## Findings of the inventory round

1. No compile thread existed at `d5b9519~1`, the commit before §109.
   The thread runs under the default form too, and
   `compiler/tests/nesting.rs` pins that a 2 MiB caller thread
   compiles a 40,000-level chain. §113.2 rule 1 keeps it.
2. `live_bytes` dates from `2fda4a9` (2026-07-26) as a walk. §109 made
   it a counter. A corpus golden prints it (`a163`). The counter stays.
3. With no quota set, `check_quota` answers true and
   `quota_headroom` answers `usize::MAX`. The removal therefore
   changes no script-visible behaviour of a program that set no limit.
4. The quota refusal was the one reader of the position parameter of
   `subscript_rt_cb_bind`, `subscript_rt_cb_register`, and the array
   sort. §113.1 rule 8 removes the parameter.
5. `codegen/tests/lir-goldens/corpus.txt` prints the intrinsic table
   in each of its 43 entries, so it loses 86 lines.
6. `tools/gate.sh` holds no expected skip count. The numbers of
   §109.6a were contract text only.

## The implementation rounds

Two rounds, split at the runtime so that the tree builds after each.
Round 1 removed everything above the runtime. Round 2 removed the
runtime limits, trap kinds 25 to 27, and the position parameter of
three calls.

- Round 1's quick gate failed `1578/2/2`. Cause: the handoff forbade
  index writes, and `codegen/tests/host_entry.rs` reads every `.rs`
  path of `git ls-files --cached`. A re-run under a substituted index
  wrote no verdict line and is void. The deletions were then staged.
- The inventory did not name `runtime/tests/quota_reserved.rs`. Round
  2 stopped on it before the gate, and §113.1 rule 9 now names it.
- The position parameter's premise held: `check_quota` was the one
  reader at each of the three sites.

## The full gate, before the Phase Review fixes

`gate full 963b1c37fbf798f19e9e5e0861ec9fa0e943bba2 dirty:138 debug 1546/0/2 release 1543/0/2 skips 2/0 debug-only 0 clippy 7/18/13 goldens-moved 16 exit 0`

The 16 moved goldens are `codegen/tests/lir-goldens/corpus.txt`, the
14 deleted `.expected` files, and `examples/sandbox/expected.txt`
(§113.4).

| Step | `38a9b9b`, 2026-09-19 | this tree | 
|---|---|---|
| debug tests, wall | 463 s | 377 s |
| release tests, wall | 501 s | 514 s |
| debug passed | 1,677 | 1,546 |
| release passed | 1,673 | 1,543 |

Both runs are on the owner's arm64 host. The heavy tests ran in the
debug step only, so the release step holds no removed heavy part.

## Phase Review

0 CRITICAL, 3 MAJOR, 14 MINOR. The MAJOR findings: an unconditional
`use std::io::Write` whose users are all `cfg(unix)`
(`codegen/src/jit/entry.rs`); the lost direct test of the inline rule
of `on_the_compile_thread`; and two `ReloadSession` constructors with
no caller that drop `RunConfig` fields. The review also found that
§113.1 rule 9 counted `a238` two times, and that §85 rule 4a named
the removed tests. Both are corrected in the contract.

## The fix round and the final full gate

One round closed the 3 MAJOR and the 14 MINOR findings. No premise
failed. `README.md` now states the measured interpreter counts: 176
entries in the debug profile, 177 under the full sweep, and 62
declared exclusions, 56 of them for a foreign call.

`gate full 963b1c37fbf798f19e9e5e0861ec9fa0e943bba2 dirty:151 debug 1545/0/2 release 1542/0/2 skips 2/0 debug-only 0 clippy 7/18/13 goldens-moved 16 exit 0`

The revision `963b1c3` in both verdict lines is the contract commit
before its last amendment; the amendments changed `specs/` only.
Record: `target/gate/20260921T083732Z-full.md`. Debug tests 342 s
against 463 s at `38a9b9b`; release tests 504 s against 501 s.
`codegen/tests/lir-goldens/corpus.txt` moved in three line classes:
the 86 `Sandbox` intrinsic lines, the two renamed `foreign` lines in
each of four entries, and the column of the parameters on those
mirror lines. `tools/hygiene.sh` is clean.

## Open

- The Linux and Windows gate runs after the landing (§113.5
  criterion 4). The `cfg` arms that went are in
  `cli/src/compile_child.rs` and `cli/tests/commands.rs`; the review
  read `codegen/src/jit/entry.rs` and eight more files by eye.
