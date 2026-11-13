# Header include directory

Contract: compiler.md §265 rules 1–4. Pin: `bdda8595`.

The CLI passes the source path through `BindOptions::with_source_path`.
The frontend uses that path as the unsaved main file name, including both preamble retry paths.
The include spelling remains the basename. Text-only input retains `header.h` in the current directory.

## Evidence

The CLI test creates sibling `api.h` and `types.h` files.
The API uses a sibling typedef with an external directive, as the boundary type contract requires.
The test compares relative and absolute paths from another directory against the header-directory output, byte for byte.
It also checks that a sibling function does not enter the mirror.
The missing-file control checks the libclang diagnostic and exit code 1.
The CLI test requires libclang and does not skip.

Before the implementation, the test failed at the pin with this text:

```text
subscript: bindgen: libclang: fatal error: 'types.h' file not found
```

Red: 1.12 s wall time; test body 0.61 s.

## Results and cost

- `cargo test -p subscript-cli --test bind_header_directory`: PASS, 1.82 s wall time; test body 0.81 s.
- `cargo test -p subscript-bindgen`: PASS, 170 tests, 4.55 s wall time.
- `cargo fmt --check`: PASS, 2.00 s wall time.
- `cargo clippy -p subscript-bindgen -p subscript-cli --all-targets`: PASS, 0.77 s wall time; no warnings in changed files.

Clippy reports existing warnings in other files.
The changed frontend replaces `map().flatten()` with `filter_map()` to remove its existing warning.

The new test costs four CLI processes and four libclang parses when libclang is present.
The three successful parses establish directory independence; the fourth checks the missing-file diagnostic.
The test starts no C compiler and no tsc process.
The implementation adds no filesystem read or process; libclang uses the supplied source path for include resolution.


## Acceptance checks

The direct API unit test calls `generate_with_options` with and without `with_source_path`.
A unique sibling filename prevents a match in the current directory; the test does not change that directory.
The source-path call succeeds and excludes sibling declarations. The default call reports the missing include.
The frontend comment states the include-directory role and the `isFromMainFile` declaration filter.

- `cargo test -p subscript-bindgen source_path_resolves_sibling_include`: PASS, 4.05 s wall time; test body 0.03 s.
- `cargo test -p subscript-cli --test bind_header_directory`: PASS, 2.09 s wall time; test body 0.81 s.
- `cargo test -p subscript-bindgen`: PASS, 171 tests, 0.55 s wall time.
- `cargo fmt --check`: PASS, 2.00 s wall time.
- `cargo clippy -p subscript-bindgen -p subscript-cli --all-targets`: PASS, 1.05 s wall time; no warnings in changed files.

Clippy reports existing warnings in other files.
The API unit test adds two libclang parses and one temporary sibling file; it starts no subprocess.
