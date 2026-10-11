<!-- §193 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 193. A text module counts graphemes and normalizes to NFC

*(Added 2026-10-11.)* Origin: a string review on 2026-10-11 that the
owner supplied, and the measurement round at `3cc0aae2`
(`specs/tracking/text-module-measure.md`). On 2026-10-11 the owner
selected the form of rules 1–4: the import is the opt-in, with no
`--enable-module`, because the module needs nothing from the host.

Problem: strings measure in UTF-8 bytes (Q5, Q21). A program that
limits, cuts, or deletes user-visible characters needs extended
grapheme clusters (UAX #29). `"か\u3099"` is 2 code points and 1
cluster. `"\u{1F469}\u200D\u{1F469}\u200D\u{1F467}\u200D\u{1F466}"` is
7 code points and 1 cluster. Canonical equivalents (`"\u304C"` and
`"か\u3099"`) compare unequal until a program normalizes them (UAX
#15). `normalize` is rejected today (`stdlib.md` §8). The language has
no form for either operation.

### 193.1 Rules

1. **The module.** The specifier is `subscript:text`. A program imports
   it with no build option. §185 needs `--enable-module` because the
   host does the I/O; this module is computation in the runtime, so the
   import is the only opt-in. It is a standard module that is always
   enabled.
2. **Graphemes.** `import { graphemeLength, sliceGraphemes } from
   "subscript:text"`:
   - `graphemeLength(s: string): i32` gives the number of extended
     grapheme clusters. It allocates nothing.
   - `sliceGraphemes(s: string, start: i32, end?: i32): string` counts
     positions in clusters, with the clamping and negative-position
     rules of `slice`. A reversed range gives `""`. The result is the
     original bytes of the range, with no normalization. It allocates
     only the result.
3. **NFC.** `s.normalize()` and `s.normalize("NFC")` give the NFC form
   of `s`. Text that is already NFC gives the receiver with no
   allocation. Any other argument is a compile error: the form must be
   omitted or the literal `"NFC"`. The method needs no import, as in
   TypeScript. No operation normalizes implicitly: equality, `Map` and
   `Set` keys, search, and slicing use the exact bytes.
4. **Where the code lives.** The runtime always contains the text
   code. A ship link removes it from a program that does not call it
   (§192: measured 16 bytes). A program carries the grapheme tables
   (measured +33 KB) only if it calls a grapheme function, and the NFC
   tables (+83 KB) only if it calls `normalize`. The call in the source
   shows the cost, so core principle 16 does not apply. The dev tier
   links all of it (measured +83 KB for the CLI).
5. **Unicode version.** Unicode 17.0, through `unicode-segmentation`
   1.13.3 and `icu_normalizer` 2.2.0, pinned in `Cargo.lock`. A version
   change is a contract change, because it can move goldens.
6. **Every tier.** The dev tier, the ship tier, and the interpreter
   call the same runtime functions and give the same output.
7. **Not in this section.** NFD, NFKC, NFKD, a grapheme iterator,
   grapheme padding, and display width.

### 193.2 Acceptance

1. Red at the pin: accept entries for each form of rules 2 and 3,
   over ASCII, Japanese BMP text, supplementary kanji, combining dakuten, a ZWJ emoji
   sequence, and a regional-indicator pair. Reject entries:
   `normalize("NFD")`, `normalize` with a non-literal argument, and a
   name that `subscript:text` does not export.
2. `tsc` accepts every accept entry with the prelude declaration.
   `normalize` entries are `js-comparable`. Grapheme entries cite the
   new collision C26: `node` cannot load `subscript:text`. The tracking
   note records `Intl.Segmenter` results for the same inputs as a
   hand check.
3. Unit tests of the runtime functions, including the UAX #29 test
   data of the pinned version and the UAX #15 NFC column, with a
   stated cost.
4. Cost: the size of a shipped program without the module (no change
   against the pin), with graphemes, and with NFC; the CLI size; the
   per-call times of the measurement note on the final code.
5. The TypeScript tutorial shows the module with a Japanese example.
