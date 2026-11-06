# A benchmark compiles with the ship C compiler — `compiler.md` §225

Status: landed. The load measurement of §225.2 follows.

Host: 20 logical CPUs, x86-64, Windows, MSVC 19.44.35222, clang 22.1.6.

## Red at `859b3a1a`

The header of `perf-gate --gate` read
`C compiler:  clang version 22.1.6 ... [-O2 -ffp-contract=off]`.

## After the landing

The header names `cl.exe` version 19.44.35222 for x64, and the flags
`/nologo /std:c11 /O2 /utf-8 /fp:strict /we4028 /we4029` for both
subjects. Every subject matched the `a22` golden and the `collect`
checksum. One quiet `perf-gate --gate` run, exit 0:

| Item | Figure | Limit |
|---|---|---|
| `a22` ship | 1.74x | 2.50x |
| `a22` dev-JIT | 5.98x | 25.00x |
| `collect` ship | 1.10x | 7.50x |
| `collect` dev-JIT | 3.59x | 6.00x |
| dev iteration | 6.141 ms | 20 ms |
| hot reload | 0.888 ms | 20 ms |

`tools/gate.sh full`: exit 0, debug 3102 passed, release 3099 passed, 0
failed. No golden moved.

The Phase Review found one MAJOR: a test pinned the old Unix compiler
rule, and the ship tier can select `clang-NN`. Rule 3 now states the
ship resolution, and the test is removed. The two copies of the command
construction are now one file.
