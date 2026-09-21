# §114 — the compile thread is removed

Contract: `specs/blocks/compiler/s114-the-compile-thread-is-removed.md`.
Status: **contract landed 2026-09-21**; the implementation round is
open.

Origin: the x86_64-unknown-linux-gnu full gate at `c72da05`. That run
is in `specs/tracking/linux-portability.md`, under the 2026-09-21 full
gate.

## The derivation lost its input

`compiler-history.md` §109.2a states the derivation:

> A file of `SOURCE_BYTE_LIMIT` bytes can therefore need
> `SOURCE_BYTE_LIMIT × 6,750` bytes of stack

| Build | Bytes per level | Stack | Worst file (131,072 bytes) | Margin |
|---|---|---|---|---|
| optimized | 6,750 | 2,147,483,648 | 884,736,000 | 2.43 |
| unoptimized | 31,151 | 8,589,934,592 | 4,083,023,872 | 2.10 |

The worst file of 131,072 bytes is the S026 byte limit, a rule of the
sandbox profile. §113.1 rule 2 retired the byte limits. Measured in
the tree at `c72da05`: `SOURCE_BYTE_LIMIT` is absent, the margin test
is absent, and the two constants have no test that pins them. The only
readers of `COMPILE_THREAD_STACK_BYTES` are its own declaration and
the `stack_size` call.

## A trusted script does not reach that depth

Invariant 6 states that scripts are trusted.

The deepest source of `corpus/`, `examples/`, and `prelude/` nests 8
brackets, over 545 files (`examples/e12-one-shot-requests.ts`). The
scan counts `(`, `[`, and `{` over the whole byte stream, so it counts
a string and a comment too. The number is an upper bound.

`tsc` 5.9.2 on `node` 24.21.0, with the default stack, `--strict`
`--target ES2022`:

| Construct | Depth | `tsc` |
|---|---|---|
| parentheses | 500 | exit 0 |
| parentheses | 1,000 | `RangeError: Maximum call stack size exceeded` |
| type arguments | 500 | exit 0 |
| type arguments | 1,000 | `RangeError` |
| `? :` chain | 2,000 | exit 0 |
| `? :` chain | 3,000 | `RangeError` |

Invariant 5 admits no program that `tsc` refuses, so no accepted
program nests 1,000 parentheses. The unoptimized stack holds about
275,000 levels of that shape, at the 31,151 bytes per level of M11.

## The two defects the stack caused

1. **§110.** Its problem paragraph names the cause: "§109.2a (history)
   then gave the compile thread a stack reservation of 8,589,934,592
   bytes unoptimized. The lowering runs on that thread, the two
   mappings fell on the two sides of the reservation, and the JIT
   failed to connect them."
2. **The Linux full gate at `c72da05`.** 12 debug tests report `fork
   JIT runner: Cannot allocate memory (os error 12)`. The host
   refuses `fork` over 37.93 GiB of mapped private address space, and
   five live compiles pass that sum. The peak `VmSize` of the
   `interop` test binary is 136,381,092 kB unoptimized and 33,609,276
   kB optimized.

## §90 is not the reason

§90 is an owner decision of 2026-09-06, from the question "does any
API panic". The compile thread landed on 2026-09-17, in `ce9ddbf`,
whose message groups it with the nesting guard, S027, the work and
output budgets, and the token and program limits (§109.2, §109.2a).
`51ef009`, the same day, raised the unoptimized constant from
4,294,967,296 to 8,589,934,592.

§90's measurement is 15,898 runs: 0 panics and 1 fault. The fault is
`parse_ts_enum_member` of `swc_ecma_parser`, and the pinned fork
answers it. No run of that measurement overflowed a stack.

## §85 rule 4a has no producer

The only `gate-debug-only:` producers in the tree are the stub suites
of `cli/tests/gate.rs`, which test the script. The producer that the
rule was written for was the heavy set of §109.6a, and
`SUBSCRIPT_HEAVY_TESTS` went with §113.1 rule 10. The Linux release
step at `c72da05` counts 0.

## The sites the round must move

`on_the_compile_thread` has 27 call sites in 16 files: `compiler/src`
(`lib.rs`, `parse.rs`), `codegen/src` (`jit/compile.rs`,
`emit_files.rs`, `reload.rs`, `ship.rs`), `cli/src` (`lib.rs`,
`watch.rs`), and eight test files.

## Open

- The implementation round. The exit criteria are §114.5. The Linux
  full gate at the landing is the measurement that passes or kills it.
