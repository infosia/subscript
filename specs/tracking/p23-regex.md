# P23 — regular expressions. COMPLETE 2026-07-27

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

**Three of the five are gaps in the gate, not in the behaviour** — the
untested trap kind, the missing tracking file, and the unbuilt size
gate. `source` and the store were real defects. *(This line first said
"four of five", which the re-review corrected: `source` diverged from
node and the store leaked, and neither is a gate gap.)* The three that
are gate gaps are the same shape P18's review found for array callbacks
and P21's found for `collect` — correct behaviour shipped with no test
that would notice it changing.

## The fixes, and what measuring them showed

### The store — the fix is the whole point of §15.5a

Measured end to end on the ship tier (emitted C, `-O2`, linked against
the runtime staticlib, peak RSS via `wait4`), not by unit test:

| program | peak RSS |
|---|---:|
| 200 000-iteration loop, no regex | 2.34 MB |
| literal hoisted to a `const`, 200 000 `test` | 3.51 MB |
| **literal written inside the loop**, 200 000 `test` | **3.49 MB** |

The 181 MB case is gone, and the literal-in-the-loop spelling is now
**byte-for-byte the hoisted one**. The emitted C shows the mechanism:
one `static void*` per literal *site*, `sub_rt_regex_new` only inside
`ss_init`, followed by `sub_rt_root_add`.

§15.5a's predicted residual growth holds and is attributable: ten
`collect()`ed frames × 2000 **distinct** dynamic patterns → 21.36 MB;
ten frames re-using the **same** 2000 patterns → 6.72 MB against
**6.70 MB for one such frame**. Growth is compiled patterns alone
(~820 B each); per-evaluation handle state is fully reclaimed.

Sweep safety was attacked rather than assumed: a handle reachable only
from a class field, an array element, a `for…of` binding or an
arrow-function capture survives `collect()` with its match state
intact, on both tiers, because `arena_sweep` restores
`MARK_STATE`→`LIVE_STATE` before the store sweep runs.

### `source` — the fix traded one divergence for a narrower one

The first fix added a `first_in_class` rule so a `]` appearing first in
a class is literal. **That is Perl's rule; ECMAScript has none** — `[]`
is an empty class and `[^]` a negated empty class, both closing at that
`]`.

`[]]` and `[^]]` render identically under both rules, so the cases
checked at review time could not discriminate. The discriminating cases
are `[]` and `[^]` with nothing between the brackets, and a committed
test pinned the wrong value for one of them. **A test written without
running the oracle defends whatever the code does** — CLAUDE.md's rule
about running the other system applies to test tables, not only to
prose.

The rule is now plain: `[` opens, `]` closes, `\` escapes the next
character inside a class as well, and a `/` is escaped only when no
class is open. Confirmed by differential fuzz over
`a / [ ] ^ \ - ( ) d w .`, node-accepted patterns only: **63
divergences in 9,956** before, **0** after — independently reproduced
on a second seed at **31 in 5,061** before, **0** after.

The engine was never wrong. `new RegExp("[]a").test("a")` is `false`
and `new RegExp("[^]").test("a")` is `true` on both tiers, matching
node exactly. **Only `normalized_source` disagreed with the parser it
feeds** — which is why no matching test could have caught it, and why
the rendering needed its own oracle comparison.

### The rest

`TrapKind::Regex` now has five trap entries with cross-tier tuple
identity, and the checker/trap split is total — every spelling of
`replaceAll` without `g` reaches one or the other, none reaches
neither. `can_trap()` gaining four variants moved **no** existing
golden or trap position: every `.expected` in the fix commit is added,
none modified.

The size gate reproduces exactly: baseline 4,832,952 B, regex
5,447,992 B, delta 615,040 B, `regress` 501,433 B — the last figure
identical across three independent measurements.

**Nothing in `cargo test` links anything**, so the size gate is a
manually-run bin like `perf-gate`. It satisfies §15.7 as written — the
number is reproducible from the repository and the gate fails when run
— but not the stronger reading that CI would notice a drift. Recorded
rather than glossed.

## Gate

`cargo build --offline --all-targets` zero warnings; `cargo test
--offline` **599 passed, 0 failed**; **83 goldens and 45 trap entries
byte-exact across dev-JIT and ship-C-AOT**; `tsc` exit 0; `git diff
--check` clean; clippy at its 16-warning codegen baseline. No
pre-existing accept `.expected` moved at any point in the phase. Size
gate: 4,832,952 / 5,447,992 B, delta 615,040 B, `regress` 501,433 B.

Two review rounds: 0 CRITICAL, 5 MAJOR, 11 MINOR, then 1 further MAJOR
found inside the first round's own fix. All closed.

**The one MAJOR the fix pass introduced is the phase's last lesson, and
it is the same one as the size number.** Both times the error was the
measurement, not the design: an unmatched pair of programs, then a test
table written without running the oracle. The behaviour was right in
both cases and the thing checking it was wrong.

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
