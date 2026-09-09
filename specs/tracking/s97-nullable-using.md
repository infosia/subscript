# §97 — a `using` binding can be null

Contract: `specs/blocks/compiler.md` §97. Origin: the owner's
2026-09-09 request to reconsider three decided restrictions on their
merits, under core principle 14.

Pin for every measurement: `de26008fc99a01815005ec845b0d66f7fef70c34`.
Host aarch64 macOS. rustc 1.95.0. tsc 5.9.2. node v24.18.0.
Command `target/debug/subscript run <file>`.

## Red at the pin

Every program uses a class `Res` that declares
`[Symbol.dispose](): void` and prints on open and on dispose, and a
factory `make(live: boolean): Res | null`.

| Program | exit | subscript | node |
|---|---:|---|---|
| `using r = make(false);` | 1 | S100 "a `using` initializer must be a non-null reference class that declares `[Symbol.dispose](): void`; narrow nullable values first" | `open:none body`, no disposal |
| `using r: Res \| null = null;` | 1 | the same S100 | `body`, no disposal |
| `using r = maybeNoHook();`, class without the hook | 1 | the same S100 | disposal fails at run time |
| `using r = null;` | 1 | S100 "cannot infer a type from `null`; annotate the declaration" | `body`, no disposal |
| the outer-`if` workaround | 0 | `open:r body dispose:r` | same |

One diagnostic covers three different problems, and it names the fix
for only one of them. §97.1 rule 2 splits it.

Node, for the mixed shape
`using a = make(true); using b = make(false);`:

```
open:r
open:none
body3
dispose:r
```

Only the live binding disposes, and reverse order reaches the null
binding first as a no-op.

## Why the restriction goes

C11 and §60.1 rule 3 state the rejection and say "narrow first, then
bind". Neither states a type-safety problem, because there is none:
the disposal is guarded, which is what JavaScript already does.

The workaround costs more than the record admits. An outer `if` is a
new block, so the resource dies at that block's end rather than at the
end of the scope the programmer meant. The rule shortens a lifetime to
express a check.

## Landed, 2026-09-09

§97 landed at `a43bd8b`. Gate verdict:

```
gate full f7d56c340331e8a32df65a8852ba354ee721baef dirty:21 debug 1358/0/2 release 1356/0/2 skips 2/0 clippy 7/18/13 goldens-moved 1 exit 0
```

Counts: accept `.ts` 199, reject `.ts` 182.

### The round found three contract errors, all mine

1. Three spec files still named `r131-using-nullable-init`, which §97
   retires: C11's reject list, §60.3's corpus item, and a tracking
   record.
2. §97.4 item 7 said no golden moves. The aggregate LIR text snapshot
   collects every async corpus entry, so a200 grows it by
   construction. This is the same error §93.3 item 8 already
   corrected for a187.
3. My first fix to C11 wrote the retired name in a parenthetical
   instead of the `retired:<name>` form the file's own preamble
   defines, so a bare name remained for the §69 stage 3 check to
   read as a file that must exist.

### The round's own finding

The disposal rewrite appends a natural-exit disposal after a nested
block that returns. No path reaches it, and the LIR drops it.
`codegen/tests/support/lir_facts.rs` `stops_statement_sequence`
returned false for every `hir::Stmt::Block`, so the walk demanded two
trap sites where the LIR carried one.

The defect was the walk's model, not the rewrite: unreachable code is
legal, and the walk claimed the program required a site no path
reaches. A block now stops a sequence when any of its statements
stops, and an `if` stops when both arms are present and both stop. An
`if` without an else is unchanged. Five witnesses cover both the
stopping and the falling-through shapes.

### Verified here

- The snapshot grew by exactly a200's block, 16,792 bytes. Removing
  it reproduces the previous file byte for byte.
- a199 runs and matches its golden. Its output shows a null binding
  disposing nothing while its neighbours dispose in reverse order.
- `collision_table_is_a_consistent_corpus_index` passes with the
  `retired:` form.
