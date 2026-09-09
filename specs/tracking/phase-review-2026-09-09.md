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
4. **§96.1 rules 1 and 4 disagree with the fork.** Open; it needs a
   parser fork change. See below.

## MAJOR 4, open

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
