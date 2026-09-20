# §112 Position id 0 is "no script site" — evidence

Contract: `specs/blocks/compiler/s112-position-id-0-is-no-script-site.md`.
Origin: `specs/tracking/s111-callback-registration.md`, "Open: position
id 0 names an unrelated site". Dates: 2026-09-19 to 2026-09-20. Host:
arm64 macOS.

## Red at the contract pin

The entries ran against the unchanged code, on both tiers.

| Entry | dev JIT | ship C | Expected |
|---|---|---|---|
| t61, the quota refusal of a binding | `allocation-quota at …:9:17` | `…:9:17` | `…:18:3` |
| t62, the quota refusal of a registration | `…:9:17` | `…:9:17` | `…:18:3` |

`9:17` is `main`, entry 0 of each tier's table. `18:3` is the
crossing. The quota is 32 bytes. `check_quota` compares the reserved
size of one charge, header and record bytes included, so the first
charge of the run passes 32 bytes on both memory modes. The entries
allocate nothing before the crossing, and an earlier charge reports a
different line and fails the entry.

## The table moved one consumer at a time

With the reserved entry in the dev-JIT table only, the differential
sweep failed on five cases, and all five were t60, t61, and t62: the
dev JIT reported `:0:0` and the ship tier `t60…ts:18:29` or
`t61…ts:9:17`. Every other trap entry agreed. That is the evidence
that a shift of the ids inside one tier moves no position.

## The literal 0 in the runtime

A scan of the call sites outside test modules, where the argument at
the callee's `pos_id` index is the literal `0`: 74 before, 71 after.

| Path | Answer |
|---|---|
| `Context::bind_callback` | takes the id of the crossing |
| `Context::register_callback` | takes the id of the crossing |
| `arrops::sort`, the quota check of the two copies | takes the id of the `sort` call; `subscript_rt_arr_sort` gained `pos_id` |
| `Context::validate_callback_userdata`, no header for the address | keeps 0, with the reason in a comment |
| `Context::registration_enter`, the refused fire of §111 rule 14 | keeps 0, with the reason in a comment |

The 66 other sites are outside §112 rule 4. Rule 1 stops each of them
from naming an unrelated site. They are: 31 `trap(…, 0)` sites, most of
them `TrapKind::Internal` reports of a defect in generated code, plus
the worker message decode and `WorkerTrapped`; helper reads inside one
runtime operation (`array_elem_ptr` 8, `assoc_receiver_is_live` 8,
`len_of` 6, `len` 6, `set_pair_is_live` 3, `delete` 2, `async_release`
2, `print_str` 1, `require_live_handle` 1, `alloc_str_from_view` 1);
and two worker-thread allocations, where the script site is on the
other side of the channel.

## What moved

Three listings print a raw position id, and each moved by the id
alone.

- `codegen/src/ship.rs`, the attribution test: each position id is one
  more. The `(class_id, file, line)` assertion of the same test, which
  resolves the ids through the table, needed no edit.
- `codegen/tests/fixtures/p21-allocation-metadata.inc`: one added row,
  `{ "", 0u, 0u },`, and the count 3 became 4. The three rows keep
  their text and their order.
- `docs/tutorial-c-cpp.md`, Step 6: `pos_id=0` became `pos_id=1` two
  times, and `positions=8` became `positions=9`. The coding agent built
  the demo from the tutorial's own commands and read the numbers from
  the run. Every other line of the transcript came back byte for byte.

No `.expected` file moved, the LIR snapshot did not move, and the gate
reports `goldens-moved 0`.

`p21-allocation-metadata.inc` and `p21-allocation-metadata.h` are
goldens of `emit_c` for `corpus/accept/a15-manual-lifetime.ts`.
`allocation_metadata_regenerates_byte_identically` in
`codegen/tests/allocation_metadata.rs` compares them byte for byte with
the emitter's
output, so a hand edit that differs from the generator fails the test.
The gap is smaller than this note first said: the generator exists, and
no capture path writes the two files. The LIR snapshot has one,
`SUBSCRIPT_CAPTURE_LIR_GOLDENS=1`, and its failure message names it.
The round took the block from the output of `subscript emit`.

*(Closed 2026-09-20.)* The test moved out of `codegen/tests/cemit.rs`,
which went from 2,879 to 2,858 lines. With
`SUBSCRIPT_CAPTURE_ALLOC_METADATA_GOLDENS` set, it writes the two
files; without it, it reads them at run time, and each failure message
names the file and the variable. Measured: a capture gives files equal
to the committed ones; with one byte of the `.inc` changed, the test
fails with no rebuild, and a capture restores the file. Cost: 0.005 s
for each run of the target, the mean of 20.

The same round closed a second gap. `tools/gate.sh` did not count a
moved file under `codegen/tests/fixtures/`, so the §112 verdicts above
say `goldens-moved 0` although the `.inc` moved. It did not count the
three `expected.txt` files under `examples/` either. §85 now holds the
list, and `cli/tests/gate.rs` holds one case for each form. Red with
the former filter: `goldens-moved 4` for the six golden lines of the
stub.

## Phase Review

A reviewer with no context read the change: 0 CRITICAL, 1 MAJOR,
7 MINOR. It found two table constructors and no merge, append, or
offset of a table; no consumer that adds, subtracts, or tests 0; the
three changed runtime entries equal in count, order, and type at every
declaration and call; and no `cfg` scope defect.

| Finding | Closure |
|---|---|
| M1: no test read the position of the sort refusal; `kind` and `pos_id` are adjacent arguments | A runtime test passes two call sites and a `kind` that equals neither. Red: a constant 0 gives `left: 0, right: 17`; the two arguments exchanged give `Internal` for `AllocationQuota`. Entry t63 pins each tier. Red: the dev lowering with 0 reports `:0:0` against the ship tier's `11:3`, and the reverse for the ship emitter. |
| m1: no tracking note | This note. |
| m2: the tutorial numbers | Measured from a rebuilt demo, above. |
| m3: the form did not carry the reservation | `PositionTable` in `codegen/src/position_table.rs`, 159 lines: private storage, `new()` places the reserved entry, `add` answers the id. `Default` calls `new()`. The five holders of a table hold the type, and the two hand-built test programs use the constructor. The renderer takes the type, so its empty case is gone. Moved in two steps with the trap sweep green between them (24.8 s, 26.4 s). |
| m4: the interpreter reported the instruction for "no script site" | `runtime_trap_site` answers `Site`, `NoScriptSite`, or `NoMatch`. The interpreter reports the reserved entry for the second and the instruction for the third. |
| m5, m6, m7 | The quota comment states what `check_quota` compares. The tutorial visitor prints `site=none` for the reserved entry; a control run that visits id 0 prints it. Two comments reworded. |

### The quota of t63, measured in one-byte steps

| Tier | The array's charges all pass from | The copies pass from | Reports the `sort` call |
|---|---|---|---|
| dev JIT, exact-size mode | 368 | 464 | 368 to 463 |
| ship C, arena mode | 256 | 448 | 256 to 447 |

The common window is 368 to 447, and the harness sets 400. Under 368
each tier traps inside the array literal.

## One event of the round

The coding agent ran `git checkout --` on
`codegen/src/lower/func/intrinsic.rs` to set up a Red proof and lost
that file's uncommitted hunk. It applied the hunk again. The
orchestrator read the diff of that file and of
`codegen/src/cemit/collection.rs` afterwards: each holds the `sort`
position argument and nothing else.

## Gates

Before the review fixes, `tools/gate.sh full`:

```text
gate full 38b4a0f83ca9226fe2ca75484faa1cbd57f63b15 dirty:26 debug 1675/0/2 release 1671/0/2 skips 2/0 debug-only 2 clippy 7/18/13 goldens-moved 0 exit 0
```

After the review fixes, `tools/gate.sh full`:

```text
gate full 661d3b584a9fd70793c6319295c03a335bdac5de dirty:33 debug 1677/0/2 release 1673/0/2 skips 2/0 debug-only 2 clippy 7/18/13 goldens-moved 0 exit 0
```

The `dirty` paths are this section's implementation, which the gate ran
before the commit. The x86-64 Linux host and the Windows host did not
run this section.

`tools/gate.sh full` with this round in the tree, before its commit:

```text
gate full 38a9b9b4396d681d42ea656031fc21dcf370ff5b dirty:5 debug 1677/0/2 release 1673/0/2 skips 2/0 debug-only 2 clippy 7/18/13 goldens-moved 0 exit 0
```

### The owed unix run: x86_64-unknown-linux-gnu

The unix reference host ran the full gate at `082657b`, the tip of this
work, with a clean tree:

```text
gate full 082657b96c483fdff8023280dafab4c3f3d68f98 clean debug 1677/0/2 release 1673/0/2 skips 2/0 debug-only 2 clippy 7/18/13 goldens-moved 0 exit 0
```

One run covers four sections, because each one landed inside this
range: §112, §111 (`specs/tracking/s111-callback-registration.md`), the
example e12, and the moved-golden list of §85. The counts equal the
counts that the arm64 macOS host measured at `661d3b5` and at
`38a9b9b`, so no test of this range excludes itself on unix.

Step wall seconds: fmt 2, build 49, debug 1407, release 711, clippy 15,
tsc 2, hygiene 0. The same host measured debug 1373 s at `afd51d7`,
where the debug step passed 1670 tests. The range adds 7 tests and
34 s.

The Windows host did not run this range.
