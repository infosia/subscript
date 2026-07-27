# RegExp linked-size gate

`baseline.ts` and `with-regex.ts` are a matched pair. The latter differs
only by one assignment that calls `RegExp`; both programs otherwise reach
the same string comparison, formatting, print, Context, and host-entry
paths.

Run from the repository root:

```sh
cargo run --offline --release -p subscript-benchmarks --bin regex-size-gate
```

The gate is intentionally active only on macOS arm64. It emits ship C,
compiles and links both programs with `-O2 -Wl,-dead_strip`, strips both
executables, checks the linked-size delta, and independently sums live
`regress` symbols from the regex-side link map. Other targets report a
skip because their object format, linker map, and strip behavior are not
the measured contract.
