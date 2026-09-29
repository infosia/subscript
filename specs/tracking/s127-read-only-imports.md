# Read-only imports (§127)

## Change

The scope entry carries the declaration and an import flag.
Import registration sets the flag. Declaration registration clears it.
The common assignment-target check rejects imported identifiers with S100.
The message and position identify the local source name.
Local bindings take precedence. Poisoned imports retain diagnostic suppression.
Member writes use the existing member-target checks.
`r269-import-assignment/main.ts` is in the reject table with S100 at
line 10, as its expected-error header specifies.

All edited Rust files remain below 2,000 lines. No split was required.
The largest edited file, `compiler/src/check/mod.rs`, has 1,732 lines.
No existing `.expected` file changes.

## Red

Pin: `5920322b0b231fb0e1ae47a325817b460f1133c4`. `d735bd1..5920322`
changes only `specs/`, so the compiler is the one at `d735bd1`
(acceptance 1).
Before implementation, `cargo build --offline --locked -p subscript-cli` succeeded.
That binary ran `corpus/reject/r269-import-assignment/main.ts` with exit 0 and stdout:

```text
11
```

The importer assigned 10 to `count`. The exporter then added 1.

## TypeScript measurement

TypeScript 5.9.2 ran with `prelude/lang.d.ts`, strict checking, ES2022,
ESNext modules, Bundler resolution, and the ES2022 and ESNext.Disposable libraries.
The command used `--noEmit` and exited 2:

```text
corpus/reject/r269-import-assignment/main.ts(10,5): error TS2632: Cannot assign to 'count' because it is an import.
```

## Tests

`compiler/tests/read_only_imports.rs`: 17 tests pass in 0.02 seconds.
Sixteen tests cover plain assignment, all eleven supported compound operators
(`**=`, `&&=`, `||=`, and `??=` are outside the surface and are S100
for every target before this check),
and prefix/postfix increment/decrement. Each checks renamed and unrenamed imports.
Each checks S100, the local name, and the target position.
Each includes accepted exporter writes, imported object fields, imported static fields,
and local shadow bindings. The controls also read the imported global after exporter calls.
One test checks imported constants, functions, and classes, with
accepted import controls. Two tests, added after the Phase Review,
check an imported enum name and a write to a poisoned import (no
second diagnostic); 19 tests pass.

Destructuring assignment is outside the surface (§107.3).
The existing pattern rejection remains unchanged.

`compiler/tests/renamed_imports.rs`: all three tests pass.
The API-reference generator regenerated the documentation; only the corpus index changed.

## Gate

The first full gate failed on the reject-table completeness test in
debug and release: r269 had no table row. After the row, one re-run:

```text
gate full 5920322b0b231fb0e1ae47a325817b460f1133c4 dirty:10 debug 1904/0/3 release 1901/0/3 skips 2/0 clippy 3/18/13 goldens-moved 0 exit 0
```

## Phase Review

No CRITICAL, no MAJOR. MINOR: this note (fixed) and two missing test
cases (added). Open, outside §127: a generic function with no
instantiation is not checked, so `function h<T>(x: T): T { count = 2;
return x; }` with no caller is accepted (`tsc` TS2632), and the same
for a `const` local (`tsc` TS2588).
