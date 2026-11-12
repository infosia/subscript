# Reload layout fingerprint

Contract: compiler section 261.

`Layouts::build` and the reload declaration hash read `class_shape` in
`codegen/src/layout.rs`. The hash uses the full record through derived
`Debug` text. A new record field thus enters the hash without a second edit.
The hash keeps the source file, class name, field names, and field order.

## Red measurement

At contract commit `f0ed99a1`, the alignment test failed before the code change.
Both module hashes were `7358494539189546389`.
Both `class V` entry hashes were `8460381249873183531`.
The only source edit changed `align: 4` to `align: 16`.

Command: `cargo test -p subscript-codegen --lib declaration_hash_changes_for_alignment`.
Result: 0 passed, 1 failed; wall time 10.71 s; test time 0.01 s.

## Boundary measurement

The checker sets `is_boundary` in `collect_boundary_struct` for mirror classes.
An ordinary value class and an ambient class can have the same remaining layout inputs.
The test checks equal value kinds, absent alignment overrides, and equal field types.
The source file and class name also stay equal.
A mirror class comes from an ambient declaration, so a source edit also changes the declaration file.

At pin `f0ed99a1`, the class entry was equal for the two boundary kinds.
This result follows from the old text: both kinds wrote `value`, with equal file, name, and fields.
This check reads the pin code; it does not execute that code.
The class entry hashes differ after the change.
The test compares class entries because ambient input also changes the module run order.

## Checks

| Command | Result | Wall time | Test time |
| --- | --- | --- | --- |
| `cargo test -p subscript-codegen --lib reload` | 33 passed | 2.43 s | 0.11 s |
| `cargo test -p subscript-codegen --lib layout` | 23 passed | 0.21 s | 0.10 s |
| `cargo fmt --check` | Pass | 2.05 s | — |
| `cargo clippy -p subscript-codegen` | Pass; no warnings in changed files | 1.93 s | — |

The reload tests include the alignment, boundary kind, and body controls.
The alignment test checks that `first_difference` returns `class V`.
The body control covers free functions, constructors, and methods.

Repository searches found no golden or snapshot that stores a declaration hash.
The searches covered declaration hash references, corpus expected files, and snapshot file names.
No corpus, golden, or snapshot file changed.

## Round 2 checks

| Command | Result | Wall time |
| --- | --- | --- |
| `cargo test -p subscript-codegen --lib reload` | test result: ok. 33 passed; 0 failed; 0 ignored; 0 measured; 306 filtered out; finished in 0.11s | 4.39 s |
| `cargo test -p subscript-codegen --lib layout` | test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 316 filtered out; finished in 0.12s | 0.22 s |
| `cargo test -p subscript-cli --test downstream_script (run 1)` | test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.52s | 5.41 s |
| `cargo test -p subscript-cli --test downstream_script (run 2)` | test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.35s | 0.47 s |
| `cargo test -p subscript-cli --test downstream_script (run 3)` | test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.34s | 0.48 s |
| `cargo test -p subscript-cli --test downstream_script (run 4)` | test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.35s | 0.46 s |
| `cargo test -p subscript-cli --test downstream_script (run 5)` | test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.39s | 0.50 s |
| `cargo fmt --check` | Pass | 2.39 s |
