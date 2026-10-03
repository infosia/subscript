# §152 — A worker trap reports its own site

Round 3 supersedes the prior scope stop. The corrected Problem site `9:14` maps to corpus site `16:14`. Runtime/codegen tests, trap corpus, and `cargo fmt --check` pass.

`worker_join` copies the Worker's `pos_id` into the parent's `worker-trapped` record.
The message is `worker trapped with <kind>: <message>`.
The thread-failure branch keeps position 0 and its message.
`runtime/src/context.rs` keeps 6,499 lines; three existing lines change.

## Red evidence

The trap sweep uses a source archive of contract pin `9eb4f4cf`, with the new corpus entry and harness expectations.
The pinned test binary reports three failures for `t82-worker-trap-site`; no other entry fails.

| Tier | Position | Message | Stdout |
|---|---|---|---|
| dev | `:0:0` | `worker trapped with index-out-of-bounds at position 6: index 5 out of bounds for array length 1` | empty |
| ship | `:0:0` (runtime position 0) | `worker trapped with index-out-of-bounds at position 3: index 5 out of bounds for array length 1` | empty |

The corpus body copies the HEAD contract's Problem program exactly.
The contract expects Problem position `9:17`; seven header lines make the corpus expectation `16:17`.
The empty `.expected` pins the stdout before the trap; no existing golden changes.
The harness checks `worker-trapped`, the read site, the exact message, and agreement between the two tiers.

The pinned interpreter returns `Unsupported`: `Worker.Spawn requires a runtime worker adapter`.
The corpus header excludes the interpreter for this reason; both native tiers execute the entry.
No new test repeats the entry outside the corpus sweep.

## Existing expectations

| Test | Old expectation | New expectation |
|---|---|---|
| `codegen/tests/exceptions.rs::an_exception_that_leaves_a_worker_entry_is_the_worker_trap` | Prefix `worker trapped with uncaught-exception at position `; suffix `: TypeError: job 7 failed`; position `0:0` | Exact `worker trapped with uncaught-exception: TypeError: job 7 failed`; position `exception.ts:8:5` on both tiers |
| `runtime/src/worker.rs::an_exception_that_leaves_a_worker_entry_is_the_worker_trap` | Exact `worker trapped with uncaught-exception at position 41: TypeError: worker failed`; no position check | Exact `worker trapped with uncaught-exception: TypeError: worker failed`; `pos_id == 41` |

## Validation

`cargo run --offline --locked -p subscript-compiler --bin generate-api-reference` regenerates the API documents.
Only `generated-docs/corpus-index.md` changes; it adds the t82 row.
`cargo fmt --check` passes.

`cargo test --offline --locked -p subscript-runtime` passes.
The updated codegen Worker exception test passes on both tiers at `exception.ts:8:5`.
`cargo test --offline --locked -p subscript-runtime -p subscript-codegen` stops in the codegen C-emission suite.
That suite reports 104 passes and one failure; later suites do not execute.
`git diff --check` passes.

## Scope stop

The fixed corpus sweep reports this result on both tiers:

```text
Trap(kind=worker-trapped, message="worker trapped with index-out-of-bounds: index 5 out of bounds for array length 1", position=t82-worker-trap-site.ts:16:14, stdout="")
```

The Worker records the array expression's start (`xs`), at Problem position `9:14`, not the index expression (`m.value`) at `9:17`.
`compiler/src/hir/sites.rs` selects `target.pos` for an `IndexRead` trap.
The runtime now preserves that recorded site as rule 1 requires.
The remaining corpus failure compares expected `16:17` with actual `16:14`.
The corpus expectation stays at the contract's required column; no golden changes.

To resolve this mismatch, the orchestrator must correct the §152 contract's position or authorize a compiler position change.
The contract file and `compiler/src/hir/sites.rs` are outside this handoff's edit set.
Work stops at this scope boundary.

## Landing gate

```text
gate full 19fdd7afb80de8b78fe897709c6aac44e05a71c1 dirty:8 debug 2230/0/3 release 2227/0/3 skips 2/0 clippy 2/18/13 goldens-moved 0 exit 0
```
