# Long-running-host audit — unbounded growth and freezes

Status: swept 2026-07-29 (owner request, after the dev-retention finding).
Scope: every quantity in the runtime that grows with **calls made** rather
than with **data live**, in either tier, plus every way a host frame can
fail to return. The dev-retention defect is the template: a cost that is
per-operation and has neither a bound nor a reclamation path.

Method: each candidate verified against `runtime/src` — a claim below
names the line that establishes it. Findings first, verified-clean second,
so the clean list is evidence and not decoration.

## Findings — unbounded, cost undocumented

### 1. The stdout sink grows per `print` and a C host cannot drain it

`print` appends to `Context::stdout` (`context.rs:773-774`) and nothing on
the C surface removes bytes: `sub_rt_ctx_stdout` takes a `const Context*`
and returns a pointer into the sink (`ffi.rs:4392-4400`), and the draining
accessor `take_stdout` (`context.rs:779`) is Rust-only — the in-repo
runners call it once at run end, which is why no gate ever noticed. The
capstone works around it by tracking a drained prefix and never shrinking
the sink (`examples/host/main.c`, "The runtime sink is cumulative").

Consequence: a long-running host whose script prints — logging once per
frame is the ordinary case — retains every byte ever printed, for the
Context's lifetime. The growth rate is exactly the bytes printed plus one
newline per call; no measurement is needed because the mechanism stores
the quantity itself.

Response requires an owner decision (a C API addition):
- a draining accessor (`take`-shaped: returns and clears), or
- a clear call beside the existing read, so the read stays `const`.
Either is additive; the gate reads the sink once at end, so no golden
moves. Until then the only mitigation is the context-per-scene pattern.

### 2. Callback bindings accumulate per registration and are never swept

Every marshaled callback-info crossing calls `bind_callback`, which boxes
a 5-pointer record and pushes it onto `Context::callbacks`
(`context.rs:1804-1820`). Nothing pops it: not `Context.free`, not
`Context.collect` (the vector is not in the mark or sweep paths), only
Context drop. This is deliberate — Q13's lifetime rule makes the deferred
fire correct, and the C side holds the raw binding pointer, so the runtime
cannot know when the host is done with it — but the *cost* of the rule is
recorded nowhere.

Consequence: a host that registers per frame — the a35-style async kick
takes a fresh callback-info per call, and nothing stops a game from
re-registering its sink every frame — grows one boxed record per registration,
without bound. Register-once hosts are unaffected.

Response options, none adopted here:
- document the cost at Q13 and in the generated header, with the guidance
  (register once; per-frame re-registration leaks by design), or
- an in-place rebind for the per-frame pattern (updates an existing
  binding's code/userdata rather than allocating a new one).
Reachability-based collection is not an option: the C side holds the raw
pointer, which is the very fact Q13 encodes.

### 3. A runaway script freezes the host with no recovery path

Exported calls are synchronous and nothing can interrupt one: no fuel, no
watchdog, no `sub_rt_ctx_interrupt` — the only bounded-execution
mechanism in the runtime is the regex budget (`context.rs:392`,
host-settable, added by P23 for exactly this class of fault). An accident
as small as a wrong loop bound freezes the host's frame forever; the host
cannot even time the call out, because the trap machinery runs *inside*
script execution and there is no safe cross-thread entry.

This may be a consequence the project accepts — invariant 6 trusts
scripts — but invariant 6's own framing spends effort on "clear, early
errors for honest mistakes", and an accidental infinite loop is an honest
mistake with the worst possible diagnostic: silence. The regex budget is
precedent that this class gets bounded when it can be bounded cheaply.

Response requires an owner decision; the options differ in cost:
- accept, and document at Q12 that a non-returning export is
  unrecoverable by design;
- a dev-tier-only check (e.g. a host-armed deadline tested at loop
  back-edges, trap on expiry), ship tier untouched — the §8.1a-1 shape:
  a diagnostic the dev tier can afford and the ship tier does not pay for.

## Verified clean — bounded or already documented

- **String-literal interns** — keyed by module-data address
  (`context.rs:1832-1843`); bounded by program text.
- **Astral code-point interns** — bounded by distinct scalars used;
  documented with its honest bound (P24, `compiler.md` §22.1).
- **Regex compiled-pattern cache** — retained across `Context.collect` by
  design and bounded by distinct patterns (`stdlib.md` §15.5a; test
  `collection_reclaims_handle_state_but_retains_compiled_patterns`).
  Runaway matching is bounded by the budget.
- **Ship-tier arena** — freed blocks return to size-class free lists;
  chunks are never unmapped, so ship memory is bounded by **peak** live,
  not current live (§8.1b). Bounded, and the standard arena trade.
- **`Context.collect` pause** — proportional to live data, host-invoked
  only; the cumulative-sweep defect was P24's and is fixed.
- **Freed-handle diagnostics ON** — unbounded by design, documented as
  the mode's stated cost (§8.1a-1).
- **Hot-reload epochs** — per-edit cost, dev-only, bounded by the number
  of edits in a session.

## What this file is not

No fix is designed here. Items 1–3 each need an owner decision, and item
1 and any dev-tier deadline need contracts before implementation. The
sweep records what grows, what freezes, and what was checked and found
bounded, so the next audit starts from evidence.
