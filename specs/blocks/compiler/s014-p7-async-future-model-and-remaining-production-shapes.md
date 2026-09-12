<!-- §14 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 14. P7 — async/Future model and remaining production shapes

Owner decision 2026-07-24. P6 binds a production C header's structure;
P7 closes the incremental interop gaps needed for the **common
main-thread-driven async / Future model** a production GPU C API uses,
and the remaining scalar/return/out shapes. Host-agnostic (invariant 4):
proven on a neutral synthetic fixture; no external API named or
committed; reference sweep stays zero.

The model P7 targets (as a real production GPU C API spells it): an async
op **returns a future** (a small by-value `{u64 id}` struct), taking a
**callback-info** value `{ mode; callback; userdata1; userdata2 }`; the
host later drives completion with a **wait/process-events** call that
takes an **out-array** of `{ future; bool completed }` the callee writes.
The callback fires on the pump/wait thread (synchronous, same thread) —
which is the a35 deferred-fire mechanism, already proven.

### 14.1 Chained integer/flag aliases

The emitter follows a `typedef → typedef → integer` chain to the
underlying sized type: `typedef uint32_t B; typedef B X;` → `type X =
u32` (and the flag-alias + `declare const` form when members exist). P6.2
handled one-level aliases and **fails loud** on two-level; P7.1 resolves
the chain (production GPU C APIs commonly spell flags as a two-level
alias). Still fail loud if the chain does not bottom out in a mapped
integer.

### 14.2 By-value boundary-struct return

A foreign function may **return a boundary value class by value** (e.g.
`SubFuture { u64 id }`). Both tiers marshal the struct return per the C
ABI (small structs in registers, larger via `sret`), subject to the
§12.3a arch-gate for the by-value aggregate ABI. Today foreign returns
are scalar/handle only (`lower/func.rs` rejects a non-scalar boundary
return); P7.2 adds the struct-return path. The returned value class is
then an ordinary in-language value (its fields readable, e.g. the future
id).

### 14.3 Out / mutable array and out fields

A foreign function may take a **mutable `T[]`** (or a boundary struct with
a callee-written field) that the callee writes back; the script reads the
written values after the call. Rule: the array/struct storage is the
caller's; the callee borrows it mutably for the call's duration and may
write; the caller observes the writes after return (no copy back — the
callee wrote the caller's own storage). Both tiers pass the same
`(ptr,count)` / pointer and the writes land in the language array/struct.
This is distinct from the const-borrow of a26/a31 (state the surface
spelling that marks an out/mutable array parameter).

### 14.4 Two userdata slots

The callback trampoline and the runtime `CallbackBinding` carry
**two** `void*` userdata slots (`userdata1`, `userdata2`), both delivered
to the language callback (each `object | null`, narrowed with `as`). P5.2b
carried one; P7.2 extends it. The Context-held-binding lifetime rule
(§13.3) is unchanged.

### 14.4a Callback bindings are interned by identity — Rev 2026-07-29

`bind_callback` allocated a fresh Context-held record on **every**
marshaled callback-info crossing and nothing ever swept it — not
`Context.free`, not `Context.collect`, only Context drop. The long-run
audit (`specs/tracking/long-run-audit.md`, finding 2) names the
consequence: a host that registers per frame grows one boxed record per
registration, without bound. The lifetime rule itself (§13.3: the binding
lives for the whole Context, because the C side holds the raw pointer and
the runtime cannot know when the host is done with it) is correct and
unchanged; the defect is that each registration paid for a new record.

**The fix is interning.** A boundary callback is non-escaping (C5), so
its function value is a non-capturing wrapper and `env` is always null —
`subscript_rt_cb_bind`'s own contract says so. A binding's identity is therefore
the tuple **(code, userdata1, userdata2)**. `bind_callback` returns the
existing record for a tuple it has seen, and allocates only for a new
one.

This converts the growth class rather than capping it: bindings become
**bounded by distinct (code, userdata) tuples used**, the same
bounded-by-distinct shape this project already accepts for the astral
code-point interns (§22.1) and the compiled-pattern cache
(`stdlib.md` §15.5a). The honest bound is stated the same way those two
state it: a program that registers a million distinct userdata objects
allocates a million bindings, and that is not a shape a real host loop
has.

Observable consequences, pre-registered:

1. Re-registering the same callback with the same userdata returns the
   **same binding pointer**; a C host may rely on pointer equality across
   re-registrations within one Context.
2. Deferred fires through a re-registered binding behave exactly as
   before — the record the C side stored at first registration *is* the
   record the pump reads.
3. No golden moves: no corpus entry or example observes binding identity
   or count today.

Exit criteria: (1) a runtime unit test registers one tuple twice and
asserts one record and pointer equality, and registers a second tuple and
asserts a second record; (2) a probe-style measurement (the
`dev-retention-probe` pattern) shows per-frame re-registration at zero
growth per frame; (3) standing gate green, no golden moved.

### 14.4b Registered callback userdata: rooted, checked at fire, advised at free — Rev 2026-07-30

Owner decision. Use-after-free through callback userdata — register,
release the object, the host fires later — is the natural failure of
the callback model, and the general freed-handle diagnostics
(§8.1a-1..3) are mis-shaped for it: their cost is proportional to
*everything freed* across a window that is exactly as long as the
host pleases. The binding records already hold the userdata pointers
(§14.4a), so this pattern gets three targeted mechanisms whose
standing memory cost is zero.

**(C) Registered userdata is rooted.** `Context.collect`'s mark phase
walks the live binding records and treats each userdata slot that is
a live allocation as a root; a slot that is not one (null, freed) is
skipped safely. No standing root table exists — the roots are derived
from the records at mark time. Consequence, both tiers: an object
registered as callback userdata survives collection and is released
only by explicit `Context.free` or Context release. Q13's "userdata
must outlive the registration" becomes a guarantee on the collect
side. Superseded registrations keep their records (§14.4a interning),
so they keep rooting their old userdata; the retention bound is
distinct registrations — the already-accepted intern class. Replacing
a registration does not release the old userdata; `Context.free`
does.

**(A) The trampoline checks liveness at fire.** Before a fired
callback enters script code, each non-null userdata slot is checked,
in order: in the freed-handle dead set (diagnostics mode on) — trap,
with the freed-allocation information; otherwise not a live
allocation — trap, best-effort in exactly §8.1a-2's class (certain
while the address is unallocated, undefined once reused). The trap
kind is a new stable kind for this pattern; message and position pin
in the corpus. Cost per fire: O(1) lookups against maps that already
exist; nothing is retained for this check.

**(B) Freeing registered userdata is advised.** A new optional
observer, `subscript_rt_ctx_set_diagnostics_observer(ctx, observer,
userdata)`, following the §18.2/§18.2f observer rules verbatim
(observation only, never re-entered, cleared by null). Its first
advisory: an explicit `Context.free` whose address is a userdata slot
of a live binding reports (advisory kind, `pos_id`, message) before
the release proceeds. It cannot be a trap: freeing userdata the host
will never fire again is legal, and the runtime cannot know
(§14.4a's reason). Observer unset — the default — skips the check
entirely; nothing changes for any existing program.

**(B2) Binding growth is advised — Rev 2026-07-30.** The frame-shaped
misuse — a per-frame entry registering with freshly allocated
userdata on every call — is invisible to static analysis (the loop
lives in the host), so it is observed dynamically. A host-set
threshold, `subscript_rt_ctx_set_binding_count_advisory(ctx,
threshold)`, default `UINT64_MAX`, literal semantics (0 = advise on
the first record; no sentinels — the §8.1a-3 convention): whenever a
**new** binding record is interned and the record count is at or
above the threshold, the diagnostics observer receives advisory kind
2 (`SUBSCRIPT_RT_DIAGNOSTICS_ADVISORY_BINDING_COUNT`), `pos_id` 0,
message carrying the count and threshold. Re-registering an existing
identity interns no record (§14.4a) and never advises — an advisory
therefore always signals real growth. No observer or default
threshold: the check is one comparison at intern time, nothing
retained. The static half of the same concern is `warnings.md` W003.

**Golden audit, pre-registered (2026-07-30).** No committed corpus
entry, gate program, or example both registers a sink and collects,
so (C) moves no committed golden. If implementation moves one, stop
and report; a moved golden is a finding, not an update.

**Corpus.** One new interop accept entry: register with userdata,
drop the script references, `Context.collect()`, pump — the callback
reads its userdata fields and the output is a committed golden
(defined behavior only under (C); this entry is the rooting proof).
One new interop trap entry in t22/t23's gating class: register, free
the userdata, pump — the fire-time trap's kind, message, and position
pin (A).

**Exit criteria (pre-registered).**

1. The rooting accept entry runs byte-identical under both tiers.
2. The fire-time trap entry pins (kind, message, position) under
   both tiers with diagnostics mode on, t22/t23's class; mode-off
   behavior is best-effort and not gated.
3. (B): a runtime unit test and an FFI test observe the advisory on
   free-of-registered-userdata; with the observer unset the full gate
   is green and byte-unchanged, which is the zero-cost proof.
4. A unit test asserts registered userdata survives collect
   (accounting), and that a freed slot is skipped at mark without
   fault.
5. No committed golden moves; `tsc` gate green with the new entries.

### 14.5 Composed Future-shape async (capstone)

A neutral fixture reproduces the whole model: an async op returns a
future (14.2), taking a callback-info `{ mode; callback; userdata1;
userdata2 }` (14.4); a host wait/process-events call takes an out-array
of `{ future; bool completed }` (14.3) and fires the registered callback
on the calling thread with both userdata. A corpus entry (a36+) drives it
end-to-end with a committed golden, byte-exact on both tiers.

### 14.6 Permanent non-goal — spontaneous (arbitrary-thread) callbacks

A production async model's *spontaneous* mode fires a callback on an
arbitrary thread at an arbitrary time. This is **permanently out of
scope**: the Context and the callback trampoline are single-threaded by
design (P5.2b soundness relies on same-thread synchronous invocation
under one Context; scripts are single-threaded, invariant 6). The
supported model is the **main-thread wait/process-events** path (14.5),
which fires synchronously on the caller thread. Binding a header that
offers a spontaneous mode is fine — a program simply must not select it;
the toolchain need not enforce this (trusted scripts).

### 14.7 Staging and gate

- **P7.1** — §14.1 chained aliases, §14.2 by-value struct return, §14.3
  out/mutable arrays: each a neutral fixture shape + both-tier corpus
  entry + golden; new structs pass the offsetof suite.
- **P7.2** — §14.4 two userdata + §14.5 the composed Future-shape async
  capstone: corpus entry, both-tier golden.
- **P7.3 (optional)** — re-run the generic `--header` CLI locally on a
  real production GPU C API header and report the coverage gain (how far
  the mirror gets now, what still fails loud), by scale/shape only, not
  committed.

Gate: each shape has a passing both-tier corpus entry with a committed
golden; the composed async entry passes both tiers; the mirror
regenerates byte-identically; still-unmapped constructs fail loud;
reference sweep clean; §14.6 documented as a permanent non-goal.
