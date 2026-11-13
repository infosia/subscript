# Duplicate module stems

Contract: compiler.md §267 rules 1–5.
Contract pin: `08d0494c`.

`check_program_with` now checks non-ambient file stems before it parses any source.
It uses `parse::stem_of`, returns one diagnostic, and stops at the second file.
`SourceFile::name` states the unique-stem rule.

The diagnostic uses S100, the code of the existing program input guards.
The registry uses `ProgramInput` because this error concerns host input, not a TypeScript subset rejection.
The empty-source and unknown-enabled-module guards also use `ProgramInput`, per rule 5.
No divergence entry, corpus entry, or golden changes.

Exact diagnostic at `two/foo.ts:1:1`:

```text
duplicate module stem `foo`: `one/foo.ts` and `two/foo.ts`
```

## Evidence

The Red test used `one/foo.ts`, `two/foo.ts`, and an entry that imports `./foo`.
Each `foo.ts` initializer prints its directory and exports a function named `value`.
At the contract pin, the check succeeded and the rejection assertion failed.
The HIR call resolved to the function in `one/foo.ts`, which returns 1.
The HIR initializer order was `two/foo.ts`, `main.ts`, `one/foo.ts`.
The corresponding print order was `two`, `1`, `one`, as read from the HIR statements.
Red result: two control tests passed; the rejection test failed. Wall time: 0.76 s.

The distinct-stem control uses `one/foo.ts`, `two/bar.ts`, and an empty exported `main` function.
The repository control checks the program and reads its HIR initializer order.
The round 1 interpreter run produced `one\n1\ntwo\n`. The output assertion passed. Wall time: 7.01 s.
The round 1 interpreter run used a temporary external harness that is not in the repository.

Two empty ambient files, `one/foo.d.ts` and `two/foo.d.ts`, remain accepted with an ordinary entry.
This control passed before and after the change.
An invalid-source test also checks that the duplicate-stem diagnostic precedes parser errors.

## Checks

- New test: PASS, 4 tests, 0.48 s.
- Compiler library and registry total checks: PASS, 611 tests, 9.18 s.
- `cargo fmt --check`: PASS, 2.07 s.
- `cargo clippy -p subscript-compiler --all-targets`: PASS, 0.27 s on the final run.
- The initial Clippy run took 26.07 s. Its warnings concern only unchanged files.
- `cargo test -p subscript-compiler`: PASS, 1527 tests, 149.09 s.

## Round 2 checks

- `cargo test -p subscript-compiler --lib`: PASS, 611 tests, 4.82 s. This includes the registry total checks.
- `cargo test -p subscript-compiler --test duplicate_module_stem`: PASS, 4 tests, 2.58 s.
- `cargo fmt --check`: PASS, 2.08 s.
- `cargo clippy -p subscript-compiler --all-targets`: PASS, 4.28 s. Warnings concern only unchanged files.
