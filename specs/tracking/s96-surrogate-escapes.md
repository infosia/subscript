# §96 — a surrogate escape denotes its code point

Contract: `specs/blocks/compiler.md` §96. Origin: the §95 Phase
Review, 2026-09-09, found the defect while checking §95.3's
supplementary-character claims.

Pin for every measurement: `f5aff2c2e34ad9343c7d120f71973f75908f3586`.
Host aarch64 macOS. rustc 1.95.0. Apple clang 21.0.0. tsc 5.9.2.
node v24.18.0. Command `target/debug/subscript run <file>`.

## Red at the pin

Every program exits 0 and prints the wrong string. There is no
diagnostic, so the Red condition is a wrong value, not an exit code.

| Source spelling | subscript output, byte length | node output, UTF-16 length |
|---|---|---|
| `"👍Z"` | `👍Z`, 13 | `👍Z`, 3 |
| `"é👍X"` | `é👍X`, 15 | `é👍X`, 4 |
| `` `t👍u` `` | `t👍u`, 14 | `t👍u`, 4 |
| `"a\ud83db"` | `a\ud83db`, 8 | `a` U+FFFD `b`, 3 |
| `"\udc4d"` | `\udc4d`, 6 | one lone low surrogate, 1 |
| `"é\|あ"` | `é\|あ`, 6 | `é\|あ`, 3 |
| `"\u{1F600}"` | `😀`, 4 | `😀`, 2 |

`tsc --noEmit --strict --target ES2022` accepts every one, exit 0.
The last two rows are controls: an escape outside the surrogate range
and a brace escape above the basic plane both decode correctly, so
the defect is confined to `\uD800` through `\uDFFF`.

The byte-length differences in the last two rows are Q5's recorded
divergence. The first five rows are a different string, not a
different measure of the same string.

## Why this is not a §95 regression

The reviewer verified that the diff `4dcbd98..f5aff2c` did not
introduce it. `specs/tracking/p24-monotonic-costs.md` records the
neighbouring CESU-8 case as "unreachable from valid source". This one
is reachable from valid source, and `subscript check --deny-warnings`
reports nothing.
