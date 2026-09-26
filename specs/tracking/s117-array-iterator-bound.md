# §117 An array iterator reads its bound from the header — evidence

Contract: `specs/blocks/compiler/s117-an-array-iterator-reads-its-bound-from-the-header.md`.
Date: 2026-09-27. Host: arm64 macOS.

## Bisection

`cross-language --only callbacks`, each tree exported with
`git archive`, built with its own target directory. `subscript-jit`
median, ms (threshold 370):

| Tree | jit | ship |
|---|---|---|
| `2335523` | 281.406 | 37.114 |
| `dd2e744` | 459.394 | 37.321 |
| `b849861` | 459.700 | 37.246 |
| `3677d1f` | 455.779 | 37.204 |
| `3aebd13` | 456.192 | 37.175 |
| `85242e9` | 282.084 | 37.168 |
| `18c8dcb` | 289.850 | 37.550 |
| `9fcde17` | 456.308 | 37.442 |
| `0bd3a19` | 284.285 | 37.019 |
| `61cc91e` | 453.513 | 37.013 |
| `79ea54b` | 454.091 | 37.021 |

First slow: `79ea54b` "runtime: 28 MINOR consolidations (§78)". Last
fast: `0bd3a19`.

## Isolation (codex measurement round, trees exported, nothing landed)

| Variant | run 1 | run 2 |
|---|---|---|
| `0bd3a19` unchanged | 281.221 | 280.546 |
| `0bd3a19` + the `require_live_handle` call in `subscript_rt_array_len` | 455.860 | 455.428 |
| `79ea54b` unchanged | 455.280 | 455.583 |
| `79ea54b` − that call | 342.126 | 281.118 |
| `306f4f4` unchanged | 459.200 | 459.939 |
| `306f4f4` − that call | 280.233 | 280.349 |

Checksum `-662567840` in every run. The benchmark path
(`jit_bench` → `run_entry` → `execute_entry(..., false, ...)`) runs
with freed-handle diagnostics off. Per element, the dev-JIT loop calls
`subscript_rt_array_len` for the bound and `subscript_rt_array_data`
for the value; 55 million element visits.
