# P23 — regular expressions. IN PROGRESS

Contract: `specs/blocks/stdlib.md` §15, `collisions.md` Q31. The phase
reverses a permanent non-goal, so §15.1 carries the evidence the
reversal required rather than asserting it.

Created 2026-07-27 by the Phase Review, which found this file cited by
§15.1 and absent — so the one item §15.1 defers was carried forward to
nowhere. Workflow step 5 had been unmet for the whole phase.

## What shipped

`RegExp` (literal and constructor), `test`, `search`, `replace`,
`replaceAll`, `split`, `source`, `flags`, and ambient
`matchStart`/`matchEnd` for capture extents. Rejected, each naming the
language gap rather than the surface: `exec`, `match`, `matchAll`,
`lastIndex`, `groups`, sticky `y`.

The engine is `regress`, forked for an execution budget
(`infosia/regress`, branch `subscript-exec-budget`, pinned in
`Cargo.lock` at `1e1d0a90`). Upstream has no budget at any version, and
that is not an oversight: ECMAScript specifies backtracking with no
execution limit, so a budget would make `regress` stop being JS. This
language traps instead, for the same reason Q20 traps on Invalid-Date
and Q28 traps on `NaN` — the host cannot interrupt a script call, so an
unbounded match is a hang the frame cannot recover from.

## The contract was corrected six times, twice on the same number

Recorded once, as the phase's own lesson: **§15's pre-registrations
were wrong about half the time, and the two worst errors were both
measurement methodology, not design.**

1. `regress` was to be vendored at the `v0.11.1` tag. The commits after
   the tag include `Harden regress against stack overflow`, which
   removes the nesting hazard §15.4 had written a shim workaround for.
   Branching from the tag would have re-introduced a fixed defect.
2. The budget patch was written against 0.10.4 and did not port to the
   vendored tree; its design was reused, the file discarded.
3. Overhead was reported as "none" (0.10.4), then 11–15% (0.10.4), then
   **2–7%** on the vendored tree. Only the third is cited.
4. A **git submodule** was contracted on a misreading of CLAUDE.md's
   "pin external sources as git submodules or fetched artifacts" as an
   exhaustive list. The rule forbids **paths**; a Cargo git dependency
   is a fetched artifact pinned by `Cargo.lock` and satisfies it with
   less machinery.
5. An **off-by-default `regex` feature**, argued from binary size.
   Removed once the size was measured properly — see below. It would
   have made *what the compiler accepts* depend on a build flag, which
   this project has nowhere else.
6. The binary-size table, twice. Below.

## The size number, and the thing it was hiding

§15.1 first put the regex charge at **+537 KB** (0.10.4, trivial call
site), then corrected it to **+5.12 MB** and declared the first figure
"wrong by an order of magnitude". The second correction was the wrong
one.

The +5.12 MB compared a **Context-only** program against a
**Context + regex** one. Regex reaches string construction; the
baseline had not. So the difference charged regex for a table the
baseline simply had not linked yet.

arm64, ship-C, `-O2`, `-Wl,-dead_strip`, stripped, four programs each
naming what it reaches:

| program | linked | Δ |
|---|---:|---:|
| `main` returning 0 | 16,824 B | — |
| + create a Context | 323,536 B | +306,712 B |
| + print a string | 4,814,904 B | **+4,491,368 B** |
| + call a regex | 5,447,032 B | **+632,128 B** |

By link map: `regress` **501,433 B**, `subscript_runtime` 4,832,058 B.
The Phase Review re-derived both independently and agreed on the
`regress` figure exactly; its regex delta is 615,024 B, differing only
because its print-only baseline reaches slightly more.

So **+537 KB was approximately right** and the "order of magnitude"
verdict is withdrawn.

**The largest single item in a shipped binary is this project's own
`context::CODE_POINT_UTF8`** — `[u32; 0x110000]`, 4,456,448 B, every
Unicode scalar's UTF-8 bytes, so `charAt` can return a handle into
static memory without allocating. It is **7× the regex engine**, every
program that touches a string pays it, it predates P23, and no size
line existed to reveal it until this phase drew one.

**Carried forward, not P23's to fix.** The table buys an
allocation-free `charAt`; the alternatives (table only the BMP at
256 KB, or ASCII with allocation above it) trade binary size against an
allocation on a hot path, which is a decision with its own measurement
and its own phase.

## The divergence that produced §15.6a

`"XXX".replaceAll(/(?<=X)X/g, "Z")` gave **`XZX`**; node gives
**`XZZ`**. The shim was searching again with `&text[start..]`, so after
the first replacement the lookbehind was looking at the start of a
slice instead of at the preceding `X`.

The fork grew a second entry point for it —
`find_from_budgeted(text, start, budget)`, whole subject, absolute byte
offsets — and §15.6a makes non-slicing contract rather than advice.
**Slicing is invisible to the engine**: it reports a match ECMA says
does not exist, and no assertion inside the regex can tell.

The Phase Review searched every repeated-search path for remaining
slicing and found none, then confirmed against node across `\b`,
lookbehind, negative lookbehind, `^`/`$` with and without `m`, and
empty-match iteration, over `replace`, `replaceAll` and `split`.

## Phase Review — 0 CRITICAL, 5 MAJOR, 11 MINOR

The engine, the budget, the start-position contract, the `$`
substituter, the byte-offset domain and cross-tier trap identity were
all verified correct — 7,537 node comparisons with zero divergence, 960
non-ASCII offset cases, and the budget trapping identically on both
tiers including on a *later* search of a repeated operation with no
partial result.

The five MAJORs were, in order: `source` escaping `/` inside a
character class where ECMA does not; the regex store growing without
bound past `collect()`; `TrapKind::Regex` shipping with no corpus
entry, no unit test and no contract line; this file's absence; and
§15.7's pre-registered binary-size gate never having been built.

**Four of the five are gaps in the gate, not in the behaviour.** The
code was right and unguarded — which is the same shape P18's review
found for array callbacks and P21's found for `collect`, and the third
time this project has shipped correct behaviour with no test that would
notice it changing.

## Carried forward

- `context::CODE_POINT_UTF8`, above.
- Upstreaming the budget. Unlikely as-is: upstream would want one
  fuel/cancellation API across the iterator, ASCII, UTF-16 and PikeVM
  paths, not a UTF-8 one-shot. The fork is expected to persist.
- Two paths outside the budget, quantified in §15.1: the prefix byte
  search (4.18 ms over 256 MiB even at budget 1) and a long
  backreference (49.7 ms inside one charged unit). The budget's
  guarantee is linear-not-exponential, not that a call fits a frame.
- §15.1's overhead table and §15.5's caching table are not reproducible
  from the repository. §15.7 now requires a cited figure to be either
  reproducible or marked as a dated one-off.
