<!-- §341 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 341. The front end parses with `oxc`

*(Added 2026-11-20.)* Origin: on 2026-11-20 the owner selected the
move from `swc_ecma_parser` to `oxc`, in one section.

Problem, measured at `3e78841f`:

1. **The front end holds a parser fork and a serde pin.**
   `compiler/Cargo.toml` pins `serde = "=1.0.219"` only because
   `swc_common` 5.0.1 imports `serde::__private`. No source file of
   this repository uses `serde`. The parser is a fork of
   `swc_ecma_parser` 6.0.2 (§90 rule 2, §96). A downstream workspace
   that depends on this compiler must pin the same `serde`.
2. **Two parser forks serve one owner.** The owner's other compiler
   parses with the `oxc` fork https://github.com/infosia/oxc at
   `43ad6ed5043f524f78c16a2d874b5fc588942a15` (`oxc_parser` 0.146.0).
   That pin builds offline on this host.
3. **The size of the move.** `swc_ecma_ast` types occur about 2,236
   times in 82 files of `compiler/src`. Three passes rewrite the AST
   after the parse (`parse/parameter_property.rs`,
   `parse/private_name.rs`, `parse/local_function.rs`). The checker
   keys maps by `Span` and `BytePos` (`check/result_dependency.rs`,
   `parse/local_function.rs`). A `swc` position is unique in the
   program; an `oxc` span starts at 0 in each file.

The `oxc` fork pin parsed each of the 1,191 `.ts` files of `corpus/`
and `prelude/`. Its diagnostics differ from `swc` in these files only:

| File | `swc` at the pin | `oxc` | `tsc` 5.9.2 |
|---|---|---|---|
| `a148`, `a201`, `a253` | no error | "'using' declaration cannot appear in the bare case statement." | accepts (gate; one probe measured) |
| `r188`, `r189`, `r190`, `r197` | lone surrogate error | no error; `lone_surrogates` is true on the literal | rejects |
| `r518` | no parse error; the checker gives S100 | "Type parameter name cannot be 'undefined'" | rejects |

A paired surrogate escape gives the one code point in a string and in
a template (measured), as §96 requires. For `"a\ud83d\x62"` and
`"\x61\ud83db"`, `oxc` gives the same span, the same value, and
`lone_surrogates = true`; the pin CLI gives S100 at columns 3 and 6.
So the AST does not carry the escape position.

`oxc` joins a pair only when both halves are `\uXXXX` with nothing
between them. For `\ud83d\u{dc4d}`, `\u{d83d}\udc4d`,
`\u{d83d}\u{dc4d}`, and a line continuation between `\ud83d` and
`\udc4d`, it gives `value` `\u{FFFD}d83d\u{FFFD}dc4d` and
`lone_surrogates = true`, in a string and in a template (measured).
`oxc_ast` documents this encoding on `StringLiteral::lone_surrogates`;
a literal U+FFFD is `\u{FFFD}fffd` (measured).

### 341.1 Rules

1. **The parser is the `oxc` fork, pinned.** The workspace declares
   `oxc_allocator`, `oxc_ast`, `oxc_ast_visit`, `oxc_parser`
   (`default-features = false`), and `oxc_span` from
   https://github.com/infosia/oxc at `rev =
   "43ad6ed5043f524f78c16a2d874b5fc588942a15"`. `swc_common`,
   `swc_ecma_ast`, `swc_ecma_parser`, and the `serde` pin go.
   `icu_normalizer` in `runtime/Cargo.toml` moves from `=2.2.0` to
   `=2.3.0` (rule 1a). No other dependency changes.
1b. `oxc_semantic` from the same pin joins the five crates (rule 4c).
1a. **The ICU pin follows the parser.** `oxc_diagnostics` depends on
   `textwrap` 0.16.4, whose default features need `icu_segmenter`
   2.3.0 and so `icu_collections` 2.3. `icu_normalizer` 2.2.0 needs
   `icu_collections` 2.2, and one workspace resolves one 2.x version.
   Measured: for every Unicode scalar value, NFC and NFD of 2.3.0
   equal those of 2.2.0, and NFC of each scalar followed by each of
   eight combining marks (U+0300, U+0301, U+0308, U+0323, U+3099,
   U+309A, U+093C, U+11127) is equal too (26,252 changed forms, the
   same in both). `ParseOptions::max_nesting_depth` stays `None`
   (§114).
2. **A source parses as a TypeScript module.** Each source, `.d.ts`
   included, parses with `SourceType::ts().with_module(true)`. The
   ambient checks of a `.d.ts` source do not change.
3. **A position is unique in the program.** After the parse, each span
   of file `i` moves by a base `b_i`. `b_0` is 1, and `b_(i+1)` is
   `b_i` plus the byte length of file `i` plus 1. So span 0 stays the
   synthesized span, and a span names one file. A position maps to a
   file, a 1-based line, and a 1-based column that counts UTF-16 code
   units, and a line ends at LF, CR, CRLF, U+2028, or U+2029, as
   `tsc` does (a character above U+FFFF counts 2). The pin agrees with
   `tsc` except for U+2028 in a string and a lone CR in a comment. A lookup does not scan the text: the front end builds
   the line and character tables of each file once.
4. **The `tsc` rule decides a parser difference.** If `oxc` reports an
   error for a form that `tsc` accepts, the front end drops that one
   diagnostic and keeps the AST. The bare case `using` diagnostic of
   the table is one such error. If `oxc` accepts a form that `tsc`
   rejects, the checker rejects it.
4c. **The early errors stay.** `oxc_parser` leaves the early errors of
   ECMAScript and TypeScript to `oxc_semantic`. The front end runs the
   `oxc_semantic` syntax check (`SemanticBuilder` with
   `with_check_syntax_error(true)`) on each file after the parse and
   before the rewrite passes. If the checker reports no error for the
   program, each error of that check is an S100 parse error, under the
   filter of rule 4. If the checker reports an error, the program gives
   the checker's diagnostics only, so a form that the checker already
   rejects keeps its diagnostic. Measured with the pin CLI, `tsc` 5.9.2, and
   the CLI without this rule:

   | Form | Pin | Without rule 4c | `tsc` |
   |---|---|---|---|
   | `const m: i32 = 0755;` | S100 | accepted, value 755 as `i32`, 493 as `f64` | TS1121 |
   | `09` | S100 | accepted | TS1489 |
   | two constructors in one class | S100 | accepted | TS2392 |
   | `const package: i32 = 1;` (each strict reserved word) | S100 | accepted | TS1214 |
   | `const eval: i32 = 1;`, `let arguments` | S100 | accepted | TS1215, TS1100 |
   | `"\01"`, `"\101"` | S100 | accepted | TS1487 |
   | `"use strict"` in a function with a default parameter | S100 | accepted | TS1347 |

   The check gives an error for each form of the table (measured with
   a probe of the pin `43ad6ed`). Over the 1,191 `.ts` files of
   `corpus/` and `prelude/`, it gives errors in 9 files only, each a
   reject entry that the checker rejects (`r146`, `r149`–`r151`,
   `r161`–`r164`, `r452`: duplicate names, a private name outside its
   class).
4a. **A surrogate pair joins in the value.** If a string literal or a
   template part has `lone_surrogates`, its `value` (`cooked`) uses
   the `oxc` encoding: `\u{FFFD}` and four hex digits is one code
   unit, and `\u{FFFD}fffd` is U+FFFD. The front end decodes that
   encoding once. A high surrogate code unit that a low surrogate code
   unit follows becomes the one code point of the pair (§96 rule 1).
   This covers each spelling of §96.1: `\uXXXX`, `\u{XXXX}`, mixed,
   and a line continuation between the two escapes. The front end
   reads `value`, never `raw`.
4b. A code unit that stays alone after rule 4a keeps the
   `ParserLoneSurrogateEscape` diagnostic. Its position is the start
   of the literal (a template: the start of the template part), not
   the escape: the `oxc` AST does not carry the escape position, and a
   second escape scanner over `raw` is the class that §96.2 forbids.
   The line stays the line of the escape in each corpus entry.
5. **The AST passes move, not change.** The three rewrite passes and
   the checker read and build `oxc_ast` nodes. Each rewrite builds its
   nodes in the parse arena. Each fact that a pass or the checker
   derives stays the same.
6. **The output stays the same.** For each corpus entry, the `check`
   output, the HIR, the LIR, and the run output of both tiers equal
   those at the pin. These differ only: the text after "parse error:"
   (the `oxc` message), the `r518` diagnostic, which `oxc` gives at
   the same line, the column of a lone surrogate diagnostic
   (rule 4), the forms of rule 6a, and the position of a parse error
   (the `oxc` parser decides it; for example `/a/gg` moves from 1:42
   to 1:46). No corpus entry other than those of rule 4 moves.
6a. **A parse error replaces a checker diagnostic of a form that `tsc`
   rejects.** If `tsc` rejects a form and `oxc` gives a parse error
   for it, the S100 parse error is the diagnostic. If no input then
   reaches a rejection site, the site and its witness go. The report
   lists each such site with its `oxc` message. Example: `class A {
   [a:i32]; }` gives "An index signature must have a type annotation."
   (`tsc` TS1021), and `IndexSignatureElementAnnotationMissing` has no
   input left: `oxc` stops the parse before it builds the class
   (measured).
6b. **The stack margin stays.** At depth 200, the `nesting` tests
   (`compiler/tests/nesting.rs`, §113, §114.2 rule 3) pass on the
   libtest stack of 2,097,152 bytes in the debug and release builds,
   with a margin of at least 2 in each. The test comment states the
   new measured need. Measured: the debug run of the full gate on this
   change overflowed the stack in
   `a_two_hundred_deep_not_chain_checks_in_linear_time`.
7. **The front end does not panic.** §90 rule 1 holds with `oxc`. The
   robustness test (§90 rule 3) runs as it does at the pin.
8. **Nothing else changes.**

### 341.2 Acceptance

1. `cargo tree` lists no `swc_` crate and no pinned `serde`.
2. The `check` output of each corpus entry (accept, reject, warn,
   trap, interop) at the pin and after: the report gives the count and
   each difference. Each difference is one of rule 6.
3. `a148`, `a201`, and `a253` pass in both tiers. `r188`, `r189`,
   `r190`, and `r197` give the lone surrogate diagnostic on the pin
   line.
4. A test of rule 3 parses two files whose spans have the same local
   offsets. A map that the checker keys by span holds one entry for
   each file.
5. A test of rule 4 drops the bare case `using` diagnostic and keeps a
   control: another `oxc` parse error in the same file still gives
   S100.
6. The full gate passes. The LIR goldens do not move.
7. The report gives the time of `check` over every corpus entry at the
   pin and after (release, one run each).

### 341.3 Sections this one amends

- §90 rule 2: the parser fork is the `oxc` fork of rule 1. The
  `swc_ecma_parser` fork is no longer a dependency. §90 rules 1, 3,
  and 4 stay.
- §6: the front end is `oxc` (rule 1), not `swc`.
- §193 rule 5: `icu_normalizer` is 2.3.0 (rule 1a). The NFC result is
  the one that 2.2.0 gives (measured). `unicode-segmentation` stays
  1.13.3.
- §60 measurement 1 and §96: the `swc` text is a record of the pin.
  §96 rule 1 holds with `oxc` through rule 4a. The lone surrogate
  diagnostic of §96.2 item 2 is at the literal start (rule 4b).
- `specs/blocks/stdlib.md` §15.3: the regular expression literal comes
  from the `oxc` parser.
