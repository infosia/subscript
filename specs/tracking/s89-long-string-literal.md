# §89 — R40: a long string constant is adjacent C literals

Status: **landed** at `97e1110`. Contract:
`specs/blocks/compiler.md` §89 (`79451c2`). Request:
`HANDOFF-R40.md` from subscript-typegpu (`main` at `05a9cca`),
2026-09-05; the red is the downstream's W8 record on windows-msvc
(`error C2026` on a 32,768-character literal).

## Decisions

- Item 1 (split length): **4,000 source bytes per piece**, decided by
  the orchestrator on the owner's behalf. A byte escapes to at most
  four characters, so a piece is at most 16,000 characters as
  written, under MSVC's 16,380 *(docs)* in every case; 16,000 bytes
  of non-ASCII would be 64,000 characters. One sound answer.
- Item 2 (above 65,000 result bytes): **open, the owner's.** Two
  options in §89.1 rule 3 (a `static const unsigned char` array, or a
  checker diagnostic). Until then the emitter writes adjacent
  literals past 65,000 bytes and MSVC reports its concatenation
  limit *(docs)*. Known limit, recorded here.

## Round 1

Red at the emitter (this host's clang accepts the long literal, so
no run can be red here): the five-case unit test on the unchanged
`c_string_literal` — (a) passed, (b) (c) (d) (e) failed, first text
`case b: piece count left: 1 right: 2`. Green after the split.
`corpus/accept/a183-long-string-literal` (20,000-byte literal;
prints the length and the first and last eight bytes): identical on
the dev JIT, the ship tier, the interpreter, both profiles, and
under `node`. The emitted C holds five adjacent pieces; the longest
line is 4,065 characters.

Fresh review: CRITICAL 0, MAJOR 0, MINOR 5 — an `eprintln!` left in
the a183 test; the unit test outside a `#[cfg(test)] mod`; the bare
`4000` without a named constant; this note; `HANDOFF-R40.md` stays
unstaged. The three code items are open cleanups for the next
codegen round. The reviewer confirmed the arithmetic (0, 4,000,
4,001, 8,000, all-`0xff`), every call site's context, the
line-based checks unaffected, and the Red/Green claims.

## Windows

The windows-msvc result is pending that host's next run (§89.3
item 4); the downstream's W8 program is the witness there.

## Response to the downstream

Re-pin candidate: `97e1110`. The downstream keeps
its chunked atlas module by its own choice; one string is now legal
up to 65,000 bytes on MSVC.

## Gate

```text
gate full c6323c0ab1c434a329c7a269526e6bbf42b6fb7d dirty:6 debug 1285/0/2 release 1283/0/2 skips 2/0 clippy 7/18/13 goldens-moved 0 exit 0
```

## Round 2 — rule 3 decided (2026-09-06)

Owner decision: a checker diagnostic, S019, at 65,000 bytes; not the
array form. `collisions.md` C15. The reject entry r184 and the
four-case unit test are §89.3 item 5.

### Round 2 result (at `bd5dfec`)

Red: r184 (a 65,001-byte literal) checked clean before the fix;
measured at `bd5dfec`, not `97e1110` as the contract first said
(corrected at `fabb363`). Green: S019 with one shared constant, the
decoded length, the literal's position; `divergence.rs` gains the
entry citing C15; `corpus_reject.rs` loses its count pin (the §88
shape, the one file §88 did not list); the language reference is
regenerated and now carries the 65,001-byte excerpt line (generator
output).

Fresh review: CRITICAL 0, MAJOR 1, MINOR 5. The MAJOR was a
contract ambiguity: "the static text of one template literal" read
as the sum of the parts, and the emitter writes each part as its own
literal, so the limit is per part; decided at `fabb363` (forced by
the emitter's form). MINOR: the Red pin; a decoded-vs-source test
case; two `///`; the thousands formatting holds below 1,000,000; the
excerpt line. Round 3 takes the code items.

### Round 3 — landed at `05195b4`

Per static part; six hand-written cases (65,000 accepted, 65,001
rejected with the message naming 65001, one part of 65,001 rejected,
two parts of 40,000 accepted, short parts with a long run-time
result accepted, 20,000 × `\u00e9` accepted); `///` on the constant
and the helper. compiler 426; clippy 7; workspace build 0 warnings.

Gate (the first after the `target/` cleanup below):

```text
gate full 0f31668a9777baff581908ceb33ad79c66e12b4e dirty:8 debug 1286/0/2 release 1284/0/2 skips 2/0 clippy 7/18/13 goldens-moved 0 exit 0
```

R40 is closed: rule 1 at `97e1110`, rule 3 at `05195b4`. The
windows-msvc result stays pending that host.

## Windows (2026-09-06)

The owner's `tools/gate.sh full` on `x86_64-pc-windows-msvc` at
`e45ba42` (`specs/tracking/windows-portability.md`, "§85 gate script
— the first Windows run") reports `debug 1262/0/2 release 1260/0/2
goldens-moved 0 exit 0`; the golden suite there builds every accept
entry's ship tier with `cl`, a183 among them. §89.3 item 4's host
result is that record: the five-piece literal compiles under MSVC.
The downstream's W8 program is not in this corpus; a183 is the
witness of the same shape at 20,000 bytes.
