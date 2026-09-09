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
