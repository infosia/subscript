# Top-level block disposal

Contract: compiler.md §139. Pin: `56a9e10d05f47809fec3cc79b714660f56cfd637`.

## Red measurement

Before the implementation changed, builds produced the CLI and engine test binaries from the contract pin.
The entry is `corpus/accept/a303-top-level-using.ts`.
The loop reads a resource from an array allocated before the loop.
This removes W001 for the repeated allocation and keeps the loop block and its disposal exit.
The CLI `run --deny-warnings` exits 0 at the pin.

Interpreter, dev JIT, and ship C each print:

```text
in a
in b
in c
in f
d f
in m
d m
```

The engine test exits 101 against the new golden.
Node v24.18.0 with TypeScript 5.9.2 exits 0 and prints:

```text
in a
d a
in b
d b
in c
d c
in f
d f
in m
d m
```

## Implementation

If a top-level statement contains a disposal binding, the checker calls the same `using_scope::structure` function as the function-body checker.
The HIR carries `Stmt::Using` before the initializer route scan.
The shared lowering supplies disposal at each block exit.
No engine consumer needs a change.
The corpus suites derive their entry sets from the directory; no fixed entry table needs a change.

## Validation

The new compiler tests check one HIR form and both full S100 messages.
The runtime tests check natural, break, continue, and exception exits on all three engines.
Warm debug execution takes 0.40 s for two ship-C program compiles.
They also check reverse order, the null guard, and a dispose hook after the global declaration.
The runtime tests print control markers so a skipped block cannot pass silently.
The final suite omits the temporary corpus measurement test; the standing corpus suite checks the golden on all three engines.
The three-engine Green output equals the measured Node output above.
The generated corpus index adds a303 through `generate-api-reference`.
The pinned formatter and the TypeScript check pass.
The all-target workspace build has zero warnings.
The changed files have no new Clippy warnings.

The Phase Review found that unconditional scope construction drops unreachable reads in blocks without `using`.
The fix uses the function-body disposal-binding guard.
A regression test checks the unchanged initializer diagnostic and the retained read in the accepted control.

## Phase Review (Opus)

No CRITICAL or MAJOR. Two facts recorded from the review:

- A read that follows a statement control cannot leave (an `if` whose two branches throw) is dropped by the `using` scope structure before the §137 scan runs. So a top-level block with a `using` accepts such an unreachable early read, and the same block without a `using` gives S100. A function body behaves the same way. The read cannot run, so the acceptance is not a soundness gap; the split depends on the presence of a `using`.
- A top-level loop that makes a `using` resource in each iteration gave W001 at the pin. It gives no warning now, the same as the loop inside a function (§139 rule 3).
