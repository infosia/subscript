# The corpus size is written down five times

Status: **finding, 2026-08-28. Open.** No change is made here.

## Fact

`corpus/accept/` holds the entries. Five other files state how many
there are, and two of them enumerate the entry groups in prose. An
entry addition edits all five.

| File | What it restates |
|---|---|
| `compiler/tests/js_corpus.rs` | the accept-entry count |
| `compiler/tests/corpus_accept.rs` | the count and the entry groups |
| `compiler/tests/corpus_warn.rs` | the checked source-file count |
| `codegen/tests/golden.rs` | the golden count and the groups |
| `codegen/tests/lir.rs` | the release and debug runnable counts |

`generated-docs/corpus-index.md` also follows the corpus, and it is
generated, so it is not part of this finding.

## Evidence

`efce1ed` added `a154` and `a155` and edited five files that hold no
behaviour. `b58048f` added `a156` and `a157` and edited the same five.

The round that added `a156` and `a157` stopped twice, each time on a
count it was not permitted to touch, because the permitted file list
was written by enumeration and the enumeration was incomplete both
times. The second stop is what CLAUDE.md's two-round rule names: the
class is a defect of the form, not of the two sites.

## Why it is a defect

Core principle 8: a form carries every fact its consumers need. The
fact is the contents of `corpus/`. Five consumers restate it, and a
restatement can disagree with the directory.

Core principle 9 applies to two of the five. `js_corpus.rs` and
`corpus_accept.rs` count the entries they are about to walk and
compare that count against a literal. The count and the walk come from
one directory read, so the assertion fails only when a person edits
the corpus, never when the corpus is wrong.

The assertion still has a use: it makes an accidental deletion loud.
That use does not need five copies.

## What a fix must do

One reader of `corpus/` produces the inventory. Every consumer takes
it from there. The literal that guards against accidental deletion
lives in one place, so an entry addition edits one file.

The prose that enumerates entry groups is documentation, and it
belongs where the corpus is described, not in three test files.

This is a test-infrastructure change with no language surface. It
needs a contract line only if the owner wants the single inventory
named in `specs/blocks/corpus.md`.
