<!-- §69 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 69. The language definition, checked instead of asserted

Origin: the owner asked on 2026-08-26 whether a language rule can be
checked instead of written as prose, and named `node` and `tsc`
output as the oracle to check against. **No language surface moves.**
This section adds checks. It decides no collision.

CLAUDE.md now states the boundary: an external implementation is a
**divergence detector**, never an oracle. A disagreement is a defect
in this compiler, or a divergence that `collisions.md` must name. A
disagreement never corrects a golden.

Measurements at `af5697d`, on this host. `node` is v24.18.0 and
`tsc` is 5.9.2.

1. **`collisions.md` is 1221 lines of prose, and nothing checks that
   its list is complete.** The file states where this language
   differs from JavaScript. A divergence this project did not decide
   reads as a decision.
2. **Two divergences are measured and are in no collision entry.**
   §66 measurement 6i: `node` resolves a name in the temporal dead
   zone as `4` and this compiler as `3`. §66 measurement 6j: the
   duplicate-declaration diagnostic. Both sit in a tracking file.
3. **32 entries assert what `tsc` does, by hand, in three
   spellings.** *(Re-measured 2026-08-27 at `e598994`: 303 entries,
   152 accept and 151 reject.)* `tsc-clean-standalone` appears 25
   times, `tsc-status` 6 times, and `tsc-clean` once. One concept
   with three words breaks this project's own rule that one concept
   takes one word. The other 271 entries assert nothing, so a reader
   cannot tell a measured silence from an unasked question. This
   session got the direction of invariant 5 wrong once, and the
   coding agent found it.
3a. **The hand measurements are already in the headers, unrepeated.**
   One reads "exit 2 (TS2345 at 22:14) verified with
   `node_modules/.bin/tsc --noEmit --strict ...`". A person ran that
   once and wrote the answer down. Nothing re-runs it, so it is a
   claim about another system that the gate does not hold — which
   CLAUDE.md's rule about running another system exists to prevent.
3b. **13 reject entries state no `expected-error` at all**:
   `r52`-`r59`, `r121`-`r123`, `r138`, and `r139`. A reject entry
   that names no diagnostic pins the rejection and not its reason,
   so a later change can reject it for a different reason and the
   gate stays green.
4. **`r153`'s header already records a `node` observation by hand** —
   "node reports a temporal-dead-zone ReferenceError". The work
   below turns that kind of note into a measurement.
5. **78 of 148 accept entries use no `Context`, no `@CStruct`, no
   `FixedArray`, and no foreign call.** That is the rough upper
   bound of the comparable subset. The real number is lower, because
   an entry that depends on integer wrap or on a trap is not
   comparable either.

### 69.1 Three stages

**Stage 1 — every `tsc` claim becomes a measurement.** The gate runs
`tsc` on every corpus entry. An accept entry type-checks. A reject
entry's header states what `tsc` does, and the gate confirms it. A
header that disagrees with `tsc` fails the build.

**Stage 2 — `node` runs the comparable subset.** Each accept entry
carries a `js-comparable` header. The gate runs the comparable
entries under `node` and compares the output against the committed
golden, byte for byte.

**Stage 3 — the collision table becomes an index.** Each collision
carries an id. Each `js-comparable: no` cites one. Each id has at
least one corpus entry. The gate checks all three.

*(Measured 2026-08-27.)* `collisions.md` already numbers C1 to C12,
and §2 carries the Q-register resolutions separately. So stage 3 is
smaller than this section first assumed: the ids exist, and the work
is the two directions of the check, plus ids for whatever §2 decides
that C1 to C12 do not cover.

### 69.2 The headers are the data

1. `js-comparable: yes` — the entry runs under `node` and prints the
   golden.
2. `js-comparable: no <collision-id> …` — the entry diverges by
   decision. It names every collision that applies.
3. **There is no third state.** An accept entry with no
   `js-comparable` header fails the build. 148 entries each get a
   decision, and "not looked at" is not one of them.
4. A reject entry's `tsc` header states `accepts` or `rejects`, and
   the diagnostic code when `tsc` rejects.

### 69.3 The `node` harness

1. **One JS file implements the ambient surface** the comparable
   subset uses. It is small, because the subset avoids `Context`,
   value classes, and foreign calls.
2. **The shim never grows to make an entry comparable.** An entry
   that needs a shim the file does not have is `js-comparable: no`,
   with the reason. A shim that emulates a decided divergence would
   hide the divergence, which is the opposite of the goal.
3. **A total check ties the shim to the prelude.** Every name the
   shim defines exists in `prelude/lang.d.ts`. A shim that drifts
   from the prelude tests nothing.
4. **`tsc` is pinned exactly. `node` is pinned to its major line.**
   *(Corrected 2026-08-28. This read "`node` and `tsc` are pinned",
   and the harness read it as one exact equality for both. The two are
   not symmetric, and one rule for both states a requirement neither
   owns.)*

   `package.json` and its lockfile **install** `tsc`, so the repository
   controls that version, and an exact check compares the record against
   something the repository put there. It fails only for a stale
   `node_modules`, which is a real defect and a cheap one to report.

   The repository does not install `node`. `node` is the host's
   interpreter, and `engines` declares a version rather than supplying
   one. An exact equality therefore fails on every host that has not
   matched a patch release by hand — a failure that reports the host, not
   a divergence, and that stops the whole gate from running.

5. **The golden comparison detects a `node` divergence; the version does
   not.** If `node` changes an observable, §69.5 criterion 4 fails and
   names the entry and the bytes. A version equality adds no detection.
   It moves attribution earlier, and it fires on the runs where nothing
   differs at all. The major line is what the pin must hold, because a
   major release brings a new V8, and that is when a person re-measures
   the record rather than reads past it.

6. **The record states the measured version, not the pinned one**
   (§69.5 criterion 6). The gate prints the `node` and `tsc` versions it
   ran, so the record follows the run. A failure on the major line names
   the version the record was measured on, so a reader can tell a host
   mismatch from a divergence without leaving the message.

### 69.4 What a disagreement means

A disagreement between `node` and the golden is one of two things,
and the round decides which and reports it:

1. **A defect in this compiler.** Fix it. The golden moves only
   because the compiler was wrong.
2. **A divergence this project decided.** Add the collision entry,
   with the measured outputs of both sides, and mark the corpus
   entry `js-comparable: no` citing it.

**A disagreement never corrects a golden on its own.** `node` is not
the oracle. Where this language decides to differ — integer types,
value types, a trap where JavaScript gives `undefined` — `node` is
wrong about this language, and the collision entry says so.

### 69.5 Corpus and gate (pre-registered exit criteria)

1. Every reject entry's `tsc` header is measured, and every accept
   entry type-checks. 151 and 148 at this pin.
2. Every accept entry carries a `js-comparable` header. No entry is
   undecided.
3. Every `js-comparable: no` cites an id that `collisions.md`
   defines. Every id `collisions.md` defines has at least one corpus
   entry. Both directions are checked.
4. Every comparable entry's `node` output equals the golden, byte
   for byte.
5. **The two divergences of measurement 2 gain collision entries**,
   with the measured output of each side. That is the sharpest test
   of this section: it exists because those two were measured and
   never recorded.
6. The record states the count of comparable and non-comparable
   entries, and the `node` and `tsc` versions.
7. Gates: the standing gate is unchanged, and no committed golden or
   `.expected` moves. This section adds checks and moves no output.
8. **Tracking**: `specs/tracking/s69-checked-language-rules.md`.
