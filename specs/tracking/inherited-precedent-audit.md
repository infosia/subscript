# Inherited-precedent audit — pre-registered

Status: pre-registered 2026-07-28 (owner request). Not yet run. Runs at
or after P25's Phase Review.

## The defect class

**A requirement carried from an existing artifact by analogy, without
re-deriving whether the destination needs it.**

The convention is correct at its source, so it survives review: a reviewer
checking the work against its own specification finds compliance, and the
question "what does this requirement buy *here*" is never asked.

## The instance that produced this audit

`examples/engine/engine.h` grew an `engWorldChecksum`, and the Stage 0
handoff required it, because `corpus/interop/interop.c` and
`corpus/accept/a23-game-loop.ts` fold state into checksums.

Both sources have a reason the destination does not share. The interop
fixture's only observability channel is a callback's `message.length`.
A corpus entry wants a compact total over state, because it is a
regression signal. Teaching material wants a value a reader can check by
eye, and a golden discriminates a printed position more strongly than a
hash. `examples.md` §1 now carries the rule.

Two review passes over the facade did not catch it. The second read
`engWorldChecksum` and `engFloatToInt32` directly and proposed a *fix
inside the requirement* — scaling before truncation — rather than
questioning the requirement. The owner caught it, by asking what the
checksum was for.

## A second instance, from the same handoff

The Stage 0 handoff also required "invalid or NULL handle -> no-op". The
NULL half is deliverable; the *invalid* half is not — detecting a released
handle means reading its storage, which is the undefined behaviour it
claims to guard against. The requirement was written by analogy with
defensive C, not derived from what C can promise, and the facade
implemented it faithfully across eleven declarations.

Same shape as the checksum: a requirement whose source is a convention
rather than a derivation, implemented correctly, and invisible to a review
that checks compliance. Found 2026-07-28 by the fresh-context review, not
by the author of the requirement.

## Scope of the sweep

Artifacts authored in the same session as that instance, and every rule in
them that was copied from an older document rather than derived:

1. `specs/blocks/examples.md` — every clause traceable to `corpus.md` or
   `compiler.md`. The `.expected` golden convention and the derived-set
   gate rule are justified in §7 and are expected to survive; the example
   set in §6 is not yet written and each entry's design is in scope when
   it is.
2. `specs/blocks/compiler.md` §23 — in particular the provenance record
   kinds in §23.3. One is already suspect: the string-view record for a
   *callback's* parameter has no consumer, because §23.4 declares the
   trampoline with a local layout-identical struct rather than the
   header's type. It exists because the fixture's trampoline declaration
   mentions `SubStringView`. Settle it as required-or-removed, with the
   consuming site named if required.
3. `examples/engine/engine.h` and `engine.c` — every declaration, against
   `examples.md` §4. The five interop patterns are required by contract;
   anything beyond them needs a reason at its site.
4. The examples gate crate, when it exists — every test asserting
   something the corpus gate asserts, checked against what an example
   can actually go wrong at.

## Method

For each rule or declaration: name what it buys, and the contract clause
or measurement that establishes it. A rule that cannot be traced to one
is removed, not annotated.

The check is deliberately not "is this correct" — the instance above was
correct C that computed a correct hash. It is "does the destination need
this at all".

## Pre-registered outcome

Every hit is either removed, or kept with its justification recorded at
its site. The count of hits is reported even when zero: a sweep that
reports nothing found is evidence only if it says so explicitly.

Findings land in this file with the trail, as the instance above is
recorded.
