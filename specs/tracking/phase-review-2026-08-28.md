# Phase review, 2026-08-28 — sections 66 to 70 and the arc after them

Status: **all five rounds landed.** This file records
the review and what it changed. The contract commits hold the rules;
the round commits hold the code.

## Why

The owner asked for a Phase Review by a fresh no-context subagent on
Fable, per CLAUDE.md. The reviews of §66–§70 that ran earlier in the
same arc were on Opus (22 of 24 subagent calls in the session named
`opus`; one inherited the Opus session). Five Fable reviewers ran, one
at a time after the first five in parallel hit the usage limit.

| Review | Range | CRITICAL | MAJOR | MINOR |
|---|---|---|---|---|
| §66–§67 | `a2228d9..f02ee3f` | 1 | 2 | 5 |
| §68 form | `f02ee3f..e598994` | 2 | 7 | 8 |
| §68 consumers | `f02ee3f..e598994` | 4 | 6 | 6 |
| §69–§70 | `e598994..7bf2559` | 1 | 4 | 6 |
| post-§70 arc | `7bf2559..2a65724` | 2 | 1 | 9 |
| | | **10** | **20** | **34** |

Six of the ten CRITICAL were defects both tiers shared, which the
differential gate cannot see (core principle 12). One was the
reference interpreter not being the third witness it exists to be.
Two were use-after-free in the reference-counted handle and in root
storage. One was the verifier not checking a rule the contract states.

## The CRITICAL findings and where each closed

| # | Finding | Contract | Code |
|---|---|---|---|
| 1 | Module-scope TDZ: a `tsc`-clean initializer reads a later global through a function; both tiers dereference zero | §67.1 rule 4c, `c7e6744` | `ea9d45c` (round 1): fixpoint over function summaries, S100, `r158` `r159` `a160` |
| 2 | A handle stored into a global, field, element, or spread is not retained; the local's release frees the frame; wrong output on both tiers | §70.3 rules 2a, 2b, `000b522` | `96e9708` (round 2): one store path, a total verifier check, `AsyncCopySite`, `a161` `a162` |
| 3 | Root storage cleared a base while an address into it was reachable through a global or a call; both tiers agree | §68.2 rule 8b, `c6cf6a6` | `135565b` (round 3): `address_taken_value`, the chain-following fixed point deleted, `a163` |
| 4 | §33.4's "escape works" rested on reads of a dead frame; a cross-activation escape terminates the dev tier abnormally and reads zeros on the ship tier | §33.4 reinstated S015, `c6cf6a6`; the withdrawal `e6a91c4` recorded as the defect | `135565b`: S015 over §33's reachable set, `r160`, `a125` and two fixtures restructured |
| 5 | The ship tier coalesced two interfering resume parameters (id-space mismatch, regression at `26403be`) | — | `135565b`: `value_interference` expands to raw ids, `a166` |
| 6 | The interpreter raised only `DivisionByZero` from a trap site; a fixed-array read past its length printed heap contents | §68.7.1, `7f769a1` | `d318367` (round 4): one dispatch over every kind at the site's position; the trap gate compares columns |
| 7 | The verifier accepted a value read after a resume that is not a successor parameter | §68.2 rule 8c, `7f769a1` | `d318367` |
| 8 | A `Local` live across a suspension is re-created at every resume on both tiers (`1,0,0` where `1,2,3`) | §68.2 item 7a, `9cc1102` | `b5ea24e`: `LocalStorageClass`, verifier check, `a164` |
| 9 | An empty template literal does not compile on the ship tier | §68.7.2, `9cc1102` | `b5ea24e`: no trap on an empty template, `a165` |
| 10 | Float→integer out of range: both tiers saturate, the interpreter wraps, nothing pins it | §68.7.2 and C3, `9cc1102` | `b5ea24e` (round 5): the interpreter saturates, `a167` |

## The MAJOR findings

Closed with the rounds above: §69–§70 M1–M4 (for-of over handles,
two leaks, C8's stale text, two analyses of one fact — all rule 2b);
§68 form M1–M7 (site position, four missing §68.7 rows, item 12's
wildcards, the self-agreeing intrinsic check, `array_base`, the
iteration machine, the `a153` exclusion); post-§70 M1 (`cross-language`
wrote a record for a run with an errored cell).

`b5ea24e` (round 5) closes §68 consumers M1–M5 (float `%`, `JsonResult.ok` by name,
the entry by name, a declaration after a label, duplicated walks) and
M6, which the owner decided: the foreign-call array snapshot is the
**call-time view** (`c3fd247`); the two interop pins `f2sync=2` and `f2=2` become `3`.

Open, by earlier owner decision: §66–§67 M1, a class field and a
method of one name (`tsc` rejects, this compiler accepts; recorded in
`s67-scoping-and-suspension.md` as a later request). §66–§67 M2 was
true at `f02ee3f` and void at HEAD.

## The MINOR findings

The text-only ones landed in `94c7fcb` (STE, line-number citations,
a147's header). The code-only ones landed with their rounds (dead
`skip_resume`, the `/tmp` path, `Suspend.invalidates` liveness, the
comment wording). Not done: the interpreter's exclusion list still
carries reasons no one re-measures; `perf_gate.rs` prints its
debug-skip reason only under `--nocapture`.

## Two things the review changed about how this project works

1. **A handoff permits a class, not a list.** Round 1 stopped twice and
   round 3 stopped three times on files the handoff did not name —
   inventory assertions, then test fixtures S015 rejects. Each list was
   incomplete because it was a list. `corpus-inventory.md` records the
   first class; the fix rounds' handoffs now say "any fixture the rule
   rejects" and "any assertion that restates the corpus size".
2. **A worktree per round.** Reviewers read the main checkout while a
   coding round edits; the round works on a branch in a git worktree
   under the scratchpad and lands by fast-forward after a gate on
   `main`. `node_modules` is a symlink into the main checkout, excluded
   through `.git/info/exclude`.

## What the owner still holds

- §67.1 rule 4c chose static rejection over a runtime trap. The
  measured cost was zero accept entries; the decision is recorded as
  the orchestrator's under delegation.
- §33.4's record stands as written: a rule from an unverified
  diagnosis, withdrawn on a measurement that did not discriminate,
  reinstated on one that does.

## After the review: the decisions the owner reopened, 2026-08-29

The owner asked which earlier owner decisions the reviewer thought
unsound or improvable. Four items were raised, and the owner decided
each the same day.

| Item | Decision | Contract | Code |
|---|---|---|---|
| A class field and a method of one name (§66–§67 M1, deferred earlier) | one member namespace per class body; a second claim fails | §67.1 rule 3a, `b31a7a3` | `127f868`: `ClassSig.member_namespace`, `r161`–`r163` |
| Static members were "not decided" | decided: separate static namespace, a static field is a module global and a rule-4c binding | §71, `e3526da` | `0cb3c8b`: `a168`, `r164`–`r168`, reload twin |
| C13's fixed entry bound bought nothing (`IteratorHasNext` already read the live count) | ECMA's rule per spelling; C13 retired; `a80` becomes comparable | `390f9d6` | `3005536`: the cursor carries its bound kind, `a80` 136→156 bytes from `node`, `a170` |
| A nullable boundary aggregate held as an address into activation storage (the root of §33.4, rule 8b, and S015) | a managed box; S015 deleted; the recursive `SubChainHeader.next` decided the box over an inline payload | §33.5, `c426ee7`; rules 9–10 for the intrusive header, `ae5662e` (forced by `e09`'s measurement) | `4ef7ea7`: `BoxBoundaryValue`, S015 deleted, `a169` `r169`, `a163` counts its boxes |
| §66 measurement 6i (a lambda reads a later `const`) | closed by rule 4; accepting it needs capture by reference (a C5 revision), recorded and not taken | `ba3a6ac` | — |

`a125` is restored to its first shape (owner, 2026-08-29): the three
`boundaryVia*` functions return the target again. §33.5 makes the
return sound, and the round-3 restructuring was a workaround for S015,
which no longer exists. The source is `96e9708`'s byte for byte; the
golden did not move; release 1094 passed 0 failed.

## The collect benchmark, 2026-08-29

The owner asked why `collect` was slow. Five standalone variants split
the 219 ms: the marker's scan of string payloads (about 121 ms) and
the string path's Rust-side temporaries (about 85 ms). §8.1c records
the decomposition and the two rules. Round A (`64a637a`): the marker
reads the class id and does not scan a handle-free block; 220 → 106 ms.
Round B (`642c295`): `alloc_str_with` and direct writers for
`padStart`/`padEnd`, the formatters, and concatenation, and no zeroing
of a handle-free block on free-list reuse; 98 → 33 ms. `collect` now
measures 1.01× of C on the ship tier and 3.30× on the dev tier. No
golden moved. The README's explanation that a generational collector
beats the arena by design was wrong and is corrected.
