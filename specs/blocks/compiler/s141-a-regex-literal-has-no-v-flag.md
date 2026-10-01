<!-- §141 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 141. A regex literal has no `v` flag

*(Added 2026-10-01.)* Origin: an open defect recorded during §133.

Problem: the checker accepts a regex literal with the `v`
(`unicodeSets`) flag, and `tsc` rejects it under the project's
configuration, which breaks CLAUDE.md invariant 5. Measured at
`fa6974b5`:

- `const r: RegExp = /a/v;` runs and prints `true`.
- `tsc` 5.9.2 with the project's target (`ES2022`) gives TS1501 ("This
  regular expression flag is only available when targeting 'es2024' or
  later").
- `new RegExp("a", "v")` runs, prints `true v`, and `tsc` accepts it:
  the flags are a string, which `tsc` does not check.

`stdlib.md` §15 supports the `v` flag, and the project's lib does not
add `unicodeSets` (ES2024). Only the literal spelling reaches TS1501.

### 141.1 Rules

1. A regex literal whose flags hold `v` is S100 at the literal. The
   message says that the `v` flag needs ES2024 in a literal and names
   the constructor spelling `new RegExp(pattern, "v")`.
2. `new RegExp(pattern, flags)` with `v` does not change.

### 141.2 Acceptance

1. Red first: a reject entry with `/a/v`, accepted at the contract pin;
   the header states `tsc: rejects TS1501` (no divergence block: both
   reject). The round records the Red output.
2. Unit tests: `/a/v` and `/a/gv` are S100 with the full message,
   exactly one diagnostic; controls: `/a/u` and `new RegExp("a", "v")`
   are accepted and run.
3. No existing `.expected` golden moves.
