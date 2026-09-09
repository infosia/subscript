# §99 — a long string constant is static C byte data

Contract: `specs/blocks/compiler.md` §99. It follows the C0
measurement in `specs/tracking/s-c0-long-strings.md`, which passed on
both supported compilers.

## Red at the pin

`146af9e`, this host: a single ASCII literal of 65,001 bytes gives

```
error[S019]: string literal of 65001 bytes exceeds the ship-tier limit of 65,000 bytes
```

exit 1.

## C0's answer, both compilers

| Decoded bytes | clang program.c | clang s | MSVC emitted C | MSVC s | MSVC binary |
|---:|---:|---:|---:|---:|---:|
| 16 to 31 | 23,743 | 0.15 | 44,002 | 0.25 | 578,560 |
| 65,000 | 88,770 | 0.13 | 109,057 | 0.26 | 643,584 |
| 65,001 | 369,089 | 0.13 | 389,345 | 0.31 | 643,584 |
| 1,048,576 | 5,594,310 | 0.50 | 5,614,608 | 1.02 | 1,627,136 |

clang 21.0.0 on aarch64 macOS; MSVC `cl` 19.44.35222 x64. Every case
exits 0 and prints a decoded length and byte checksum that a hand
calculation predicted: `97 * n mod 65536` gives 1552, 13544, 13641
and 0.

The form changes at the threshold, which the per-character cost shows
directly: 1.0 emitted byte per decoded byte at 65,000, and 5.31 at
65,001 and at 1 MiB.

## Two defects in the measurement scripts, both found on the far side

The Windows script I wrote shared one build directory across the four
cases. The binary size was therefore a running total, and every case
after the first ran `at65000.exe`, because `Select-Object -First 1`
takes the first name in sort order. `-Include *.exe -Recurse` was also
the wrong spelling for `-Filter`. The owner found and fixed both on
the Windows host, and the table above is from the corrected script.

A measurement script is evidence. Its bugs are the same class as a
test that cannot fail: the first version would have reported four
identical outputs and a monotonically growing binary and looked
plausible.

## A third scripting defect, found by the checks

The first attempt at this contract wrote C15's retired-name marker
before the entry was deleted. `collision_table_is_a_consistent_corpus_index`
rejected it: the marker and the deletion must land together. The
second attempt kept the marker's exact spelling inside the
explanatory prose, and the same check rejected that too, because it
reads the form by spelling rather than by position.

`collisions.md` states that design in its own preamble: keying the
check on a word in a sentence would put a check back on prose, which
§69 exists to end. The consequence is that prose in this file cannot
quote the marker at all.

## The round found a third language-data site

§99.2 rule 5 named two emission sites. There are three.
`codegen/src/cemit.rs` `emit_globals` writes a string-alias member
into a `SubStringAliasMember` table, and `format_value` passes that
entry's `data` and length to `subscript_rt_str_lit`. The member text
is an observable language string, not a C identifier or diagnostic
metadata.

Measured with the length check removed and only the two authorized
sites converted: `type Word = "<65,001 ASCII bytes>" | "b"`, with the
alias value interpolated, checks and runs, and the emitted C carries
the member as 17 adjacent C literals with no array. That is the shape
C15 existed to prevent, so a two-site change would have removed the
rule and left its reason standing on one path. Stock TypeScript
accepts the program.

The round also corrected a second wording defect. §99.2 rule 7 said
"per-evaluation Context allocation". `intern_literal` keys on the
static data address and the byte length, so repeated executions of one
literal reuse one allocation. The array symbol is a static address, so
interning behaves as it does for a literal, and the invariant to state
is the existing interning rather than an allocation per evaluation.

## Landed, 2026-09-09

Gate verdict:

```
gate full f6cde3d384dff68c9ccf2752862a356879175a74 dirty:16 debug 1364/0/2 release 1362/0/2 skips 2/0 clippy 7/18/13 goldens-moved 0 exit 0
```

No golden moved and the aggregate LIR snapshot did not grow.

### Verified here

- 65,001 bytes and 1 MiB both run, with lengths and checksums equal to
  a hand calculation: `len=65001 sum=13641` and `len=1048576 sum=0`.
- The alias-member shape that the round found now works:
  `type Word = "<65,001 bytes>" | "b"` prints `len=65001 sum=13641`.
  Its emitted table reads
  `{ sub_long_string_0, 65001ull }, { (const unsigned char*)"b", 1ull }`,
  so the long member takes the array form and the short one keeps the
  literal, which is §99.2 rule 5a.
- `program.c` for that program is 369,673 bytes, consistent with the
  5.31 bytes per decoded byte C0 measured.
- S019 survives only as a comment in `compiler/src/diag.rs` recording
  that the code retires without renumbering the others.
- a204 matches its committed golden.
