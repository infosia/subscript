# The CLI's trap output, and the docs gate — 2026-09-08

Status: **landed** — `cli.md` §13 at `ca16728`, `compiler.md` §91 at
`228b2a8`. Origin: two findings of the docs refresh
(`docs-refresh-2026-09-08.md`), reported to the owner and approved.

## §13 — `run` prints what the program printed

Measured at `95c7d74`, one program, the two paths:

| Command | stdout | exit |
|---|---|---|
| `subscript run corpus/trap/t03-loop-stops-at-fault.ts` | *(empty)* | 1 |
| `subscript build --source … --run` | `len=3` | 3 |

`RunError::Trap(report)` carries `report.stdout`, and the CLI
rendered only `report.to_string()`. Red: a test that asserts stdout
equals the committed `.expected` bytes failed with `left: []`.
Green: the same test passes; three more cases pin the empty-output
program, the exit code, and the watch path. A broken pipe on the trap
path no longer changes the exit code.

## §91 — the tutorials' programs run in the gate

`codegen/tests/docs.rs` reads `README.md` and `docs/*.md`. Every
`ts` block is a program (checked, run, and its stdout compared with
an adjacent `text` block or a `$ subscript run` transcript), a
fragment (checked, with each tracked mirror tried), or an excerpt
(every line after the citation held against the tracked file it
cites). The block count per document is asserted against a
hand-written table, so a changed fence spelling fails instead of
going silent.

**The gate's first run found four failures, not the one the contract
predicted.** Three were documents a reader cannot compile: `EventLog`
used and never declared; the worker program using `Job` and `Total`
and declaring neither; and a generated-mirror excerpt that cannot
stand alone as a mirror. The fourth was the gate's own pairing, which
took a host's reload message eight paragraphs below a program as that
program's output. Two of the three document defects were written by
this session's own refresh two commits earlier.

Contract corrections the rounds forced: rule 2 defines *immediately*;
rule 4a holds an excerpt line-for-line against the tracked file
instead of asking it to stand alone; rule 4a reads that file from the
working tree, not `HEAD` (a review found the gate would go green at
the moment a regenerated file drifts, and red when a document and its
file are fixed together); rules 7 and 8 add the transcript comparison
and the count assertion.

Fresh review: MAJOR 2, MINOR 8, all fixed. The second MAJOR was
`cli/src/watch.rs` still documenting, in a public `///`, the
behaviour §13 deleted.

## Gate

```text
gate full bf2c706edfcc250f1d0f6eaa03f73d95a00a80ea dirty:6 debug 1297/0/2 release 1295/0/2 skips 2/0 clippy 7/18/13 goldens-moved 0 exit 0
```

19 stated outputs are compared on every gate run.
