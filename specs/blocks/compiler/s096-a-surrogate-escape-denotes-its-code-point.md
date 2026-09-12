<!-- §96 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 96. A surrogate escape denotes its code point

*(2026-09-09.)* Origin: the §95 Phase Review. A reviewer measured a
string escape that this language accepts and reads as a different
string than TypeScript does. Nothing records it, and no diagnostic
reports it. `CLAUDE.md` calls this class out by name: a divergence
this project did not decide is a defect that reads as a decision.

Measured at `f5aff2c` on aarch64 macOS, dev JIT, against node
v24.18.0 and tsc 5.9.2. Every program below type-checks under `tsc
--strict --target ES2022`, exit 0.

| Source | subscript | node |
|---|---|---|
| `"\ud83d\udc4dZ"` | the 13 characters `\ud83d\udc4dZ` | `👍Z`, UTF-16 length 3 |
| `"\u00e9\ud83d\udc4dX"` | `é\ud83d\udc4dX`, 15 bytes | `é👍X` |
| `` `t\ud83d\udc4du` `` | `t\ud83d\udc4du`, 14 bytes | `t👍u` |
| `"a\ud83db"` | `a\ud83db`, 8 bytes | `a` U+FFFD `b` |
| `"\udc4d"` | `\udc4d`, 6 bytes | one lone low surrogate |
| `"\u00e9|\u3042"` | `é|あ` | `é|あ` |
| `"\u{1F600}"` | `😀` | `😀` |

The parser decodes every other escape and leaves an escape in the
range `\uD800` to `\uDFFF` as its raw source characters, in a string
literal and in a template part alike. The result is not a divergence
in the value's measure, which Q5 already records. It is a different
string.

### 96.1 Rule

1. A **surrogate pair** — an escape whose value is `\uD800` to
   `\uDBFF` followed by an escape whose value is `\uDC00` to
   `\uDFFF` — denotes the one code point that pair encodes. The
   compiler decodes it to that code point's UTF-8 bytes.

   *(Corrected 2026-09-09 by the Phase Review.)* **The pair is made of
   values, not of spellings.** Either half is written `\uXXXX` or
   `\u{XXXX}`, and a line continuation between them joins nothing,
   because it produces no character. The rule first said "a high
   surrogate escape … immediately followed by a low surrogate
   escape", and the implementation read that as one spelling and no
   separator. Measured against node v24.18.0, which reads all four as
   `👍`:

   | Source | before | node |
   |---|---|---|
   | `"\ud83d\udc4dZ"` | accepted | `👍Z` |
   | `"\ud83d\u{dc4d}"` | rejected | `👍` |
   | `"\u{d83d}\udc4d"` | rejected | `👍` |
   | a line continuation between the halves | rejected | `👍` |

   For the last three the diagnostic said the escape "has no UTF-8
   encoding", which is false: they denote U+1F44D, and its encoding is
   F0 9F 91 8D. The remedy it named, "write the paired escape", is
   what the author had written.
   `"\ud83d\udc4dZ"` is `👍Z`, five bytes, and equals the
   source spelling `"👍Z"` byte for byte.
2. A **lone surrogate escape**, high or low, is rejected: S100 "a lone
   surrogate escape has no UTF-8 encoding; write the paired escape or
   the character", at the escape.

2a. *(Added 2026-09-09; the round measured a shape where rule 2 does
   not hold.)* **The diagnostic does not depend on where the literal
   sits.** One shape breaks that today: a high surrogate escape
   followed by another high surrogate escape, in a call argument.

   | Program | diagnostic |
   |---|---|
   | `const s: string = "\ud83d\ud83d";` | rule 2's, at the first escape |
   | `print("\ud83d\ud83d");` | `parse error: Invalid character in identifier`, at the **second** escape |
   | `print("\udc4d");` | rule 2's |
   | `print("\ud83dA");` | rule 2's |
   | `print("\ud83d\u{41}");` | rule 2's |

   The source declares no identifier. The lexer raises the lone
   surrogate error with the cursor past the first escape, the string
   token is abandoned, and recovery re-reads the second escape outside
   a string, where `\u` begins an identifier escape.

   The fix is the fork's recovery: a lone surrogate uses the
   recoverable emission the lexer already has (`emit_error` in
   `src/lexer/util.rs`) and returns a replacement character, so the
   string token completes and lexing resumes after the closing quote.
   The value is irrelevant because the program is rejected.

   This predates the value-based pairing of rule 1: for two high
   surrogates the old lookahead and the new one both find no low half
   and error at the same point. It arrived with §96's first fork
   change.

   `corpus/reject/r197-high-surrogate-before-high.ts` is the Red. It
   does not land until the fork carries the fix. UTF-8 encodes no surrogate, and Q5
   makes every string UTF-8. `tsc` accepts the form, so the diagnostic
   carries a divergence block with the new `Divergence` variant
   `LoneSurrogateEscape`.
3. Rules 1 and 2 hold in a string literal and in every static part of
   a template literal.
4. A pair is recognized across two escapes with nothing between them
   but line continuations. A high surrogate escape followed by a
   literal low surrogate character cannot occur, because a lone
   surrogate is not valid UTF-8 source. A high surrogate escape
   followed by anything else is rule 2. *(Corrected 2026-09-09 with
   rule 1; this rule said "adjacent" and "any other character", which
   excluded a line continuation and a brace spelling.)*
5. The measured length stays Q5's byte count.
   `"\ud83d\udc4dZ".length` is 5 here and 3 under node, which is
   Q5's recorded divergence and not a new one.
6. Every other escape keeps its behaviour, including `\u{...}` above
   the basic plane, which already decodes.
7. *(Added 2026-09-09, after the round measured it.)* **An identifier
   escape is not covered.** The lexer's Unicode routine also serves
   identifier escapes, and TypeScript rejects a surrogate pair there.
   Measured with tsc 5.9.2: `const \ud801\udc00 = 1;` gives TS1127
   "Invalid character" at each escape, and `const \u{10400} = 1;` is
   accepted. Rule 1 applies to a string literal and a template part
   alone. An identifier keeps `InvalidIdentChar`, and a valid
   supplementary brace escape in an identifier stays accepted. A
   prototype that decoded pairs in the shared routine accepted an
   identifier that TypeScript rejects; that is the shape this rule
   forbids.

### 96.2 Sites

*(Decided 2026-09-09, after the round measured the AST.)* **The fix
belongs in the fork.** The round measured that `Str.value` and a
template part's `cooked` are identical for `"\ud83d"` and for
`"\\ud83d"`, an escaped backslash followed by the same characters.
Only `Str.raw` and the template's `raw` separate them. Core principle
8 names that state: a consumer needs a fact the form does not carry,
so the form is wrong.

A compiler-side fix would read `raw` and decide, for each `\u`, whether
an odd run of backslashes precedes it. That is a second escape scanner
that has to agree with the lexer's on every input, which is the
duplicate-implementation class this project makes unreachable rather
than fixes twice.

The fork already carries the capacity. `Char` is `Char(u32)`, so it
holds a surrogate value; only `read_unicode_escape`'s fallback throws
it away, resetting the cursor and pushing the raw characters when
`char::from_u32` returns `None`
(`src/lexer/mod.rs`, `read_unicode_escape`).

Fork, `https://github.com/infosia/swc_ecma_parser`, branch
`subscript-eof-bump`:

1. A high surrogate escape immediately followed by a low surrogate
   escape becomes the one code point that pair encodes. `Str.value`
   and `cooked` then carry the correct string, and rule 1 needs no
   change in this compiler.
2. A lone surrogate escape raises a lexer error instead of falling
   back to the raw characters. Add a `SyntaxError` variant for it if
   the enum takes one cheaply; report which mechanism the round used.

This compiler:

- `compiler/src/parse.rs` maps a parser error to S100 with
  `err.kind()` in hand. Match the lone-surrogate kind and emit §96.1
  rule 2's text with `Divergence::LoneSurrogateEscape` at the error's
  span, in place of the generic `parse error: …` text.
- `compiler/src/divergence.rs`: the `LoneSurrogateEscape` variant and
  its table row, citing `compiler.md §96`.
- `compiler/src/check/expr.rs` `check_lit` and the template arm need
  no change: they read a value the fork has already made correct.

The owner pushes the fork branch; this project then bumps the pinned
commit in `compiler/Cargo.toml` and `Cargo.lock`. A round that cannot
push reports the patch and its measurements, and stops.

### 96.3 Corpus and gate (pre-registered exit criteria)

Red first, at the contract pin: the seven measured rows above,
recorded on this host with their exit codes.

1. One accept entry: a surrogate pair escape beside the same
   character written literally, asserted equal; a pair inside a
   template part; a pair adjacent to a BMP escape and to `\u{...}`;
   and the byte length of each. `js-comparable: no Q5`, with the node
   result in the header beside this language's.
2. One reject entry per lone-surrogate shape: a lone high surrogate,
   a lone low surrogate, and a high surrogate followed by an ordinary
   character. Each header states `tsc: accepts`.
3. Unit tests in the same commit: the decoded bytes of a pair equal
   the bytes of the literal character; a lone surrogate at the start,
   in the middle, and at the end of a literal each report at the
   escape; a template part reports at the escape; a positive control
   in each shape that decodes and runs.
4. Gates: `tools/gate.sh full` green in both profiles; clippy at the
   7/18/13 baseline; `cargo fmt --check`; the `tsc` gate; every
   pre-existing golden byte-identical. No golden is expected to move.
