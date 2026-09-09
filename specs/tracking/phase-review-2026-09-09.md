# Phase Review — §96 to §100

Two fresh no-context reviewers covered `a3d4ba7..7d8ac2f`, 63 files and
3,498 lines, split by area. **No CRITICAL.** Four MAJOR, thirteen
MINOR.

## The four MAJOR findings

1. **§89.1 rule 3 was still an active rule that §99 deleted.** It told
   an implementer to reject above 65,000 bytes with S019 and to add a
   `collisions.md` row, for a code the compiler no longer has. §0's
   stage index points at §89 for the ship tier. Corrected at
   `0eac01e`.
2. **§97.4 item 8's report was never produced, and its missing half
   was live.** A `switch` whose every arm returns was treated as
   falling through, so the walk demanded facts for statements no path
   reaches. In the §97 shape it demanded three trap sites where the
   LIR carried two. §97.4 item 8a and `a205` close it.
3. **§100.2 rule 3's total check was not total.** It matched the byte
   string `int main(void)` and read two directories. `int  main(void)`
   and `int main( void )` both compile as C and bypassed it, and
   `benchmarks/` and `examples/` could write unread host bodies.
   §100.2 item 3a and `codegen/src/host_source.rs` close it.
4. **§96.1 rules 1 and 4 disagree with the fork.** **Closed
   2026-09-10**; see below.

## MAJOR 4, closed 2026-09-10

The fork pairs surrogate escapes by **spelling**, not by value:

```rust
let low = if !is_curly && value <= 0xdbff {
    self.input.as_str().strip_prefix("\\u").and_then(|rest| rest.get(..4))
```

so three spellings that TypeScript accepts are rejected. Measured on
this host against node v24.18.0, which reads all four as `👍`:

| source | subscript | node |
|---|---|---|
| `"👍"` | accepted, 4 bytes | `👍` |
| `"\ud83d\u{dc4d}"` | S100 | `👍` |
| `"\u{d83d}\udc4d"` | S100 | `👍` |
| a line continuation between the halves | S100 | `👍` |

The diagnostic says the escape "has no UTF-8 encoding". For the last
three that is false: they denote U+1F44D, which encodes as F0 9F 91
8D. The remedy it names, "write the paired escape", is what the author
already wrote. §96 exists to close undecided divergences; this is one
a level below the rule §96 wrote.

The fix belongs in the fork again, by the same argument §96.2 made:
pairing is the lexer's job and this compiler cannot redo it without a
second escape decoder.

## A pre-existing defect the round found

An accepted program fails LIR lowering. Measured here at `0eac01e`:

```ts
class R { [Symbol.dispose](): void {} }
function run(): void {
  using resource: R = new R();
  for (;;) { return; }
  print("after");
}
```

```
subscript: internal lowering error: LIR construction failed:
produced invalid LIR: function 1 (`run`): use of value 0 in block 4
is not dominated by its definition
```

exit 2.

**It is not a §97 regression.** The nullable spelling fails the same
way, and so does the non-nullable one; a program with no `using`
compiles. The trigger is a `using` binding whose scope contains a
conditionless `for` that returns. §90 asks that no public entry point
fault on any input; this is a clean internal error rather than a
panic, but a valid program cannot be compiled. Contracted as §101 on 2026-09-09,
with the shape measured across nine loop forms.

## What the reviewers confirmed by running

- **§99 has no fourth emission site.** A structural argument over
  `compiler/src/lir.rs`, plus 24 emitted program shapes scanned for
  any C literal group above 3,000 characters. Every language string
  above the threshold took the array form.
- **§97's guard is checker-complete.** The synthesized guard and a
  hand-written `if (s !== null)` have identical HIR node shapes, and
  the receiver is narrowed through the established `Coerce`, not
  relabelled. `a43bd8b` touches no file under `codegen/src/`,
  `runtime/src/` or `prelude/`.
- **§98 masks identically on all three tiers** for three operators and
  three compound forms at eight widths and four counts, 32 rows.
- **No test that cannot fail** was found in either scope.

## §101 landed

The pre-existing defect above is fixed. Gate verdict:

```
gate full 801dbe1a7ed7317d3881115c2f99cfc52a2595e1 dirty:20 debug 1378/0/2 release 1376/0/2 skips 2/0 clippy 7/18/13 goldens-moved 0 exit 0
```

### Why the two infinite loops differed

The round answered it before changing anything. The checker keeps both
conditions intact and removes the trailing disposal from neither.
`codegen/src/lir.rs` differs: `lower_while` emits a conditional branch
even for a literal `true`, so `while.exit` carries a false edge and
its disposal is dominated; `lower_for` emits an unconditional branch
when the condition is absent, so `for.exit` has no predecessor.

**`while (true)` passed by accident.** Its exit block was dominated
only because the CFG carried an edge execution never takes. Both forms
were wrong the same way, and the fix removes the disposal rather than
repairing either lowering. §60.1 rule 8 held: only
`compiler/src/check/` changed, verified against the working tree.

### Verified here

All eight shapes that failed now emit with exit 0: the conditionless
`for` that returns, the one with a trailing statement, the one with an
initializer and update, the one that never leaves, and the same nested
in a block and in an `if`. `a212` keeps `while (true)`'s behaviour and
`a213` keeps the `break` control's disposal, both against their
committed goldens.

### One contract defect the round found

§101.4 item 4 asked every statement kind for a leavable case and an
unleavable one. `return`, `break` and `continue` have no leavable
case. The item now asks each kind for the cases it admits and names
those three. The round reported the contradiction and stopped.

### Not independently reviewed

§101 is a fix for a finding of this review, so it did not get a review
of its own. Its change is confined to `compiler/src/check/`, the gate
is green, and §60.1 rule 8 was verified by file set. A future phase
review covers it.

## A flaky gate test, found while landing §101

`gate quick` at `2bf8da9` produced three different verdicts on the
same clean tree:

```
debug 1376/2/2 exit 1
debug 1377/1/2 exit 1
debug 1378/0/2 exit 0
```

Neither failure is a defect in §101, whose own full gate at `801dbe1`
was `debug 1378/0/2 release 1376/0/2 exit 0`.

**`context_per_scene_host_builds_runs_and_matches_golden`** failed once
with `Blocking waiting for file lock on artifact directory` and then
`in-repo runtime archive was not found at
target/release/libsubscript_runtime.a`. Two cargo processes contended
for the target directory; the archive exists and is not missing. This
is the self-inflicted-noise class already recorded for benchmarks.

**`spawned_watch_preserves_stdout_before_each_trap`**
(`cli/tests/watch.rs:483`) timed out twice waiting for
`watch: swapped`. Run alone it passes in about 1.2 seconds, measured
three times. Under the gate it consumed the full deadline.

`wait_for_count` (`cli/tests/watch.rs:266`) uses
`Instant::now() + Duration::from_secs(20)`.

**The constant has no recorded basis.** *(Checked 2026-09-09 after the
owner asked why 20.)* It entered with `27e64bd`, the commit that added
`run --watch`. That commit records the polling interval, 150 ms mtime
polling, and says nothing about 20 seconds. `specs/blocks/cli.md` §12
states no timeout, deadline, latency or budget. The number appears in
the test source and nowhere else.

That is the defect, and it is not the number's size. Because no rule
says what the deadline should be, a failure at 20 seconds cannot be
read: nobody can say whether the product stopped working or the
machine was busy. A first version of this note called it "a
performance assertion in disguise", which named the symptom and
skipped the cause.

What the deadline is for decides its value. The test asserts that a
file swap is noticed and the program re-runs. The chain is the 150 ms
poll, a JIT recompile, and a re-invocation. A deadline stops a hang;
it does not bound that chain. So either it is derived from the poll
interval with the multiple stated, or the test carries no wall-clock
bound and relies on the harness timeout.

Open. The fix states the rule the constant lacks.

## §102 Red, measured 2026-09-09

The contract claimed that removing the deadline leaves the waiter
blocked when its child dies. Measured rather than asserted.

A standalone binary reproduces `cli/tests/watch.rs` `Capture` with the
deadline removed: `reader` breaks on `Ok(0) | Err(_)` without
notifying, and `wait_forever` uses `Condvar::wait`. The child writes
one line and exits without producing the needle.

```
RESULT: still blocked after 3s with the child gone
```

So "delete the deadline" turns a diagnosable failure into a hang, and
`tools/gate.sh` has no timeout above it. The bound is necessary; the
clock is not the thing that makes it necessary. §102.2 rule 1 is the
fix: notify on end of input and return an error derived from that
fact.

Source: `$TMPDIR/capture-probe`, `cargo run --offline`.

## §102 landed

```
gate full 4ccc6dfff1efefc03e6df9376b8c94a131d33790 dirty:1 debug 1380/0/2 release 1378/0/2 skips 2/0 clippy 7/18/13 goldens-moved 0 exit 0
```

`tools/gate.sh quick` three times, all `debug 1380/0/2 exit 0`. The
flake that produced this section did not reappear.

`Capture` now carries `CaptureState { bytes, ended }`. The reader sets
`ended` and notifies on end of input, where it previously broke
without waking anyone. `wait_for_count` returns an error naming the
needle, the count wanted, the count seen, and everything captured.
`Instant` and `Duration` are gone from the file; the wait is a plain
`Condvar::wait`.

`capture_reports_early_child_exit` is the firing control: a child
prints the needle once and exits, the test waits for two, and the
exact message including `saw 1` is asserted. Before the change that
shape hung.

### The contract defect the round found

§102.2 rule 3 said a surviving clock's value would be derived from
`cli.md` §12's 150 ms poll. §12 line 345 reads "the interval is
implementation-chosen and not contracted". The number came from
`27e64bd`'s commit message and was written here as though the contract
stated it — the same defect §102 exists to remove. The round checked
the citation rather than taking it, and stopped.

With nothing to derive from, the test keeps no clock at all.

### What remains, recorded rather than hidden

A child that stays alive and silent still hangs, and `tools/gate.sh`
sets no timeout. Rule 3a puts that where it belongs: a per-suite bound
is one decision for every test, and a test that invents its own is
what rule 2 forbids.

## The step bound, and a defect my own verification missed

`tools/gate.sh` bounds each step under §102.2 rules 3b and 3c. Two
errors in it were caught by others before it landed.

**The owner asked whether the mechanism works on Windows.** It does
not, and `specs/tracking/windows-portability.md` already recorded why:
a native parent starts the script, so MSYS `kill` cannot map that
process id. The first draft wrote its timeout marker before the kill,
so on Windows a step would run on and the record would claim a stop
that never happened. The marker now records only that the bound
elapsed, and the step's own exit decides between `gate-timeout`, which
fails, and `gate-timeout-unenforced`, which does not.

The same question found the value. 1800 seconds was three times this
host's longest step of 561, and the Windows release step is 1,257 — a
bound of 1.4 times a real run. It is 3600 now.

**The round measured a defect my verification could not see.** The
watchdog slept for the whole bound in one call. Killing the subshell
after the step finished left that `sleep` orphaned, holding the
descriptors it inherited, so anything reading the gate's output waited
for the full bound. Measured by the round: a step that exits in 3
seconds took 10.18 seconds under a 10 second bound.

My own check ran with a 2 second bound and no pipe, which is exactly
the shape that hides it. Under the real 3600 second default, every
`tools/gate.sh quick | tail` would have hung for an hour.

The watchdog now inherits no descriptor and sleeps in one second steps,
so the longest orphan is one second. Re-measured through a pipe: a 3
second step under a 600 second bound takes 3.5 seconds, and a 60
second step under a 3 second bound is stopped at 3.4 with exit 143.

A verification chosen for speed can pick the conditions that hide the
defect. The round's bound was ten seconds and mine was two, and only
one of them was slow enough to notice.

### The bound's own test

`cli/tests/gate.rs` gains two cases, and they cover all three
branches.

`quick_step_timeout_fails_with_a_passing_control` runs with
`GATE_STEP_TIMEOUT=1`: the gate exits 1, the record carries
`gate-timeout: step debug stopped at the 1 second bound`, and it does
not carry the unenforced line. Its control runs the same stub at 10
seconds: exit 0, and neither line.

`quick_step_timeout_unenforced_still_passes` reaches the branch I
expected to be unreachable on this host: a step outruns the bound,
still exits 0, and the gate passes with `exit status: 0` and the
unenforced line.

Gate at the landing: `debug 1384/0/2 release 1382/0/2 exit 0`, debug
422 wall seconds and release 470, so no real step comes near the 3600
second bound.

## MAJOR 4 closed, verified at `ed7a668`

The finding had three parts and each was checked rather than assumed.

**The rules and the fork agree.** Rule 1 pairs by value, in either
spelling, with line continuations between the halves producing no
character. The fork implements it: `5428cb6` replaced the textual peek
that matched one spelling, and `c603b41` made the lone-surrogate error
recoverable so the diagnostic no longer depends on where the literal
sits.

**No spelling is rejected with a false reason.** Measured here:

| Source | subscript | node |
|---|---|---|
| `"\ud83d\u{dc4d}"` | 4 bytes | U+1F44D, UTF-16 length 2 |
| `"\u{d83d}\udc4d"` | 4 bytes | same |
| a line continuation between the halves | 4 bytes | same |
| `"\ud83d\udc4d"` | 4 bytes | same |
| the literal character | 4 bytes | same |

**The corpus and the record cover them.** `a198` carries six brace
spellings and declares `js-comparable: no Q5`.
`codegen/tests/surrogate_continuations.rs` runs a198's source and a
CRLF transform of it through the dev JIT, the ship C tier and the
interpreter against the same golden. It first asserts that the source
really contains a `\` and a newline, and that it contains no `\r`, so
the CRLF variant is a real transformation rather than a copy. Without
those two assertions the test could pass on a source that exercises
neither.

The finding cost two fork commits and an owner push each, and it
found a second defect on the way: rule 2a's positional diagnostic.
