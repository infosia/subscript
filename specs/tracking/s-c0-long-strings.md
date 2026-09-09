# C0 — a static C byte array for long string constants

A measurement round, 2026-09-09. **Nothing landed.** The prototype
lives on the branch `c0-long-string` (`055f20b`) and never merges into
`main`. It bypasses `collisions.md` C15 on purpose, which is the rule
under test (`CLAUDE.md` workflow step 0).

Origin: the owner's request to reconsider three decided restrictions
on their merits. This is the third, and the only one that needed a
measurement before a contract.

Pin: `e2a2b6da53eb73314321ef5c601fc9c592402f73`.
Host aarch64 macOS, Apple clang 21.0.0 (`clang-2100.1.1.101`),
rustc 1.95.0.

## The question

C15 caps a decoded string literal at 65,000 bytes with S019, because
the ship tier emits it as a C string literal and MSVC caps a
concatenated literal. The cap is a language rule chosen for one
emitter's representation. C15's own reason cites MSVC's 65,535-byte
cap as *(docs)*; the one measured point is `error C2026` on a
32,768-character literal.

The runtime needs no such representation. Both language-data emission
sites already pass a pointer and an explicit length.

## Red at the pin

A single ASCII literal of 65,001 bytes:

```
error[S019]: string literal of 65001 bytes exceeds the ship-tier limit of 65,000 bytes
```

exit 1.

## Measured under the prototype, clang

Timings are one wall-clock sample around the clang subprocess,
covering compilation and linking only. Every emitted `entry.c` is
19,646 bytes and is excluded from the `program.c` column.

| Case | Decoded bytes | program.c bytes | Compile + link s | Linked bytes | Output |
|---|---:|---:|---:|---:|---|
| short | 31 | 23,743 | 0.146 | 8,336,280 | `31 2015` |
| at the limit | 65,000 | 88,770 | 0.134 | 8,402,328 | `65000 4225000` |
| one past it | 65,001 | 369,089 | 0.134 | 8,402,360 | `65001 4225065` |
| 1 MiB | 1,048,576 | 5,594,310 | 0.505 | 9,393,080 | `1048576 68157440` |
| mixed, literal | 65,001 | 370,105 | 0.120 | 8,402,360 | printed twice, equal |
| mixed, template | 65,001 | 370,601 | 0.128 | 8,402,360 | printed twice, equal |

Every output matches an independently calculated byte length and
unsigned-byte sum. The mixed payload is
`("é\0\n\\\"A" * 9285) + ("B" * 6)` after escape decoding: 65,001
bytes, byte sum 5,246,421. It covers a multi-byte code point, an
embedded zero, a newline, a backslash and a quote. The emitted array
bytes were compared against the complete expected sequence, not only
the sum.

Verified again here on the branch, dev tier: 65,001 bytes gives
`len=65001 sum=13641` and 1 MiB gives `len=1048576 sum=0`, both equal
to the value computed by hand from `97 * n` under the program's
running mask.

## The cost, which the decision must weigh

One byte past the threshold takes `program.c` from 88,770 to 369,089
bytes, because each data byte becomes a numeric initializer. The 1 MiB
case emits 5,594,310 bytes of C and quadruples compile time against
the short control. The linked binary grows by roughly the data size.

The expansion is linear with a constant factor near 5.4. The round
verified that no per-call stack buffer is proportional to the literal,
and that emission below the threshold is byte-identical to the
unpatched emitter on a corpus entry containing string literals.

## Open, and why there is no contract yet

**C0 is pending on MSVC.** A clang result does not establish MSVC
compatibility, and MSVC is the compiler whose cap created C15. Two
ways to run it:

- `tools/c0-windows.ps1` on the branch, from a Developer PowerShell.
  It builds this compiler, emits and links four cases through the
  MSVC ship tier, and writes `c0-windows-report.md`.
- `REPORT-c0-msvc.zip`, a self-contained package the round built,
  needing no checkout and no network.

The contract that follows the measurement must decide: whether S019
disappears or an independent limit replaces it; a measurable source,
compile-time and binary-size budget on both compilers; embedded zero
handling and the explicit runtime length; whether identical sites
share a data symbol, against the existing pointer-based intern cache;
and the corpus shapes, including the threshold, Unicode and repeated
evaluation.

No numeric cost threshold was set in advance, so these samples alone
cannot establish acceptable cost. That is the owner's call, and it is
the reason this round stops here rather than proposing a rule.
