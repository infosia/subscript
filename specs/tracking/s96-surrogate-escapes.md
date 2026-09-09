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

## Measurement round, 2026-09-09

A measurement round patched the fork under `$TMPDIR`, pointed this
workspace at it with a `[patch]` section, prototyped the compiler
half, and restored every repository file. `git status --porcelain` is
empty; the report is kept as `REPORT-s96-fork.md`, which holds the
fork diff.

### Where the fix belongs

The round's first report proposed the compiler side. Its own AST
measurement argued against that and changed §96.2: `Str.value` and a
template part's `cooked` are identical for `"\ud83d"` and for
`"\\ud83d"`, so a compiler-side fix must re-decide which `\u`
sequences are escapes. Only `Str.raw` and the template's `raw`
separate them.

### Measured under the fork patch, dev tier

| Source spelling | Unpatched, bytes | Patched, bytes | node |
|---|---|---|---|
| `"👍Z"` | `👍Z`, 13 | `👍Z`, 5 | `👍Z`, UTF-16 3 |
| `"é👍X"` | `é👍X`, 15 | `é👍X`, 7 | `é👍X`, 4 |
| `` `t👍u` `` | `t👍u`, 14 | `t👍u`, 6 | `t👍u`, 4 |
| `"a\ud83db"` | `a\ud83db`, 8 | rejected, exit 1 | `a` U+FFFD `b` |
| `"\udc4d"` | `\udc4d`, 6 | rejected, exit 1 | one low surrogate |
| `"é\|あ"` | `é\|あ`, 6 | `é\|あ`, 6 | `é\|あ` |
| `"😀"` | `😀`, 4 | `😀`, 4 | `😀` |
| `"\\ud83d"` | `\ud83d`, 6 | `\ud83d`, 6 | `\ud83d`, 6 |

The last three rows are the controls, unchanged. The escaped
backslash keeps its six bytes, so the patch decodes no
surrogate-looking text that the source did not escape.

The diagnostic is §96.1 rule 2's text at the escape, with the
divergence block, measured at the start, middle and end of a literal
and in a template part. Example, `"a\ud83db"` at 2:15:

```
error[S100]: a lone surrogate escape has no UTF-8 encoding; write the paired escape or the character
```

### The rule the round found

The lexer's Unicode routine also serves identifier escapes. The first
prototype decoded pairs there and accepted `const \ud801\udc00 = 1;`,
which TypeScript rejects with TS1127 at each escape.
*(Corrected 2026-09-09: this line first wrote the decoded character
instead of the escape spelling. The forms differ.)* Measured here
with tsc 5.9.2:

| Identifier | tsc | subscript |
|---|---|---|
| `const \ud801\udc00 = 1;` | TS1127 at each escape | rejected |
| `const 𐐀 = 1;` | accepted | accepted |
| `const \u{10400} = 1;` | accepted | accepted |

A literal supplementary character is a valid identifier in both.
Only the surrogate escape spelling is rejected. §96.1 rule 7
states the boundary.

### Open

The fork branch needs the owner's push, and this repository then
bumps the pinned commit in `compiler/Cargo.toml` and `Cargo.lock`.
Nothing lands before that. The fork's own test suite needs
dependencies the offline build does not have, so only the eleven
added tests ran there.

## Landed, 2026-09-09

The fork commit is `383d5c8` on branch `subscript-eof-bump`, pushed by
the owner. `Cargo.lock` names it (`8b04792`). The compiler half and the
corpus landed at `4a74ead`.

Gate verdict:

```
gate full 552a4ecbda2b88490a0cc257c6cb45fb3b33a9cc dirty:10 debug 1352/0/2 release 1350/0/2 skips 2/0 clippy 7/18/13 goldens-moved 0 exit 0
```

No golden moved. Counts: accept `.ts` 196, reject `.ts` 179.

### Verified here, not only reported

- a198 runs and matches its committed golden.
- r188 reports the contract's text at the escape column, with the
  divergence block, and no `parse error: ` prefix.
- `compiler/src/check/expr.rs` is unchanged, as §96.2 requires.
- The eleven bad-and-good pairs in `parse.rs` each assert the good
  half's decoded parts, so a pair cannot pass by failing to compile.

### What the rounds cost, and why

Three rounds stopped before implementing. Every stop was a table in
which I wrote a decoded character where the escape spelling belonged,
and every stop was correct: the two are different programs. The
contract text was right each time, because it was written through a
script with doubled backslashes rather than a shell heredoc.

The first stop also produced the finding that changed §96.2. The round
proposed fixing this compiler; its own AST measurement showed
`Str.value` cannot separate `"\ud83d"` from `"\\ud83d"`, which is
what moved the fix into the fork.
