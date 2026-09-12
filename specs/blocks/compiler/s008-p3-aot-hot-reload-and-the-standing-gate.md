<!-- §8 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 8. P3 AOT, hot reload, and the standing gate

Observable obligations only; internal design is the implementer's.

### 8.1 AOT tier

- The **same** `lower_module` instantiated with `cranelift-object`
  (§1: one lowering, both tiers). A lowering change that helps one tier
  and not the other is a contract violation, not an optimization.
- Host-target AOT (the machine running CI) is the gate path: emit an
  object, link it with the runtime staticlib and a small C or Rust
  entry that calls the program's `main` and writes the Context sink
  bytes to stdout, run it, capture stdout bytes.
- Device-triple AOT (`aarch64-apple-ios`, `aarch64-linux-android`)
  must still **compile and link** for a run-set entry — the P0.5 spike
  proved a minimal program; P3 proves the real lowering's output links.
  No device execution is required (P0.5 criterion, unchanged). The
  desktop host ship targets (§11: `x86_64-unknown-linux-gnu`,
  `aarch64-apple-darwin`, `x86_64-pc-windows-msvc`) are emitted the same
  way for Cranelift-object shape parity and, being native, are
  additionally executed by the standing gate on their own hosts.
- Cross-tier determinism: the AOT binary's stdout bytes must equal the
  JIT's for every run-set entry. Where they differ, the language rule
  decides which side is wrong (§2), never the golden.

### 8.1a Ship-tier manual memory is released, not retained

The dev tier realizes `Context.free`/`Context.collect` (Q6/Q7) by
**retain-and-poison**: the freed allocation's bytes stay owned by the
Context and its header is stamped dead, so a stale handle *traps* instead
of reading reused memory (§7). That retention is the price of the dev
tier's trap-on-use-after-delete guarantee.

The **ship tier does not owe that guarantee** — in AOT, double delete and
use-after-delete are undefined (Q6; invariant 6, trusted scripts). So the
ship tier **returns the allocation to the system allocator immediately**:
`Context.free` (and a `Context.collect` sweep) free the backing storage and drop
the Context's bookkeeping entry for it, rather than retaining a poisoned
corpse.

- **Soundness / gate-safety.** For a *correct* program — one that never
  reads a handle after its `Context.free`, and never deletes twice — the
  released and retained policies are observationally identical: the only
  difference is the state of memory the program has promised not to touch.
  Every `corpus/accept` entry is such a program by construction, so
  **dev-JIT bytes ≡ AOT bytes ≡ golden** (§8.3) is unaffected. The reject
  corpus's use-after-delete / double-delete entries are dev-tier
  diagnostics and are not run under AOT.
- **Mechanism.** The tier is identified by the Context constructor, with
  no generated-code and no C-entry change: the dev-JIT driver builds its
  Context in Rust (retaining), and the AOT host entry builds its Context
  through the runtime's C constructor (releasing). One lowering, one
  runtime; only the allocation-lifetime policy differs, selected at
  Context creation.
- **Why it matters (measurable).** Retention makes the Context's
  allocation table grow monotonically for the whole run: a program that
  builds and frees N reference-class instances in sequence leaves N dead
  entries behind, so each later allocation works against an ever-larger
  table (cache-hostile, superlinear). Release bounds the table at the live
  set. Exit criterion: (1) §8.3 stays byte-exact on the reference-class
  entries (including `a16` collect); (2) a runtime unit test asserts that
  in ship mode a deleted allocation leaves **no** table entry (live count
  and table size both drop), where the dev-mode test still observes the
  poisoned-corpse trap. The `tree` benchmark's per-allocation cost, flat
  in C and superlinear under retention, is the informal corroboration, not
  a gate.

### 8.1a-1 Retention is a mode, not the dev tier's policy — Rev 2026-07-29

**What changed.** §8.1a above made retain-and-poison the dev tier's
standing policy. It is now **off by default**; the dev tier releases like
the ship tier, and retention becomes a mode a host switches on when it is
hunting a dangling handle.

**Evidence.** `specs/tracking/dev-retention.md` measured what the policy
costs: retained bytes per allocation are exactly **payload + 16**, across
four object shapes and both `Context.free` and `Context.collect`, growing
strictly linearly in cumulative allocations with the live set held at
zero. A particle-shaped object at 1 000 allocations per frame and 60 fps
exhausts 8 GB in **0.77 hours**.

**Decision (owner, 2026-07-29).** Memory that grows linearly and without
bound is not acceptable regardless of duration. Bounding the retention was
considered and rejected in favour of removing it from the default path
entirely: a bound narrows *how far back* a use-after-free is detected
while still costing memory, and it leaves a stale read landing on a
recycled address — undetected and silently wrong — rather than absent.
A mode is either complete or off, and says which. (**Narrowed by
§8.1a-2**: the mode now carries a size threshold and states its coverage
by class.)

**The guarantee is now conditional, and that is a real loss.** With the
mode off, double free and use-after-free in the dev tier are undefined,
as they already are in AOT (Q6; invariant 6). **A third diagnostic is
gated with them:** freeing a pointer the Context never owned traps as an
invalid free with the mode on and is a silent no-op with it off, matching
the ship tier. Naming only the first two would understate what the
default gives up. The dev tier no longer
diagnoses them by default. This is stated here rather than footnoted
because §8.1a called the trap a dev-tier guarantee and it is one no
longer.

**The mode is per Context and host-set, not compile-time.** A build flag
would force two builds of the workspace to run one `cargo test`: the trap
corpus needs the mode on in the same run where the accept corpus, the
benchmarks and the examples need it off. So it is a Context-level setting
established before the first allocation, exposed on the host C API beside
the other `subscript_rt_ctx_*` settings, defaulting to off. When on, behaviour
is exactly today's retain-and-poison, unbounded — a diagnostic session
accepts that cost deliberately.

**Consequences.**

- **No golden moves.** §8.1a's own soundness argument applies unchanged:
  a correct program cannot observe released-versus-retained, so
  dev-JIT ≡ ship-C-AOT ≡ golden is unaffected.
- **`corpus/trap/t22` and `t23`** carry `tier-policy: dev-JIT traps;
  ship-C-AOT behavior is deliberately unspecified`. They now additionally
  require the mode, and the gate enables it for them. Their trap tuples
  and `.expected` bytes must not move.
- **Dev-tier accounting** loses its retained term: `reserved_bytes`
  becomes live plus per-allocation overhead, with the mode off.
- §8.1a's "Mechanism" paragraph — the tier picks the policy at Context
  construction — is superseded: the policy is now a setting, and the tier
  only chooses its default.

**Exit criteria (pre-registered).**

1. `benchmarks/src/bin/dev-retention-probe` reports **no growth per
   allocation** with the mode off, on every shape it already sweeps.
2. The same probe with the mode **on** reproduces `payload + 16`, so the
   diagnostic path is intact rather than removed.
3. `t22` and `t23` trap with the same kind, message and position.
4. No accept or trap golden moves; the standing differential gate green;
   `tsc` clean.
5. A runtime unit test asserts both directions of the setting, as §8.1a's
   criterion (2) does for the tier.

### 8.1a-2 The mode takes a size threshold — Rev 2026-07-29

**What changed.** §8.1a-1's mode retained every freed allocation. The
setting now carries a minimum payload size: with diagnostics on, a freed
allocation is retained and poisoned only when its requested payload is at
least the threshold; a smaller allocation is released exactly as with the
mode off. Threshold 0 reproduces §8.1a-1's behaviour unchanged.

**Decision (owner, 2026-07-29).** Retention costs `payload + 16` per
freed allocation, so the mode's growth is driven by allocation count, and
small short-lived objects are the high-count class in the loops this
language targets. Recording them exhausts a diagnostic session's memory
before the session finds its fault, while the handles a session hunts are
typically to larger, longer-lived objects. The mode therefore records
larger objects only, with the boundary host-chosen.

**C API.** The setting and its threshold are established together:

```c
int32_t subscript_rt_ctx_set_freed_handle_diagnostics(
    subscript_rt_context *ctx, uint32_t enabled, uint64_t min_payload_bytes);
```

Per Context, host-set, refused (returns 0) after the first allocation, as
before. `min_payload_bytes` is ignored when `enabled` is 0. The threshold
compares the **requested payload** — the same quantity the dev
accounting's `live_bytes` sums — not the layout.

**What §8.1a-1 argued and this narrows.** §8.1a-1 rejected bounding
retention by *age* with "a mode is either complete or off, and says
which". A size threshold is a bound by *class*, and the mode now says
which classes it covers: **complete at and above the threshold,
best-effort below it.** Below the threshold, with the mode on:

- A stale handle **traps while its address remains unallocated** — the
  live-map membership check that funds the trap is already paid for and
  stays — and is undefined once a later allocation reuses the address,
  exactly as with the mode off.
- A double free likewise traps while the address is unallocated, reported
  as an **invalid free** — without a retained record the runtime cannot
  distinguish the two — and is undefined once the address is reused.
- Freeing a pointer the Context never owned traps regardless of the
  threshold: no size exists for a pointer that was never owned, and the
  bookkeeping map detects it whole.

A session that saw no trap has shown absence of stale-handle faults only
at and above its threshold. The generated host header states this beside
the setting.

**Exit criteria (pre-registered).**

1. `benchmarks/src/bin/dev-retention-probe` gains a threshold column:
   with the mode on and a threshold strictly between two swept shapes'
   payloads, shapes below it report **0.000** bytes per allocation and
   shapes at or above it report `payload + 16`; the off and threshold-0
   columns reproduce §8.1a-1's table.
2. `t22` and `t23` trap with the same kind, message and position; the
   gate enables the mode for them with threshold 0.
3. Runtime unit tests assert: release below the threshold and retention
   at and above it, visible in the accounting; the pre-first-allocation
   refusal with the new signature; a below-threshold stale handle traps
   when its address has not been reused; invalid free traps under a
   nonzero threshold.
4. No accept or trap golden moves; the standing differential gate green;
   `tsc` clean.
5. The generated host header documents the threshold and its coverage
   statement — generator-driven, never hand-edited.

### 8.1a-3 The mode takes a retention budget — Rev 2026-07-29

**What changed.** §8.1a-2 bounded retention by class; the mode now also
bounds it in total. The setting carries a byte budget: the layouts of
retained-and-poisoned allocations never sum above it. When retiring one
more allocation would exceed the budget, the oldest retained allocations
are evicted — released and forgotten — until the new one fits; an
allocation whose own layout exceeds the whole budget is released
immediately. The budget bounds diagnostic retention only; live
allocations are the program's and are never evicted.

**Decision (owner, 2026-07-29).** Above the threshold, retention is still
unbounded (§8.1a-2's header states so), and a diagnostic session that
exhausts the machine diagnoses nothing. The owner requires a hard ceiling
on the memory the mode may hold.

**What eviction means.** An evicted allocation joins the best-effort
class §8.1a-2 defines for below-threshold frees: its stale handles trap
while the address remains unallocated and are undefined once a later
allocation reuses it. Eviction introduces no new semantic class. The
guarantee becomes: **diagnostics are guaranteed for the most recently
retained frees whose layouts fit the budget, within the size class the
threshold covers; best-effort everywhere else.** Eviction is
oldest-first because a hunted stale handle is typically to a recently
freed allocation; evicting newest-first would spend the budget on the
frees least likely to matter.

**C API.** The setting, threshold and budget are established together:

```c
int32_t subscript_rt_ctx_set_freed_handle_diagnostics(
    subscript_rt_context *ctx, uint32_t enabled, uint64_t min_payload_bytes,
    uint64_t max_retained_bytes);
```

`max_retained_bytes` is literal, as the regex budget is: 0 retains
nothing (the mode becomes wholly best-effort), and a host that wants no
practical ceiling passes `UINT64_MAX`. The budget counts the retained
allocations' **layouts** — the `payload + 16` the probe reports — because
that is the memory actually held. Both parameters are ignored when
`enabled` is 0; same pre-first-allocation refusal.

**Default budget (owner, 2026-07-29): 1 GiB** (`1_073_741_824` bytes).
The parameter has no optional form in C, so the default lives in two
places: the generated header exposes it as
`SUBSCRIPT_RT_FREED_HANDLE_DIAGNOSTICS_DEFAULT_MAX_RETAINED_BYTES` for hosts
to pass, and the dev tier's own mode-enabling path (the JIT runner's
boolean parameter) uses it rather than `UINT64_MAX`. A host that wants a
different ceiling passes its own number; nothing in the runtime treats
the constant specially.

**Exit criteria (pre-registered).**

1. Runtime unit tests drive frees past the budget and assert: retained
   layout bytes never exceed the budget at any point; eviction is
   oldest-first; an evicted handle still traps while its address is
   unallocated; the newest retained handle traps as guaranteed; an
   allocation whose layout alone exceeds the budget is released
   immediately; a zero budget retains nothing.
2. The probe gains a budget setting: with the mode on, threshold 0 and a
   budget smaller than a run's cumulative frees, reserved bytes plateau
   at or below the budget while frees continue — growth per allocation
   reaches 0 after the plateau.
3. `t22` and `t23` trap with the same kind, message and position; the
   gate enables the mode for them with threshold 0 and the default
   budget — their retention is orders of magnitude below it. No accept
   or trap golden moves; the standing differential gate green; `tsc`
   clean.
4. The generated host header documents the budget, the eviction order,
   the resulting guarantee, and the default constant — generator-driven,
   never hand-edited.

### 8.1b P8 — ship-tier allocator: Context-owned arena, size-class free lists

§8.1a removed retention; the remaining ship-tier allocation cost is the
**per-allocation bookkeeping map** (measured: on the `tree` workload's
30×131071 alloc/delete pairs, the map plus its bookkeeping is ~75% of the
runtime's allocation overhead; the 32-byte-zeroed-with-header allocation
shape itself is ~+17% over the C baseline's bare `malloc`/`free`). The
ship tier therefore drops the per-allocation map from the hot path.

**Scope: ship tier only** (`Context::new_releasing`). The dev tier keeps
the map, and retain-and-poison when freed-handle diagnostics are on — the
map is what funds its trap-on-stale-handle diagnostics (§8.1a, **narrowed
by §8.1a-1**: this paragraph described the retention as unconditional). One runtime, two allocation
policies, selected at Context construction as today; no generated-code,
lowering, or `subscript_rt_*` ABI change.

- **Mechanism.** The ship Context owns memory in chunks. Small
  allocations (header + payload up to a largest size class) are carved
  from **per-size-class chunks** by bump pointer; `Context.free` pushes
  the block onto that class's LIFO free list; the next same-class `alloc`
  pops it. Allocations above the largest class are carried as individual
  system allocations with their own Context record (they remain
  enumerable). Context drop frees chunks and large records wholesale —
  Context-scoped memory (invariant 2) is preserved.
- **Header.** The 16-byte block header (state word, class id) is kept and
  additionally carries what tracing needs (payload size or size class);
  payload alignment stays 16.
- **Zeroing.** `alloc` returns a zeroed payload in every case, including
  free-list reuse — conservative tracing and language zero-init rely on
  it.
- **Membership is exact.** The conservative scan and `Context.collect()` need
  "is this word a managed payload address?". The test must never
  identify an address as a managed block unless it is one: chunk-range
  lookup, block-grid alignment within the per-class chunk, bump-watermark
  bound, and a live header state — all four. A false positive that lets
  the sweeper treat arbitrary memory as a block is memory corruption, not
  conservatism.
- **`Context.collect()`** (Q7, explicitly invoked only) still works on the ship
  tier: mark from roots/shadow/interned as today; sweep by walking each
  chunk's blocks linearly (bump watermark bounds the walk) plus the large
  records; unreached live blocks go to their free list (or are freed, for
  large records). Mark state lives in the block, not in a map.
- **Q6 amendment.** §8.1a described ship-tier double delete as "presents
  as an absent entry, a silent no-op" — that was a property of the map
  implementation, not of the contract. Under the arena, double delete and
  use-after-delete remain **undefined** on the ship tier (Q6, trusted
  scripts) and may corrupt the allocator; the dev tier remains the
  diagnosing tier and still traps both.
- **Retired array blocks** (§8.1a array growth) flow through the same
  free-list/large-record path.

**Exit criteria (pre-registered):**

1. Ship `tree` ratio **≤ 2.0× C** on the arm64 reference machine
   (from 5.11×), same runner and methodology (§9); no other workload's
   ship row regresses by more than 5% beyond run noise.
2. The standing gate (§8.3) stays byte-exact on every corpus entry —
   including `a16` (collect) and the reference-class entries — on both
   tiers.
3. Runtime unit tests, same commit: (a) ship alloc→delete→alloc reuses
   free-listed storage without chunk growth over N cycles; (b) Context
   drop frees every chunk and large record (no leak, asserted by a
   counting hook or chunk count); (c) ship `Context.collect()` frees unreachable
   blocks and keeps rooted ones (arena edition of the existing tests);
   (d) the dev-tier trap tests (double delete, stale handle) pass
   unchanged.
4. Inspection aids (`is_live`, `live_count`) remain functional on both
   tiers.

### 8.1c A block that holds no handle is not scanned, and a string is written where it lives

*(Owner, 2026-08-29, after the `collect`
benchmark was decomposed.)*

`collect` measured 6.45× the C baseline (`benchmarks/README.md`,
2026-08-27). Five variants of the workload, each a standalone ship
binary built with the runner's flags and timed eleven times:

    nodes only, no collect                         12.5 ms
    nodes only, with collect                       16.3 ms
    short strings (`${uid}`), with collect         14.4 ms
    the four padded strings, no collect            97.9 ms
    the four padded strings, with collect         219.0 ms   (the benchmark)

Two causes, of comparable size.

**A. The marker scans string payloads.** `arena_mark` pushes every
payload word of every marked block as a pointer candidate and looks
each one up in the chunk table, and it does not read the class id the
block header carries. A string is `[len][bytes]` and holds no handle.
The live strings of one round are 60k, averaging 13 words: about 4.7
million lookups per run, about 121 ms. The dev tier's marker
(`allocations.get_mut` per word) has the same shape.

*Rule.* A block whose class holds no handle — `CLASS_STRING` today; a
class the runtime can name as handle-free in the same way later — is
marked and not scanned. The marker reads the class id from the header
before it pushes the payload. Conservative tracing of padding stays
for every other class. Zero-initialization of a handle-free block is
not required by tracing; the language's zero-init rules decide it
(a string writes its length and bytes and exposes nothing else).

**B. A string is built through Rust-side temporaries.** `${uid}` builds
a `String` and copies it into the arena. `padStart` copies the receiver
and the pad to two `Vec`s, builds a third, and copies that into the
arena; the arena zeroes the whole block on free-list reuse before the
string overwrites it. The C baseline writes each string into its
`malloc` with one `memset` and one `memcpy`. The 120k nodes' strings
cost about 85 ms here and are inside the baseline's 32 ms.

*Rule.* A string operation whose result length is known before the
bytes are produced allocates the result first and writes into it.
`alloc_str` gains a form that takes the length and a writer; `padStart`,
`padEnd`, the integer formatters, and concatenation use it. No `Vec` or
`String` is allocated per call on the runtime side for these
operations.

**Exit criteria.** No corpus golden moves (neither rule changes an
observable). After A: `collect` under 3.5× on the arm64 reference
machine. After A and B: under 2.0×. §3's 7.5× gate stays a ceiling
against regression. The `cross-language` snapshot and `benchmarks/
README.md` are re-measured on a quiet machine after each round.

**Measured.** A landed at `64a637a`: the standalone ship binary
220.4 → 106.4 ms, `perf-gate` collect ship 3.02×, dev 5.22×. B landed
at `642c295`: 98.2 → 33.0 ms, `perf-gate` collect ship 0.95×, dev
3.13×; `cross-language` collect ship 1.01×, dev 3.30×, against LuaJIT
3.75× and JSC 1.08×. No golden moved in either round. The size-class
arena that frees each object individually was never the cost; the
marker's string scan and the string temporaries were, and
`benchmarks/README.md`'s earlier explanation is corrected.

### 8.1d A push is inline, and a static callback is a loop

*(Owner, 2026-08-29.)*

`callbacks` measured 21.84× the C baseline. Decomposed as §8.1c was:

    the benchmark (map, filter, reduce with named callbacks)      321 ms
    the same three stages as hand-written loops with push         284 ms
    20M pushes alone                                              149 ms   7.5 ns each
    20M bounds-checked reads with an add                           22 ms   1.1 ns each
    20M direct calls of a named function                           10 ms   inlined

The callback trampoline is not the cost; the push is. Every `push`
is an out-of-line runtime call that takes the value through memory,
multiplies by a run-time `elem_size`, copies with a dynamic length,
and is followed by a trap-flag read. The workload's 35 million pushes
at 7 ns are 245 of its 321 ms.

**A. A push is inline.** The element type is static at every push
site, so both transcribers emit the fast path in place:

    if (len < cap) { data[len] = value; len += 1; } else { grow }

with a typed store and a static stride, and the growth path stays the
runtime's. The trap-flag read after a push moves to the growth path,
which is the only place a push can trap (allocation failure). The
runtime gains `array_with_capacity(n, elem)` so a producer that knows
its bound allocates once. The interpreter keeps its own push; the
verifier is unchanged, because LIR's `ArrayPush` does not change —
the transcribers read the same instruction and emit it differently.

**B. A static callback is a loop.** `map`, `filter`, `reduce`,
`reduceRight`, `forEach`, `some`, `every`, and `findIndex` whose
callback operand is a known function — a named function, or a lambda
literal with or without captures — lower in `codegen/src/lir.rs` to
the iteration protocol of §68.7.4 with a **direct call** at each
element, the output array (for `map` and `filter`) allocated with
`array_with_capacity(len)`, and the push of part A. The range is the
one ECMA gives each method and the one the runtime implements today:
fixed before the first call for every method in this list, elements
removed before their visit not visited. A callback that is a function
*value* (a variable, a parameter) keeps the runtime intrinsic, which
keeps its per-element `len_of` check and its trap propagation. Both
tiers and the interpreter read the loop as they read any loop; no tier
gains a form.

The observable behaviour of the two paths is identical, and a corpus
entry pins it by running the same program through both spellings
(`a171`: a named callback, a lambda, and a function value held in a
local, each over the same array, printing the same lines).

**Exit criteria.** No corpus golden moves. After A: the 20-round
push-only program under 60 ms on the arm64 reference machine (2.6 ns
per element, of which the doubling growth and process start are about
half; the fast path itself is about 1 ns). After A and B: `callbacks`
under 3×. §3 gains no gate for it; the `cross-language` snapshot is
re-measured after each round.

**Measured.** A landed at `a430255`: the push-only program 150.8 →
51.8 ms; `callbacks` unchanged within noise. B landed at `7775aa7`:
`callbacks` as a standalone ship binary 296.7 → 37.0 ms, `cross-
language` ship 2.84× of C (dev 21.49×: Cranelift does not inline the
direct call, and the dev tier is gated on iteration time, not on
this), against LuaJIT 9.25× and JSC 5.23×. `a171`, `a172`, `t52`, and
`t53` pin the equivalence of the loop and the intrinsic. No golden
moved in either round.

*(Corrected 2026-08-29 when A was measured. The criterion first read
"push under 1.5 ns and `callbacks` under 8× after A". The 1.5 ns
assumed a pre-sized array, which the language cannot spell, so the
push-only program pays eighteen doublings per round. And `callbacks`
does not move under A at all — 321 → 296 ms — because its pushes are
the runtime intrinsics' own `array_push`, which part A does not
touch; the hand-loop decomposition above measured hand loops, not the
benchmark's spelling. Both were the orchestrator's estimates, and the
measurements replace them.)*

### 8.1e A runtime result is rooted while script code runs, and release work is one table

*(Owner decision, 2026-08-30; review finding.)*

**Rule 1 — a runtime operation roots every allocation it holds across a
call into script code.** This applies to `map`, `filter`, `groupBy`, and
every other runtime entry that allocates a result and then calls a
callback. The entry pushes the result handle on the shadow stack before
the first call. It pops the handle after the last call. `Context.collect()` inside the callback (Q7) then keeps the
result. The rule applies to the dynamic-array family and the fixed-array
family alike; the two families are one loop over an element source,
written once in `runtime/src/arrops.rs`.

Measured before the rule (`arrops::map`, `arrops::filter` did not root;
`fixed_map`, `fixed_filter`, and `group_by` did): `a173` traps
`[internal]: array storage disappeared while growing it` on the dev tier
at the first push after the callback's collect. A known callback (§8.1d
B) does not reach the runtime loop, so `a171` did not see it.

**Rule 2 — the release work for a class is one function.** `Context`
frees an allocation on three paths: the dev `delete`, the arena chunk
sweep, and the arena large-record sweep. The per-class work (container
entry clear for `CLASS_MAP` and `CLASS_SET`, intern removal for
`CLASS_STRING`, compiled-pattern removal for `CLASS_REGEX`) is one
function that every path calls with the class id and the payload. A
class that needs release work is added in that function only. A runtime
unit test frees one allocation of each such class on each of the three
paths and checks the same post-state.

**Corpus.** `a173-callback-collect-rooted`: `map` and `filter` through a
function value, and `filter` on a `FixedArray`, each calling
`Context.collect()` per element; the printed results are complete. The
golden is the same on both tiers and the interpreter.

### 8.2 Hot reload (dev tier)

Per §1's rules, made testable:

- **Declaration hash** over: every value/reference class (field names,
  types, order), enum member values, `FixedArray` shapes, every
  module-level variable's name and type, every function signature
  (name, parameter types, return type). Function *bodies* are excluded
  by construction. The hash is computed from the typed HIR, is stable
  across recompiles of identical declarations, and changes for any
  declaration edit.
- **Accepted swap**: same declaration hash → the per-function
  indirection table is repointed at the newly compiled bodies. Context
  state (globals, live allocations) survives; execution continues.
- **Rejected swap**: different declaration hash → the swap is refused
  with a diagnostic naming the first differing declaration; the running
  program is untouched (a refused reload never corrupts a live
  Context).
- **Stale coroutines**: a coroutine suspended in a function whose body
  was replaced is invalidated; resuming it traps with a
  `stale coroutine after reload` report carrying the resume position.
  A trap does not end the dev session: the host clears the trap record
  at the boundary and calls script again; Context state (globals, live
  allocations) is unaffected by the clear, and a stale coroutine stays
  stale (resuming it traps again). A trapped session that cannot be
  resumed would make the contract's own stale-coroutine rule kill the
  program the reload was required to keep running.
- **Frame boundary**: swaps are applied only between host calls into
  script (no swap while script code is on the stack). The demo drives
  this explicitly.
- **Demo** (a test, not a script): exercises all three cases — an
  accepted body edit whose new behaviour is observed in output, a
  rejected layout edit, and a stale-coroutine trap.

### 8.3 Standing differential gate and golden freeze

- The default `cargo test` path gains: for every run-set entry with a
  committed golden, **dev-JIT bytes ≡ AOT bytes ≡ golden bytes**.
  Byte-exact; no normalization; a missing AOT toolchain fails the test
  rather than skipping it (the gate machine is the dev machine).
  The command that runs this gate, its two shapes, and its record
  are §85.
- On green, the a22–a24 goldens captured at P2 are **frozen** (§2): the
  tracking entry records the confirmation, and later changes follow the
  golden-change procedure.

### 8.4 Gate (§4)

Run set matches goldens under AOT; JIT≡AOT≡golden is the default
`cargo test`; reload demonstrated on a run-set program; device-triple
link green for a run-set entry.
