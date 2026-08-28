# §69 — the language definition, checked instead of asserted

Contract `3c37cf7`, re-measured at `2bf49f1`. Landed `5631a47`,
`f523bb0`, and `06c4079`.

Origin: the owner asked whether a language rule can be checked instead
of written as prose, and named `node` and `tsc` output as what to check
against. CLAUDE.md's oracle rule was amended for it: an external
implementation is a **divergence detector**, never an oracle.

## The question, and the answer

`collisions.md` is 1221 lines stating where this language differs from
JavaScript, and nothing checked that the list was complete. Two
divergences were measured in the §66 arc and appeared in no entry.

**The list was very nearly complete.** Running `node` over 152 accept
entries produced no divergence that an existing id could not name,
after two ids were added. The value is not that it was complete; it is
that completeness is now checked, every build, for 6.4 per cent of the
compiler suite.

## Stage 1 — every `tsc` claim is a measurement

One key, `tsc`, on all 303 entries, valued `accepts` or
`rejects TSxxxx`. It replaced three spellings for one concept —
`tsc-clean-standalone` 25, `tsc-status` 6, `tsc-clean` 1 — and 271
entries that said nothing.

**The 32 hand-written claims were all correct.** The measurement's
value is the 271 silences: **117 of the 151 reject entries are
`tsc`-clean**, where 25 declared it. 92 places where this compiler is
narrower than TypeScript and nothing recorded it.

117 of 151 is 78 per cent. CLAUDE.md's non-goal — sound typing rejects
the unsound patterns the TS ecosystem is written against, as a
structural consequence and not a gap to close — now has a number.

This session verified the check fires by perturbing a header: it
reported `header says rejects TS9999; tsc said accepts`. A check whose
"0 disagreements" cannot be distinguished from a broken check is worth
nothing (CLAUDE.md core principle 9).

Cost: 0.542 s, 2.7 per cent, by batching into one `tsc --build`.

## Stage 2 — `node` runs the comparable subset

152 accept entries, `js-comparable: yes` on 42 and `no <id>` on 110.
No third state and no absent state, so every entry got a decision.

**The 42 comparable entries match their goldens byte for byte under
`node`.** 17 non-comparable entries were run to confirm the divergence
each cites is the decided one, and every one was.

**The shim is 193 bytes and defines one name, `print`.** That is the
rule "the shim never grows to make an entry comparable" working: had it
been allowed to grow, it would have emulated `Context` and value
classes, and a shim that emulates a decided divergence hides it. A
check confirms every name it defines exists in `prelude/lang.d.ts`.

**The round stopped rather than invent an id**, twice, which §69.2
requires. C13 and C14 were assigned between the stop and the resume.

Cost: 42 entries in one `node` process, 0.20 s, 3.4 per cent.

## Stage 3 — the table is an index, both ways

95 corpus names across C1 to C14, each confirmed to exist; each id
confirmed to pin at least one entry through its own `Accept:` and
`Reject:` lines; each id a header cites confirmed defined; each
`retired:<name>` confirmed absent.

Three references were missing: `r14-async` twice and `r104`, both
retired in prose. Deleting them would lose why the entry went, and
keying the check on the word "retired" in a sentence would put a check
back on prose. So a retired name is spelled `retired:<name>`.

**What the check deliberately does not do.** C1, C4, C5, C6, C7, C9,
and C14 appear in no `js-comparable: no` header, and that is correct:
those collisions are mostly rejections, so their accept entries are
`yes`. A check demanding every id appear in a `no` header would report
seven false defects for ever. **This session measured that before
ordering the work**, which is why the handoff said not to check it.

Q3 to Q35 are out of scope for the corpus-name and evidence checks and
in scope for the defined-id check, because §2 carries compound ids, ids
C already resolves, and evidence written several ways. A check that
half-works was refused in favour of one that states its scope.

Cost: 0.014 s, 0.25 per cent.

## The two ids this session assigned

The owner delegated the assignment where TypeScript's behaviour cannot
be matched.

- **C13, iteration over a container that changes.** `a80` already
  decided a fixed entry bound in its own header. The array half is
  matchable at a cost; the `Map` half is not, because an append can
  rehash flat insertion-ordered storage and a cursor stable across a
  rehash is an iterator object that `stdlib.md` §14.3 rules out as a
  memory-model change.
- **C14, declaration scope and order.** The class `compiler.md` §67.1
  decided and the table carried none of. Matching is not available:
  matching means accepting the programs whose two tiers printed
  different numbers with no diagnostic.

C14 covers a class, not the temporal dead zone alone. The table's
granularity is a collision class — twelve ids covered 151 rejects
before this. That structural judgment is this session's and is recorded
so it can be reversed.

## The `node` pin is the major line, 2026-08-28

§69.3 rule 4 read "`node` and `tsc` are pinned", and the harness read it
as one exact equality for both. On a host running Node v24.16.0 against a
v24.18.0 record, stage 2 did not run at all: the assert fired before the
42 comparable entries were measured.

**The two are not symmetric.** `package.json` and its lockfile install
`tsc`, so an exact check compares the record against something the
repository put there, and it fails only for a stale `node_modules`. The
repository does not install `node`. `engines` declares a version and does
not supply one, so an exact equality reports the host, not a divergence,
and it stops the gate.

**The version check adds no detection.** §69.5 criterion 4 is what
catches a `node` divergence, and it names the entry and the bytes. The
version equality moves attribution earlier and fires on the runs where
nothing differs. The major line is what the pin must hold: a major
release brings a new V8, and that is when a person re-measures.

### Measured, not assumed

Running stage 2 on the older line is the evidence that the loosening is
safe, and it was run:

    js corpus gate: 42 comparable, 112 non-comparable, 1 shim name(s),
                    node v24.16.0, tsc 5.9.2, 0.263s

**All 42 comparable entries match their goldens byte for byte on Node
v24.16.0**, the same result §69 stage 2 recorded on v24.18.0. No
observable this corpus reads moved between the two releases.

The record keeps the exact version it was measured on, and the failure
message names it, so a reader tells a host mismatch from a divergence
without leaving the message. The summary prints the version that ran, so
the record follows the run (§69.3 rule 6).

Two unit tests pin it: one reads what "major line" means, including that
a new major and a malformed string both fail; one reads `package.json`
against the same constants, because two records that disagree are worse
than one.

### The Windows profile closes

This was the last open failure on `x86_64-pc-windows-msvc`. Both
profiles pass: 1044 in debug and 1043 in release, zero failures
(`specs/tracking/windows-portability.md`).
