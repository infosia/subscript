<!-- §36 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 36. OBS-1 — emitted C names every referenced boundary typedef

Owner decision 2026-08-01 (downstream observation OBS-1, accepted
as a bug): a boundary class referenced **only in null position**
(every use passes `null` for its `X | null`; no construction, no
member access) was omitted from the ship tier's emitted typedefs
while the emitted call signatures still named it — an accepted
program failing late at C compilation. The dev tier, with no C
emission, was unaffected; no corpus entry had the shape, which is
why the gate never saw it.

Rule: the C emitter's type-reachability walk keys off **referenced**
boundary types — any type named by an emitted signature, field, or
element — not off constructed/accessed types. An accepted program's
emitted C compiles; "accepted" may not mean "fails later at the C
step".

Corpus: `a109` (accept) — a program whose only use of a boundary
class is passing `null` where `X | null` is expected, verified
end-to-end under both tiers (the ship path compiles and runs).
Staging is Red-first: reproduce the C compile failure at the current
pin before fixing; the entry lands with the fix.

Exit criteria: (1) the pre-fix reproduction is recorded (the C
error text); (2) `a109` byte-identical under both tiers; (3) an
emitter unit test pins that every typedef named by emitted
signatures is defined in the emitted C; (4) no existing golden
moves; full gates green.
