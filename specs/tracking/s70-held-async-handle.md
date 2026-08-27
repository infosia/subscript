# §70 — a held async handle, by reference count

Contract `340284e`, corrected at `51f5aa7` and `e3a7385`. Landed
`efce1ed`.

Origin: the owner asked whether a `@Shared`-style decorator with a
reference-counted handle relaxes the async restrictions. It does.

## What moved

C8 rejected a floating async call, so the result of an async call had
to be awaited at the call site. Holding a handle, storing it in an
array, and passing it are now legal. Dropping one without awaiting is
still rejected: `r100` and `r105` are rewritten to that form, because
a coroutine that never completes runs none of its effects.

No `Promise` object appears and no scheduler. C8's model is
unchanged. The surface was already `tsc`-clean, so this accepts more
of what `tsc` accepts and invariant 5 needs no gate change.

## Two measurements taken before the work was ordered

**Where the count lives.** The count is at the frame's byte offset 4.
The frame does not grow, the allocation header does not move, and no
emitted offset changes.

**A coroutine frame was never freed.** The emitted C for
`a93-async-chain` called `subscript_rt_free` zero times; a frame was
allocated with class id `CLASS_GENERATOR` and left to the Context's
lifetime. So this section is the first thing that frees one, and that
was recorded in the contract before the round ran rather than
discovered in a measurement afterwards.

    a program that awaits a million calls, peak RSS
      pin       256,819,200 bytes
      now        14,155,776 bytes      94.5 per cent less, 18.1x smaller

A test pins `live_bytes` at zero after the last decrement, with no
`Context.collect()`.

**Invariant 2 holds and improves.** This is not a collector: no
traversal runs and the free happens at a known decrement. The
invariant says a program that never collects is correct, merely
larger. This makes "merely larger" eighteen times smaller.

## A correction this session owes the record

The handoff said offset 4 was "four bytes of alignment padding that
nothing reads". **It was the reload epoch**, documented in
`runtime/src/context.rs` as an ABI contract with generated code. The
claim came from reading the struct declaration in `cemit.rs` without
looking for a writer; one `grep` in `runtime/` would have found it.

The round took the wrong premise and handled it correctly: an async
frame's epoch moved to Context metadata, and a generator still uses
offset 4. The conclusion held and the reason recorded was wrong.

This is the third time in this arc that a claim of absence came from
reading one file. The other two: "the language has not decided what a
`for...of` does when its container changes", which `a80`'s own header
decides; and "a `SubFn` copy keeps sharing one environment", which
S009's "by value" contradicts. **Confirming that something is absent
costs more than confirming it is present, and this session spent the
same effort on both.**

## The third consumer

The interpreter had to agree, and it is what proves the rules are
implementable from the section rather than from a tier: 101/101 debug,
102/102 release. It gained a per-coroutine owner count, retain and
release, a cached completion value for a second holder, and handle
pack and unpack inside an array.

## Gates at `efce1ed`

Zero warnings. 1031 passed in debug, 1030 in release, no failures. The
full interpreter sweep ran. `cargo fmt --check` and the `tsc` gate
clean. clippy compiler 7, runtime 22, codegen 13. No pre-existing
corpus entry, golden, or `.expected` moved.

`a22` measures 1.34x, the same as the pin, so the count's increments
and decrements do not reach the performance gate. `a22` holds no async
handle, which is why — the check was still needed.

`a154`, `a155`, and `r157` are Red at the pin, verified against a
binary built from it (CLAUDE.md core principle 10).
