<!-- §89 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 89. R40 — a long string constant is adjacent C literals

*(Orchestrator decision 2026-09-06 on the owner's behalf for item 1;
item 2 is open.)* Origin: `HANDOFF-R40.md` from subscript-typegpu
(branch `main` at `05a9cca`, workspace pin `3677d1f`), 2026-09-05.

Measured downstream on windows-msvc (their `windows.md` W8): a
generated module held one string literal of 32,768 characters and
`cl` failed with `error C2026`. clang accepted it, so the branch
closed green on macOS. In this tree, `c_string_literal`
(`codegen/src/cemit.rs` line 8016 at `7742ab9`) escapes byte by byte
into one `"…"` and never splits; nine call sites use it (string
literals, source names, class names, positions).

Limits *(docs)*: MSVC accepts at most 16,380 single-byte characters
in one literal as written, and at most 65,535 bytes after
concatenation; C99 names 4,095 characters after concatenation as the
translation minimum, which clang and gcc do not enforce.

### 89.1 Rule

1. **A piece holds at most 4,000 source bytes.** `c_string_literal`
   writes a constant of more than 4,000 bytes as adjacent literals,
   one per line, each holding at most 4,000 bytes of the source
   string. A split lands between bytes: an escape sequence
   (`\"`, `\\`, `\ooo`) is whole in one piece. *(Item 1 of the
   request: 4,000, not 16,000. A byte can escape to four characters,
   so a 4,000-byte piece is at most 16,000 characters as written and
   under MSVC's per-literal limit in every case; a 16,000-byte piece
   of non-ASCII bytes would be 64,000 characters and fail. One sound
   answer.)*
2. **Every call site.** The rule is inside `c_string_literal`, so a
   source name, a class name, a position file, and a string literal
   all split the same way; no caller decides.
3. **Superseded by §99, 2026-09-09.** The limit and `RuleCode::S019`
   are removed, and the ship tier emits a constant above 65,000
   decoded bytes as a file-scope byte array. §99.1 records the
   measurement: both supported compilers build a 1 MiB constant, and
   the 65,535-byte figure this rule rested on was documentation
   rather than a measurement. Rules 1 and 2 stand; they describe the
   literal form the emitter still writes at or below the threshold.
   The rest of this rule is history.

   ~~**Above 65,000 bytes: the checker rejects.**~~ *(Owner decision
   2026-09-06, item 2 of the request: a diagnostic, not the array
   form.)* A string literal, or **one static part** of a template
   literal, whose decoded UTF-8 length exceeds 65,000 bytes is **S019**
   "string literal of N bytes exceeds the ship-tier limit of 65,000
   bytes" at the literal's position. The limit is a constant the
   checker and the message share. A string built at run time
   (concatenation, `repeat`, a template with substitutions whose
   result is long) is not a literal and is not limited. `tsc` accepts
   the program; the divergence is a ship-tier limit, and
   `collisions.md` gains the row. No emitter change: rule 1 already
   covers every literal the checker admits (at most 65,000 source
   bytes, 17 pieces). *(Clarified 2026-09-06 after the round-2
   review: the emitter writes each static part of a template as its
   own literal (`cemit.rs`, `Template`), so the limit is per part;
   the first round summed the parts, which rejects programs the ship
   tier compiles. The length is the decoded value, not the source
   text with escapes.)*
4. **The dev tier and the interpreter do not change.** Both read the
   bytes from the program image; the C text is the only output that
   moves.

### 89.2 Sites

- `compiler/src/check/expr.rs` (string and template literal
  checking): the S019 report *(both removed by §99)*;
  `compiler/src/diag.rs`: the code *(removed by §99)*;
  `compiler/src/language_reference.rs` if it lists codes;
  `specs/blocks/collisions.md`: the row (orchestrator).
- `codegen/src/cemit.rs` `c_string_literal`, and one unit test
  beside it.
- `corpus/accept/a183-long-string-literal.ts` + `.expected`.
- `codegen/tests/cemit.rs`: one test that reads the emitted C of a183.
- `generated-docs/corpus-index.md`: regenerated.

### 89.3 Corpus and gate (pre-registered exit criteria)

1. **Red at the emitter, not at run time** (this host's clang accepts
   the long literal, so no run can be red here — core principle 10
   is met by the text). A unit test in `cemit.rs` calls
   `c_string_literal` on hand-built inputs and compares against
   hand-written expectations: (a) 4,000 ASCII bytes → one piece;
   (b) 4,001 ASCII bytes → two pieces, the first 4,000 bytes and the
   second 1 byte, joined by a newline; (c) an input whose byte 4,000
   (1-based) is `0xff` and byte 4,001 is `0x01` → the first piece
   ends with `\377` and the second starts with `\001`; (d) a 20,000
   byte input → five pieces; (e) an input with `"` at a piece
   boundary → the escape `\"` whole. Each expectation is a literal
   string or a literal count. Red at `7742ab9`: (b), (c), (d), (e).
2. `corpus/accept/a183-long-string-literal.ts` + `.expected`: a
   20,000-byte literal built in the source text (`"abab…"`), printed
   by its length and its first and last eight bytes; `tsc: accepts`;
   `js-comparable: yes`. Golden on the dev JIT, the ship tier, and
   the interpreter, both profiles.
3. `codegen/tests/cemit.rs`: the emitted C of a183 contains no line
   longer than 16,380 characters, and the literal appears as five
   adjacent pieces (a hand-written count).
4. `tools/gate.sh full`: `goldens-moved 0`; every pre-existing C-text
   assertion unchanged; the windows-msvc host result is recorded in
   the tracking note when that host next runs (the downstream's W8
   program is the witness there).
5. *(Round 2, rule 3.)* `corpus/reject/r184-string-literal-too-long.ts`:
   a 65,001-byte string literal in the source text; `expected-error:
   S019 at the literal`; `tsc: accepts`. Red at `97e1110`: the
   program checks clean. A unit test in the checker with hand-written
   expectations: 65,000 bytes accepted, 65,001 rejected with the
   message text naming 65001, a template literal with one static
   part of 65,001 bytes rejected, a template with two static parts of
   40,000 bytes each (sum above the limit, each part under) accepted,
   a template whose total with substitutions would exceed the limit
   but whose static parts are short accepted, and a literal whose
   source text exceeds 65,000 bytes while its decoded value does not
   (20,000 × `\u00e9`) accepted. a183 (20,000 bytes) stays green.
   Red measured at `bd5dfec` (the tree after the round-1 landing and
   the rule-3 decision).
