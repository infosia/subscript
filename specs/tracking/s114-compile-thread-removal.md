# §114 — the compile thread is removed

Contract: `specs/blocks/compiler/s114-the-compile-thread-is-removed.md`.
Status: **landed 2026-09-21** (contract `0f8221a`, implementation
`e242ec9`). The Windows and arm64 macOS gates and the Phase Review are
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
the tree at `c72da05`: `SOURCE_BYTE_LIMIT` is absent. The margin test
is absent. No test pins the two constants. The only readers of
`COMPILE_THREAD_STACK_BYTES` are its own declaration and the
`stack_size` call.

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
API panic". The compile thread landed on 2026-09-17, in `ce9ddbf`.
That commit message groups it with the nesting guard, S027, the two
budgets, and the token and program limits (§109.2, §109.2a).
`51ef009`, the same day, raised the unoptimized constant from
4,294,967,296 to 8,589,934,592.

§90's measurement is 15,898 runs: 0 panics and 1 fault. The fault is
`parse_ts_enum_member` of `swc_ecma_parser`, and the pinned fork
answers it. No run of that measurement overflowed a stack.

The defect is not §90's words. §90 is older than the profile, and its
words were right when it was written. §113.2 rule 1 cited it — "A
stack overflow aborts the process, which §90 forbids" — and the
unqualified "any byte sequence" carried an adversarial reading. §90.1
therefore carries a premise before its rules, and the rules do not
change. An exception for each hostile shape does not converge
(CLAUDE.md, two review rounds).

## §85 rule 4a has no producer

The only `gate-debug-only:` producers in the tree are the stub suites
of `cli/tests/gate.rs`, which test the script. The producer that the
rule was written for was the heavy set of §109.6a, and
`SUBSCRIPT_HEAVY_TESTS` went with §113.1 rule 10. The Linux release
step at `c72da05` counts 0.

## The sites the round must move

`on_the_compile_thread` has 27 call sites in 17 files: `compiler/src`
(`lib.rs`, `parse.rs`), `codegen/src` (`jit/compile.rs`,
`emit_files.rs`, `reload.rs`, `ship.rs`), `cli/src` (`lib.rs`,
`watch.rs`), and nine test files. *(Corrected 2026-09-21: the first
form of this paragraph said 16 files and eight test files. The round's
report gave the count.)*

## The landing

The implementation is `e242ec9`. 20 files moved: the 17 above,
`tools/gate.sh` and `cli/tests/gate.rs` for §85 rule 4a, and
`codegen/src/jit/memory.rs`. That file holds a §110 test doc comment,
which named the compile thread's stack as the separator of one
module's two mappings. No golden moved.

Outside `specs/`, `git grep` answers nothing for
`on_the_compile_thread`, `COMPILE_THREAD_STACK_BYTES`,
`gate-debug-only`, `113.2 rule 1`, and `compile thread`.

Two calls held the wrapper alone, so each one goes:
`check_on_this_thread` is inlined into `check_program_with`, and
`watch_load` into its two callers.

### The Linux full gate

The x86_64-unknown-linux-gnu host ran the full gate at `e242ec9`, with
a clean tree.

```text
gate full e242ec9b96fc2bc8c712c825c15bbeea292ba131 clean debug 1539/0/2 release 1536/0/2 skips 2/0 clippy 7/18/13 goldens-moved 0 exit 0
```

Record: `target/gate/20260921T102159Z-full.md`. The verdict line holds
no `debug-only` field. No test reports `fork JIT runner`. The two debug
skips are the `perf_gate` line and the `a22-matrix-propagation` line,
as before.

Step wall seconds, at the pin and at the landing: fmt 1 / 1, build
0 / 0, debug 154 / 122, release 283 / 242, clippy 15 / 0, tsc 0 / 0,
hygiene 0 / 0. The build and clippy steps read a warm `target/` in
both runs, so those two numbers measure the cache.

Test counts fall by 6 in each profile: 1,545 to 1,539 in debug and
1,542 to 1,536 in release. The four capacity tests of
`compiler/tests/nesting.rs` and the two gate cases of
`cli/tests/gate.rs` account for all six. The ignored count stays 2.

### The address space

Peak `VmPeak` of the `subscript-codegen` `interop` test binary, at the
default 16-thread parallelism:

| Profile | At the pin (kB) | At the landing (kB) | Factor |
|---|---|---|---|
| unoptimized | 136,381,092 | 1,179,976 | 115.6 |
| optimized | 33,609,276 | 1,103,012 | 30.5 |

The round measured 1,245,464 kB unoptimized on its own run. The
unoptimized figure moves between runs; both are under 1.2 GiB, and the
fork threshold of that host is 37.93 GiB.

### The Red state

The gate before the change reported 3 failures, not the 12 of
`c72da05`: `array_empty_shift_reports_identically_across_tiers`,
`trap_corpus_entries_match_dev_stdout_on_both_tiers`, and
`async_start_array_growth_matches_all_three_witnesses`. Each one is
`fork JIT runner: Cannot allocate memory (os error 12)`. The failure
needs five live compiles at one time, so 3 and 12 are two samples of
one defect.

## Phase Review (2026-09-21)

A fresh no-context reviewer read `c72da05..e649be3`. No CRITICAL. Two
MAJOR, six MINOR.

**MAJOR 1.** §85 rule 4a carried a second paragraph of its own body
under the deletion marker, so the section contradicted §114.3. The
paragraph is deleted.

**MAJOR 2.** The two kept tests of `compiler/tests/nesting.rs` now run
on the thread libtest gives them, which holds 2,097,152 bytes. No
document stated that cost (core principle 15). Measured on the x86-64
Linux host, for one `check_program` and one `check_warnings` at depth
200:

| Build | Stack the pair needs | Margin |
|---|---|---|
| unoptimized | over 917,504 and under 950,272 bytes | 2.2 |
| optimized | over 262,144 and under 524,288 bytes | 4.0 |

Method: `RUST_MIN_STACK=<n> target/<profile>/deps/nesting-* --test-threads=1`.
At 917,504 unoptimized the binary prints `thread … has overflowed its
stack` and aborts; at 950,272 it passes. §114.2 rule 3 carries the
table.

A stack overflow ends the test binary, so the gate reads the loss as a
count that fell, not as one named failure. §114.5 criterion 5 measures
the margin on each other gate host.

**MINOR, fixed:** the direction word of §90.1's premise; the index
line, which called two deletions an amendment and left rule 2 out; the
landing `tsc` second, which read 1 against the 0 of the record it
cites; the `Open` item of `specs/tracking/s113-sandbox-removal.md` and
the closing line of the 2026-09-21 section of
`specs/tracking/linux-portability.md`, which both left the question
open that this arc answered; four sentences over the 25-word limit and
two words outside the approved set.

**MINOR 6, closed by measurement.** The §110 test
(`two_items_of_one_module_are_inside_that_modules_reservation`) builds
the conforming form only, and its doc comment claimed that nothing
bounds the distance between one module's two mappings. The 8 GiB stack
was the measured separator, and §114 removed it, so core principle 9
asks whether the check can still fail.

A reverted prototype built the same module with no reservation.
`reservation_bytes(source.len())` is 4,875,498 bytes on that source.

| Build | No-reservation displacement | Runs |
|---|---|---|
| unoptimized | 67,952,640 to 128,765,952 bytes | 6 |
| optimized | 121,409,536 to 121,585,664 bytes | 5 |

Every run passes the reservation by 14x to 26x, and the test fails on
6 of 6 runs against a `compile_jit` with no reservation. **The check
can still fail**, so core principle 9 holds.

Every run is also 16x to 31x under the 2,147,483,648 bytes a 32-bit
displacement carries. The test therefore reads the reservation, and it
does not witness the distance of §110's problem. The 8 GiB stack made
that distance. The doc comment now states both facts.

**Verified clean by the reviewer, and re-measured:** exit criteria 1 to
4; the 27 call sites in 17 files at `c72da05`; the 545 files and the
bracket depth of 8; every row of the `tsc` table; the three Red
failures at `0f8221a`; the five moved call sites of `codegen/` and
`cli/`, for order and error mapping; the two `#[cfg(test)]`
thread-locals, which drain before use; the panic count of
`compiler/tests/robustness.rs`, which `resume_unwind` did not change;
`bash -n tools/gate.sh`; `cargo test -p subscript-cli --test gate`, 14
passed.

### The gate after the review fixes

Two test doc comments carry the two measurements above (`93a4041`).
The x86_64-unknown-linux-gnu host ran the full gate again, with a
clean tree.

```text
gate full 93a40417e48395de1fdb2b28bae921d46c741e2e clean debug 1539/0/2 release 1536/0/2 skips 2/0 clippy 7/18/13 goldens-moved 0 exit 0
```

Record: `target/gate/20260921T105118Z-full.md`. Step wall seconds: fmt
1, build 0, debug 126, release 252, clippy 2, tsc 1, hygiene 0.

No CRITICAL and no MAJOR is open, so §114.5 criterion 6 is
discharged.

## Open

- The Windows and arm64 macOS full gates after the landing (§114.5
  criterion 5). Each host must measure the margin of the two
  `compiler/tests/nesting.rs` tests over the 2,097,152-byte libtest
  stack.
