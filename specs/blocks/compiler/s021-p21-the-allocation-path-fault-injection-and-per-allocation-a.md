<!-- §21 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 21. P21 — the allocation path: fault injection and per-allocation attribution

Owner decision 2026-07-26. Two items that arrived separately and are
done together because **both touch `Context::alloc` and the 16-byte
allocation header**, and §20.4's rationale applies unchanged: splitting
them means touching that path twice.

§18.2e is superseded by §21.2.

### 21.1 Allocator fault injection

**The gap.** `TrapKind::AllocationFailure` has six raise points across
the two allocator modes and **no corpus program can reach any of
them**: there is no source-level memory quota, and exhausting real
memory is neither safe nor deterministic under overcommit. P20's `t26`
records the gap; the site is represented in the IR and unverified.

**Why this is not tidiness.** Three things are currently untested, and
the third has already happened once:

1. **The two tiers have different allocators** — the dev tier makes an
   individual system allocation per object tracked in a map; the ship
   tier (§8.1b) bump-allocates size-class blocks in chunks, with
   individual allocations above `LARGEST_BLOCK`. For the same program
   they fail at different moments, and a single object allocation can
   surface on the ship tier as a *chunk* allocation failing.
2. **`alloc` returns null on failure and the check happens after.**
   Generated code therefore holds a null payload for a window. That is
   the shape P19 found with `subscript_scratch` — a fault recorded, then
   execution continuing through a poisoned value, which there wrote 320
   bytes into a 256-byte buffer. Nothing looks at this one.
3. **Allocation *sequences* differed between the tiers until P20**,
   which removed a C-only empty-string allocation emitted per non-empty
   template — 9 fault points against 7 for `` `x${a}y${a}` ``. Only the
   differential gate holds them equal now.

**The knob.**

```c
void subscript_rt_ctx_fail_alloc_after(subscript_rt_context*, uint64_t n);
```

The Context refuses the **n-th subsequent allocation**. This is the
same shape as `subscript_rt_ctx_set_now` and `subscript_rt_ctx_seed_random`
(§18.1): a host knob that pins a nondeterministic input so tests and
replays reproduce, which §0.3 already establishes as the pattern.

**Count, not bytes — and the reason is measured.** §18.2d established
that `live_allocations` is tier-independent while `live_bytes` and
`reserved_bytes` are not, the ship tier rounding to size classes.
So "fail the n-th allocation" gives both tiers **the same failure
point**; "fail past n bytes" would give them different ones and could
not be compared at all.

**Expect the first run to find divergences, and treat that as the
point.** Injection by count *is* a test of allocation-sequence parity.
Item 3 above shows the sequences were unequal as recently as P20, and
nothing but the differential gate holds them equal now.

**What it does not cover**, stated so the limit is not mistaken for
coverage: real out-of-memory under overcommit stays untestable. This
measures **the language's response** to a refused allocation, not the
operating system's behaviour — which is the part that matters, since
the question is whether both tiers report the same tuple at the same
point and neither continues through the null.

**Check first, before building anything:** the "not representable"
raise point (`Layout::from_size_align` failing) may already be
reachable from source through a sufficiently large `FixedArray<T, N>`,
since the checker reads `N` from the annotation. If it is, that one
site can be tested today and the injection work shrinks. Report the
answer either way.

### 21.2 Per-allocation attribution (supersedes §18.2e)

**A live-allocation figure cannot name the script variable behind it,
and the obstacle is definitional, not cost**: an allocation is not
bound to a variable — values move between variables and into fields and
arrays — and a leaked allocation is usually reachable from no named
variable at all, which is why it leaked.

The **allocation site** is both available and better suited. A loop
allocating ten thousand times has one site and no useful name.

`alloc(size, class_id, pos_id)` **already receives** the site's
`pos_id` and discards it, using it only if the allocation fails, so no
new value has to be threaded anywhere.

**The header's fourth word was not free, and this section said it
was.** *(Corrected 2026-07-26 during implementation.)* On the ship tier
that word held each classed block's **exact requested payload size**,
and `Context.collect`'s mark phase read it to know how far to trace. Storing
`pos_id` takes it, so the mark phase now traces **the whole size-class
payload capacity** instead.

That is safe, and the safety is a property of the allocator rather than
an assumption: a fresh block comes from an `alloc_zeroed` chunk, and a
block reused from a free list is re-zeroed across its **full capacity**
(`write_bytes(payload, 0, block_size - HEADER_SIZE)`) before the header
is re-armed. The padding a conservative trace now reads is therefore
always zero.

The cost is real and bounded: `Context.collect` scans up to the size-class
rounding of each block rather than its exact request — at most a factor
of two, on an operation that never runs unbidden (invariant 2). The
alternative, widening the header to 24 bytes, costs every allocation 8
bytes to save an explicitly-invoked operation some scanning, which is
the worse trade. **Recorded rather than left silent, because a future
reader finding `Context.collect` tracing padding should find the reason here.**

Only the dev tier and the ship tier's large-allocation path add a
genuinely new store; for classed blocks the store replaces one that was
already there.

```c
typedef void (*subscript_rt_alloc_visitor)(void* userdata, uint32_t class_id,
                                     uint32_t pos_id, uint64_t payload_bytes);
uint64_t subscript_rt_ctx_visit_live_allocations(
    const subscript_rt_context*, subscript_rt_alloc_visitor, void* userdata);
```

Read-only, like §18.2d's three figures, and with the same cost
character: O(live blocks), a diagnostic rather than a per-frame
counter.

**Two things this phase must settle, not assume:**

- **The `class_id` and position tables belong to the compiler, not the
  Context.** §18.1 records the same problem for a trap's `pos_id`: a
  host cannot turn either id into a name without an artifact this
  repository does not emit. Emit it, on the same principle as §18.1b's
  generated header — **generated, never hand-written**, with a
  byte-identical regeneration test. A hand-kept table drifts from the
  ids it claims to describe.
- **The extra store lands in the ship tier's arena path** (§8.1b),
  which is performance-sensitive; that tier's justification is that
  emitted C is close to hand-written C. One `u32` store per allocation
  is expected to be negligible — **measure it, do not assert it.**

### 21.3 Corpus and gate (pre-registered)

**Fault injection.** Trap-corpus entries covering an allocation failure
in each distinguishable position — a reference-class `new`, an array
literal, a `push` that grows, a string concatenation, a template, a
generator frame, and the checker-generated JSON `RawNew`. Each compares
`(kind, message, position, pre-fault stdout)` across tiers, which is
what P20 made possible. `t26`'s policy-only record is replaced by real
entries for every site injection can reach; any that remain
unreachable keep a policy record saying so.

**The null-continuation question is a gate item, not an aside.** For
each entry, the generated code must not read or write through the null
payload between the failed allocation and the check. Demonstrate it —
AddressSanitizer on the ship tier, as P19's review did for the
320-byte case.

**Attribution.** An accept entry is not the right shape, since the
output is host-side. A both-tier host test allocates a known
population from known sites, visits, and asserts the `(class_id,
pos_id, bytes)` triples. `live_allocations` agreeing across tiers is
§18.2d's contract and this test inherits it.

Exit criteria:

1. Every allocation-failure site injection can reach is covered, on
   both tiers, with identical tuples — or is recorded as unreachable
   with the reason.
2. No entry continues through the null payload; ASan clean.
3. The visitor reports the allocating site, and the generated id tables
   let a host resolve `class_id` and `pos_id` to names.
4. The regeneration test for those tables is byte-identical, as
   §18.1b's header is.
5. Standing gate green; `tsc` clean; no accept `.expected` moves.
6. **Benchmarks re-run.** The `u32` store is on the ship tier's
   allocation path. Report emitted-C against P20's 1.53×; a regression
   is a finding, not a cost to absorb.
