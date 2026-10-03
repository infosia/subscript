# §152 — A worker trap reports its own site

`worker_join` copies the Worker's `pos_id` into the parent's `worker-trapped` record.
The message is `worker trapped with <kind>: <message>`.
The thread-failure branch keeps position 0 and its message.
`runtime/src/context.rs` keeps 6,499 lines; three existing lines change.

## Red evidence

The trap sweep uses a source archive of the contract before the change (`9eb4f4cf`, a pre-amend commit of the contract that main does not reach; the runtime source is the same as at `9c6c3eb5`), with the new corpus entry and harness expectations.
The pinned test binary reports three failures for `t82-worker-trap-site`; no other entry fails.

| Tier | Position | Message | Stdout |
|---|---|---|---|
| dev | `:0:0` | `worker trapped with index-out-of-bounds at position 6: index 5 out of bounds for array length 1` | empty |
| ship | `:0:0` (runtime position 0) | `worker trapped with index-out-of-bounds at position 3: index 5 out of bounds for array length 1` | empty |

The corpus body copies the HEAD contract's Problem program exactly.
The contract expects Problem position `9:14` (measured); seven header lines make the corpus expectation `16:14`.
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

## Landing gate

```text
gate full 19fdd7afb80de8b78fe897709c6aac44e05a71c1 dirty:8 debug 2230/0/3 release 2227/0/3 skips 2/0 clippy 2/18/13 goldens-moved 0 exit 0
```

## Phase Review result

One review pass found CRITICAL 0, MAJOR 0, MINOR 4 (a stale tutorial
message, stale lines in this note, the open item in `s115-exceptions.md`
and the topic index, and a comment placed one line early in
`runtime/src/context.rs` near line 984). The first three are fixed; the
comment stays, as moving it would not grow the file but is a cosmetic
edit of a file past 2,000 lines. §152 is COMPLETE.
