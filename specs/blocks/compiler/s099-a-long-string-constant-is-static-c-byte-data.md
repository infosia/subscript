<!-- §99 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 99. A long string constant is static C byte data

*(Owner decision 2026-09-09.)* Origin: the owner asked for three
decided restrictions to be reconsidered on their merits. This is the
third, and the only one that needed a measurement before a rule.

C15 caps a decoded string literal at 65,000 bytes with S019. Its
reason is that the ship tier emits a literal as a C string literal and
MSVC caps a concatenated literal. That is a language rule chosen for
one emitter's representation, and C15's own figure is marked *(docs)*:
the 65,535-byte cap was never measured, and the one measured point was
`error C2026` on a 32,768-character literal.

The runtime never needed that representation. Both language-data
emission sites already pass a pointer and an explicit length.

### 99.1 What C0 measured

The measurement round is recorded in
`specs/tracking/s-c0-long-strings.md`. Its prototype lives on the
branch `c0-long-string` and never merges.

Apple clang 21.0.0, aarch64 macOS, and MSVC `cl` 19.44.35222 x64 both
build, link and run every case, exit 0:

| Decoded bytes | clang program.c | clang s | MSVC emitted C | MSVC s | MSVC binary |
|---:|---:|---:|---:|---:|---:|
| 16 to 31 | 23,743 | 0.15 | 44,002 | 0.25 | 578,560 |
| 65,000 | 88,770 | 0.13 | 109,057 | 0.26 | 643,584 |
| 65,001 | 369,089 | 0.13 | 389,345 | 0.31 | 643,584 |
| 1,048,576 | 5,594,310 | 0.50 | 5,614,608 | 1.02 | 1,627,136 |

MSVC never reaches C2026. Every program printed a decoded length and a
byte checksum that a hand calculation predicted, on both hosts.

The source cost is the fact the decision rests on. Below the
threshold a constant costs 1.0 emitted byte per decoded byte, which is
a C string literal. Above it the cost is 5.31, which is the byte
array. The expansion is linear with that constant factor, and no
per-call stack buffer is proportional to the constant.

### 99.2 Rule

1. The 65,000-byte limit on a decoded string literal, and on one
   static part of a template literal, is removed. S019 retires.
2. No language limit replaces it. Ordinary allocation limits, address
   space, and the existing run-time string limits still apply. This
   section does not claim unlimited strings; it removes a rule that
   described an emitter, not the language.
3. The ship tier emits a decoded constant of at most 65,000 bytes as
   the C string literal it emits today, and a longer one as a
   file-scope `static const unsigned char` array with numeric byte
   initializers, passing that symbol where the literal went. The
   length passed to the runtime excludes any trailing zero.
4. **65,000 is a representation choice and carries no language
   meaning.** It is the point at which the array's 5.31 bytes per
   decoded byte stops being worth avoiding on the compilers this
   project supports. A program cannot observe which form its
   constants took, except in the size and build time of its own
   artifacts.
5. **Three** emission sites carry language string data, and each one
   changes form above the threshold. *(Corrected 2026-09-09; this rule
   first named two, and the round measured the third.)*
   `codegen/src/cemit.rs` at the direct literal site and at the
   template text site both build
   `(const unsigned char*)<literal>, <len>ull`. The third is
   `emit_globals`, which writes a string-alias member into a
   `SubStringAliasMember` table as
   `{ (const unsigned char*)<literal>, <len>ull }`; `format_value`
   passes that entry's `data` and length to `subscript_rt_str_lit`,
   so the member text is an observable language string.

   The callers that emit C identifiers, source names and diagnostic
   metadata keep the literal at every length.
5a. A string-alias member above the threshold takes the array form in
   its table entry. The discriminant machinery is unchanged: a use
   site still carries an integer, and only the table holds the text.
   Measured before this correction: with the length check removed,
   `type Word = "<65,001 bytes>" | "b"` emitted 17 adjacent C
   literals and no array, which is the shape C15 existed to prevent.
6. Each constant is emitted once per translation unit and its symbol
   is reused at its use sites. Sharing one symbol between sites with
   equal content is optional and is not required by this section.
7. Context-owned interning, lifetime, source positions and traps are
   unchanged. `intern_literal` keys on the static data address and the
   byte length, so repeated executions of one literal reuse one
   allocation and interned strings are collection roots. The array
   symbol is a static address, so interning behaves as it does for a
   literal. *(Corrected 2026-09-09; this rule first said
   "per-evaluation Context allocation", which describes behaviour the
   runtime does not have.)* The emitter must not substitute a borrowed
   static string handle, a stack array, or run-time concatenation.

### 99.3 What this supersedes

C15 retires. Its title stays for existing links, and its entry records
that both supported compilers build a 1 MiB constant.

`corpus/reject/r184-string-literal-too-long.ts` retires, and C15's
list carries `retired:r184-string-literal-too-long`. The marker and
the deletion land in one step: `collision_table_is_a_consistent_corpus_index`
fails if the file still exists when the marker appears. *(Measured
2026-09-09 while writing this section.)*

§89's R40 rules stand for the literal form, which is still what the
emitter writes at or below the threshold. §89.1 rule 1's 4,000-byte
adjacent pieces are unchanged.

`RuleCode::S019` and `Divergence::StringLiteralLength` are removed. A
rule code is not recycled: no later rule takes S019's number.

### 99.4 Sites

- `compiler/src/check/expr.rs`: `check_string_literal_length` and
  `STRING_LITERAL_BYTE_LIMIT`.
- `codegen/src/cemit.rs`: the three language-data sites, including
  `emit_globals`'s `SubStringAliasMember` table, and the file-scope
  data pool that makes a declaration available before its uses.
- `compiler/src/diag.rs` and `compiler/src/divergence.rs`: the code
  and the variant.
- `compiler/tests/string_literal_limit.rs`: rewritten around
  acceptance.
- `compiler/src/language_reference.rs`, `docs/`, and
  `generated-docs/`.

### 99.5 Corpus and gate (pre-registered exit criteria)

Red first, at the contract pin: a 65,001-byte literal gives S019 and
exit 1, recorded on this host.

1. One standing accept entry above the old limit, with a decoded
   length just past 65,000 and an explicit expected length and
   checksum rather than a dump of its content. It is the only large
   committed witness; a compact escaped source is acceptable, and the
   expected length refers to decoded UTF-8 bytes.
2. Test-time generated cases, not committed as corpus: decoded
   lengths 64,999, 65,000, 65,001, 65,535, 65,536, and 1 MiB. Each
   asserts the decoded byte length and an independently computed
   checksum, on the dev tier, the ship tier and the interpreter.
3. Coverage of ASCII, multi-byte UTF-8, escapes, an embedded zero, a
   quote, a backslash, one long static template part, and one long
   string-alias member converted to `string` (rule 5a). Bytes on both
   sides of any generated line break in the emitted array.
4. The representation boundary: emission at 65,000 bytes is the
   literal form and at 65,001 the array form, asserted by reading the
   emitted C. Emission below the threshold is byte-identical to the
   emitter before this section, proved on a corpus entry that
   contains string literals.
5. `corpus/reject/r184-string-literal-too-long.ts` is deleted with its
   harness row, and C15 carries `retired:r184-string-literal-too-long`.
6. `compiler/tests/string_literal_limit.rs` is rewritten around the
   acceptance contract. Tests for the independent literal and template
   diagnostics stay.
7. Gates: `tools/gate.sh full` green in both profiles; clippy at the
   7/18/13 baseline; `cargo fmt --check`; the `tsc` gate; every
   pre-existing golden byte-identical. The new entry is synchronous,
   so the aggregate LIR snapshot must not move.
8. The Windows half is the owner's, as C0's was. Report what a
   Windows gate would need and stop there; this host cannot run it.
