<!-- §43 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 43. R16 — absence-capable Q32-alias descriptor members

Owner decision 2026-08-02 (downstream R16, re-sent; not blocking
but shaping the downstream's public API wrongly — its E2 shipped
WebGPU's `compare` member required, which makes every explicit
sampler descriptor a comparison sampler). Scope taken at the
downstream's offered minimum: **Q32-alias members only** — the one
value type with a spare representation for "absent" by
construction, and the place absence is semantically loaded in
WebGPU IDL. Probed before contracting (`--lib es2022` standalone,
exit 0): the declaration form, the presence test with narrowed
reads, and template prints are stock-`tsc`-clean; an explicit
`undefined` member value is also `tsc`-accepted, so its rejection
below is strictly narrower.

### 43.1 Surface

Inside a `@Descriptor` class, `name?: A` with **no initializer**,
where `A` is a Q32 string-literal union alias (§24), declares an
**absence-capable** member. In a literal, omitting the member means
*absent* — a state distinct from every member value; supplying it
means that value. Absence is only spellable by omission: an
explicit `undefined` member value rejects (r117, `tsc`-clean).
`?`-without-initializer for every other member type keeps its
existing rejection (r93 stands, now pinning that boundary).

Reads go through presence narrowing only: `expr.m !== undefined` /
`expr.m === undefined` on an absence-capable member is the **single
legal appearance of the `undefined` token in the language** (C7's
ban stands everywhere else — r13 unchanged; collisions C7 note
amended). The test narrows exactly as `!== null` narrows nullable
references — inside the presence arm the member reads as `A`; an
unnarrowed read rejects (r118 — `tsc`-clean, template positions
accept `undefined`).

### 43.2 Runtime and boundary

Absent is a reserved discriminant outside the alias's member set
(representation is the implementer's; observable behavior is the
contract). The §24 formatting string table never sees it — reads
are narrowed by construction. Both tiers byte-identical. **No
bindgen change**: the request is script→C only, and the sentinel
write at the C boundary is the downstream generator's code built
from presence tests.

### 43.3 Corpus

`a118-absence-capable-member` (accept): the sampler shape — an
absence-capable alias member beside a defaulted scalar; literals
covering present, absent-with-other-members, and `{}`; presence
tests driving both arms with narrowed prints. Rejects:
`r117-explicit-undefined-member` (`tsc` status recorded — probed
clean), `r118-unnarrowed-absence-read` (`tsc` status recorded).

### 43.4 Exit criteria (pre-registered)

1. `a118` byte-identical under both tiers.
2. `r117`/`r118` pin (code, line) with `tsc` statuses recorded;
   `r93` retained.
3. Narrowing unit tests: presence arm reads as `A`; the negative
   arm stays absent-typed (no read); reassignment through the
   member invalidates the narrowing (the null-narrowing precedent);
   `undefined` outside a presence test keeps r13's rejection.
4. Full gate green; `tsc` gate green with `a118` included; no
   existing golden moves; zero-warning sweep green; generated-docs
   byte-compare gates green.
