# §192 — a ship link removes unreferenced code

Date: 2026-10-11. Contract:
`specs/blocks/compiler/s192-a-ship-link-removes-unreferenced-code.md`.
Contract pin: `826367be`. Host: Apple arm64, Apple clang 21.0.0
(clang-2100.3.34.2). Sizes are in bytes.

## 1. Link sites

`unreferenced_code_removal_arguments` (`codegen/src/ship.rs`) gives the
flag for a compiler style. These sites add it after
`add_executable_output`:

| Site | Link |
|---|---|
| `codegen/src/ship.rs` `build_c_aot` | Every `run_c_aot*` ship run. |
| `cli/src/lib.rs` `compile_build` | `subscript build`, and the two example hosts through it. |
| `cli/src/lib.rs` `link_flags_command` | `subscript link-flags` prints the flag on its last line. |
| `codegen/src/ship_tests.rs` `run_c_aot_with_entry` | The test host links. |

These sites also pass the flag, so each measures or shows the shipped
form:

| Site | Link |
|---|---|
| `benchmarks/src/bin/async-cost.rs`, `cross-language.rs`, `perf-gate.rs` | The ship-tier subject. The hand-C baselines do not link the runtime and do not pass the flag. |
| `benchmarks/src/bin/bound-call.rs` `link_sources` | Each link that takes the runtime archive. |
| `benchmarks/src/bin/regex-size-gate.rs` | The two size-gate links. The function replaces the literal `-Wl,-dead_strip`; the argument list on macOS arm64 is unchanged. |
| `codegen/device-link.sh` | iOS and macOS `-Wl,-dead_strip`; x86-64 Linux and Android `-Wl,--gc-sections`. |
| `docs/tutorial-rust.md` | The host link command reads `subscript link-flags`, as the C/C++ tutorial does. |

The benchmarks are built, not run. `codegen/device-link.sh` ran on
this host: the iOS link and the macOS link pass (iOS 809,296 bytes,
macOS 815,744 bytes, `a22-matrix-propagation`). The macOS output
matches `a22-matrix-propagation.expected`. The Linux and Android
sections did not run.

These test links keep the full archive. They do not pass the flag:

- `codegen/src/finally_tests.rs`;
  `codegen/src/interpreter/budget_tests.rs`,
  `counted_measurement_tests.rs`, `inspection_tests.rs`,
  `reference_holder_tests.rs`.
- `codegen/tests/async_cleared_trap.rs`, `cemit/operations.rs`,
  `host_export_boundary.rs`, `promise_reaction.rs`.

`examples/rust-host` uses the dev tier only and links no runtime archive.

## 2. Rule 3: used symbols stay

These tests link with the flag and pass:

- Host exports: `examples/tests/gate.rs`
  `capstone_host_builds_runs_and_matches_golden` and
  `context_per_scene_host_builds_runs_and_matches_golden`. Each host
  `main.c` calls the program exports; each output equals the pin output.
- Native-library symbols: `codegen/tests/native_library.rs`
  (`static_archive_link_input_follows_translation_units_on_all_tiers`,
  `no_opt_in_hard_signal_returns_retained_output_on_both_tiers`),
  `codegen/tests/interop.rs`, `codegen/tests/golden.rs`, and
  `examples/tests/gate.rs` `every_example_matches_dev_jit_ship_c_aot_and_golden`.
  Each runs through `build_c_aot`.

## 3. Size test

`ship_link_is_at_most_half_the_control_link`
(`codegen/src/ship_tests.rs`, one `cfg` item for macOS and Linux) reads
the executable that `build_c_aot` links for one `print` program. The
control links the same `program.c` and `entry.c` by hand without the
flag. The test asserts that the ship link is at most half the control.
A ratio replaces an absolute bound: an ELF executable keeps the DWARF
of its live members.

| Archive profile | Ship link | Control, no flag | Ratio |
|---|---:|---:|---:|
| debug | 1,425,864 | 8,457,752 | 0.17 |
| release | 798,936 | 3,732,456 | 0.21 |

The test takes 0.18 s (debug) and 0.16 s (release). With the flag
removed from `build_c_aot`, the test fails: both links are 8,457,752
(debug). No Linux link is measured.

## 4. Link time

`corpus/accept/a01-hello`, release archive. The two objects are
compiled once. Each link runs 11 times, the two forms alternate.

| Link | Median | Min | Max |
|---|---:|---:|---:|
| `-Wl,-dead_strip` | 0.038 s | 0.037 s | 0.040 s |
| no flag | 0.041 s | 0.041 s | 0.042 s |

## 5. Test wall

`cargo test --offline --locked --no-fail-fast`, after a `--no-run`
build. Release runs set `SUBSCRIPT_FULL_INTERPRETER_SWEEP=1`. The CLI
set is every `subscript-cli` target except `gate`.

| Suite | Profile | Pin wall | After wall | Pin tests | After tests |
|---|---|---:|---:|---:|---:|
| `subscript-codegen` | debug | 299 s | 280 s | 928 | 930 |
| `subscript-codegen` | release | 330 s | 323 s | 927 | 929 |
| `subscript-cli` | debug | 16 s | 15 s | 95 | 95 |
| `subscript-cli` | release | 10 s | 9 s | 95 | 95 |
| `subscript-examples` | debug | 23 s | 16 s | 7 | 7 |
| `subscript-examples` | release | 6 s | 6 s | 7 | 7 |

Every run has 0 failed. Codegen has 1 ignored test in each run. The
two new tests are the size test and
`unreferenced_code_removal_follows_the_platform_and_the_style`.

## 6. Executable sizes

Release archive, `subscript build`. "Stripped" is after `strip`.

| Executable | Pin | After | Pin stripped | After stripped |
|---|---:|---:|---:|---:|
| `examples/host` (`game`) | 3,733,928 | 889,912 | 2,842,440 | 742,152 |
| `examples/context-per-scene` (`scene`) | 3,733,864 | 888,840 | 2,842,424 | 741,896 |
| `corpus/accept/a01-hello` | 3,732,456 | 798,936 | 2,841,944 | 658,296 |

The stdout of each example host is byte-identical to its pin output.

## 7. Windows

No Windows build ran. `/OPT:REF` follows `-link`
(`unreferenced_code_removal_follows_the_platform_and_the_style`
asserts the argument order). A Unix-style link on Windows (windows-gnu)
gets `-Wl,--gc-sections`.

Open: a windows-gnu host whose `PATH` clang targets MSVC passes
`-Wl,--gc-sections` to `link.exe` or `lld-link`, which do not know the
option. No fix is in this section.
