# §111 A callback registration with an explicit end — evidence

Contract: `specs/blocks/compiler/s111-a-callback-registration-with-an-explicit-end.md`.
Date: 2026-09-19. Host: arm64 macOS.

## The problem, read from the code

`Context::bind_callback` appends one record for each new
`(code, env, userdata1, userdata2)` identity, and no code removed a
record before the Context ended. The mark phase of `Context::collect`
roots every userdata slot of every record. A boundary callback does not
capture, so a host that starts N one-shot requests registers N distinct
userdata objects, and each stays allocated. No measurement round ran:
the code shows the growth.

The firing control of the retention test gives the number. At
N = 10,000, the Context-lifetime path grows the charge by N records of
40 bytes and keeps 2N allocations after `collect()`.

## Rounds

| Round | Scope | Result |
|---|---|---|
| 1, 1b | runtime: the record, the live set, the second trampoline, the release entry, trap kind 28 | green |
| 2 | binder option, directive, CLI, the `callback_lifetime` field of HIR and LIR | green |
| 3 | rule 5a, the fixture, a244 to a248, t59, t60, the dev-JIT and ship-C lowering, the reload test | green after one stop |
| review fixes | 4 MAJOR, 10 MINOR | green |

Round 3 stopped at the fixture with no change to the tree. A host
function that a script calls receives no Context, so the fixture could
not name the Context of the release call on the dev tier. The contract
gained rule 5a, `subscript_rt_cb_registration_context`. A harness-only
host hook was refused, because a host with more than one Context has the
same gap. A release with no `ctx` parameter was refused, because the
release then reads the pointer before it tests membership.

## Contract corrections the rounds forced

- Rule 5 returns `int32_t`. `bool` put `<stdbool.h>` into the host
  header and into every emitted C preamble.
- Rule 14 traps. The first implementation returned with no report from a
  fire through a released registration.
- Rule 6 counts down only a call that it counted.
- Rule 1 names four bind errors and the position of the directive.
- §111.3 names `a90-callback-userdata-rooted` as the precedent, and the
  entries carry `// interpreter: no`.

## Retention, §111.4 criterion 2

`ten_thousand_registrations_return_their_charge_and_their_userdata`,
N = 10,000. After N registrations and N releases with no collection,
the live registration count is 0 and the charge equals its value before
the registrations. After one `collect()`, `charged_bytes` equals the
value of a fresh Context. The record is 48 bytes.

| Step | Debug | Release |
|---|---|---|
| build | 25.7 ms | 3.67 ms |
| register | 4.74 ms | 1.18 ms |
| release | 4.21 ms | 0.64 ms |
| collect | 4.56 ms | 0.96 ms |
| control: bind | 13.0 ms | 2.58 ms |
| control: collect | 12.3 ms | 1.64 ms |

Each N-sized loop runs one time.

## Red at the contract pin

The entries ran with the lowering unchanged, so `SubRequestInfo` crossed
on the Context-lifetime path and the release answered 0.

| Entry | Expected | Measured at Red |
|---|---|---|
| a244 | `released 1`, `reclaimed 1` | `released 0`, `reclaimed 0` |
| a245 | `released 1`, `released 2` | `released 0`, `released 0` |
| a246 | `released 1` | `released 0` |
| a247 | `released 1`, `released 1`, `released 2` | `released 0` three times |
| a248 | `released 1` | `released 0` |
| a249 | `released 1`, `released 2`, `released 3`, `reclaimed 1` | `released 0` three times, `reclaimed 0` |
| t59 | `released 1` before the trap | `released 0` |
| t60 | the trap `callback-registration-ended` | no trap; the callback ran two times |

a244 compares charged bytes with a margin. With 1,024 `i32` elements
the measured fall was 4,032 to 4,064 bytes against a payload of 4,096,
because the strings of the entry are live at the comparison. The entry
holds 16,384 bytes and asks for a fall of 8,192.

## A tier disagreement that the round found and closed

With trap kind 28 mapped by name only, both tiers trapped, and they
reported two positions: the dev JIT `t60…ts:12:5`, the ship tier
`t60…ts:18:29`. Each tier resolved position id 0 through its own table,
and the two tables are built in different orders. The four sites that
resolve a position now share one mapping, and a kind with no script
site reports the empty position on each tier.

## Phase Review

A reviewer with no context read the whole change: 0 CRITICAL, 4 MAJOR,
10 MINOR. It found no use after free and no `&mut Context` alias on a
production path.

| Finding | Closure |
|---|---|
| M1: a246 cannot see rule 6 | Measured. With a release that ends the record at once, a246 prints the same two lines on both tiers, and the runtime unit test fails. The contract now states what a246 pins. The same prototype made the rule 14 test read a freed record (`misaligned pointer dereference`, SIGABRT), so rule 6 is what keeps the certain case of rule 14 defined. |
| M2: no test reached a second release of a closed record that a call keeps | A new test releases two times inside the callback: 1, then 0. It fails when the branch is deleted. |
| M3: the fixture lost a registration under a chained start | The adapter holds a table of four pending one-shots for each device and finds a slot again by ticket after a fire. A full table and a second subscription end the registration at once and answer -1. Entry a249 and four adapter tests, each Red against the former fixture. |
| M4: a selection in a header with no function | Already a loud bind error. A test pins it, with a firing control. |
| m1, m3, m5 to m10 | Fixed. |
| m4 | The contract now states how the LIR snapshot moves. |

## Open: position id 0 names an unrelated site (core principle 12)

Measured by a probe under the sandbox profile, with a quota that the
callback-info crossing is the first charge to pass (8, 16, 32, 64
bytes), through `register_callback` and through `bind_callback`: both
tiers print `AllocationQuota at prog.ts:1:17`. That is the position of
`main`, which is entry 0 of each tier's position table. A runtime path
that records position id 0 to mean "no position" therefore reports the
first entry of the table. The two tiers agreed in this probe because
their tables share entry 0, not because a rule makes them agree; t60
showed that they can differ. The class is older than this section, and
this section closed it for kind 28 only. No golden pins it. It needs a
section of its own: the form cannot say "no position".

The same probe showed one more fact. At a quota of 64 the dev tier
traps at the `print` string and the ship tier runs clean at 128. The
two memory modes charge different bytes for one allocation (§18.2d),
so this is not a position defect.

## The moved snapshot

`codegen/tests/lir-goldens/corpus.txt` moved. The fixture gained one
class and ten foreign functions, and the mirror gained one directive
line, so class ids, field ids, foreign ids, and mirror line numbers
moved. With those normalised, the diff removes 0 lines and adds 120.
No `.expected` file of an existing entry moved.

## Gates

Before the review fixes, `tools/gate.sh full`:

```text
gate full 5b6bc739354441a10a2a24b334cf21b59818c3f3 dirty:66 debug 1663/0/2 release 1659/0/2 skips 2/0 debug-only 2 clippy 7/18/13 goldens-moved 1 exit 0
```

After the review fixes, `tools/gate.sh full`:

```text
gate full e490079cbd2bde2c83861ec8d88e655db40084e3 dirty:69 debug 1670/0/2 release 1666/0/2 skips 2/0 debug-only 2 clippy 7/18/13 goldens-moved 1 exit 0
```

The `dirty` paths are this section's implementation, which the gate ran
before the commit. The moved golden is the LIR snapshot above. The
x86-64 Linux host and the Windows host did not run this section.
