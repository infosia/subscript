# The tutorials, refreshed — 2026-09-08

Status: **landed** at `a0edcd9`. Origin: the owner's request of
2026-09-08: check that `docs/` is current, and make it fuller,
readable, and honest, with `tutorial-typescript.md` explaining the
TypeScript differences in detail. Opus wrote the prose.

## What was stale

`docs/` last changed 2026-07-31. §66 to §90 landed after that date.
The measured staleness:

| Document | False or missing |
|---|---|
| `tutorial-typescript.md` | "there are no workers" (Q35 landed 2026-08); "exports take no arguments" (§59); "no index signatures on classes" (§58); the rule table stopped at S013, missing S008, S014, S016–S019; no `using`, accessors, `??`/`?.`, descriptors, literal unions, generic methods, static members, field initializers, `for...of`, or UTF-8 strings |
| `tutorial-c-cpp.md` | the same export claim at three sites; no workers; "`Date` observes only the time the host sets" (the default is the system clock, `stdlib.md` §3); a broken anchor; invented C names in the callback section |
| `tutorial-rust.md` | `W004` missing; `call_export_with` missing; "the crates are `publish = false`" (three of four); the watch loop's line count |
| `README.md` | "the two tiers are held to byte-identical output" (the interpreter is a third witness); "`Map`/`Set` in progress" (landed); no mention of workers; stale method counts; "29 lines of C" (31) |

## Method

Sizes: typescript 419 → 767 lines, c-cpp 587 → 1206, rust 146 → 617.

Every code block was run. The 13 executable TypeScript programs each
produce the output the page states, checked by a parser that pairs a
`ts` block with the `text` block after it. All 16 sample files
type-check under stock `tsc --strict` with the prelude. The C and
Rust pages' commands were run by their authors, including
`subscript build --run` hosts, a `bind` regeneration diffed against
the committed mirror, and `cargo run -p subscript-example-rust-host`.

A fresh reviewer executed 25 programs from the pages plus 18 of its
own, and checked about 130 claims against
`generated-docs/language-reference.md`,
`generated-docs/api-reference.md`, `prelude/lang.d.ts`, and 36
contract citations. It found 5 false claims and 10 imprecisions, all
fixed:

- `using` disposal order was stated forwards; it is reverse
  declaration order (the page's own example showed the reverse).
- `Date` determinism: `Date.now()` reads the system UTC clock unless
  the host pins it. Only `Math.random` is seeded by default.
- A worker transcript showed two-worker output beside a one-worker
  program.
- The interpreter does not run every corpus entry on every test run:
  124 entries in debug, 125 under the sweep variable, 56 excluded by
  header.
- `T | null` is `Ref | null`: a scalar has no null (S011, C7).

## Open

No gate reads `docs/`. Nothing catches the next drift. A candidate:
extract every `ts` block from `docs/*.md`, run it, and compare with
the `text` block that follows, as this refresh did by hand.
