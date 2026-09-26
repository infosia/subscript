<!-- §117 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 117. An array iterator reads its bound from the header

*(Added 2026-09-27.)* Origin: the arm64 benchmark snapshot at
`306f4f4`. The owner asked for the cause of a dev-JIT regression and
chose this form on 2026-09-27.

Problem, measured on arm64 macOS with
`cross-language --only callbacks`, `subscript-jit` median:

| Tree | ms |
|---|---|
| `2335523` | 281.4 |
| `0bd3a19` | 281.2, 280.5 |
| `79ea54b` (§78 runtime consolidations) | 455.3, 455.6 |
| `306f4f4` | 459.2, 459.9 |

A bisection over exported trees found `79ea54b` as the first slow
tree. Two controls isolate one line of that commit, the
`require_live_handle` call added to `subscript_rt_array_len`:

- `0bd3a19` with only that call added: 455.9, 455.4 ms.
- `306f4f4` with only that call removed: 280.2, 280.3 ms.

The ship tier stayed at 37 ms throughout. The benchmark runs with
freed-handle diagnostics off, so the cost is the call and its two
branches, not a table search: about 3 ns per call over 55 million
calls.

The call is on the per-element path because the two tiers lower the
bound of an array iterator differently:

- the ship C reads `((const SsArrayHeader*)subject)->len`
  (`codegen/src/cemit/iterator.rs`);
- the dev JIT calls `subscript_rt_array_len` for each bound
  (`codegen/src/lower/func/iterator.rs` `iterator_current_bound`).

The array iterator carries the static-callback expansion of
`map`/`filter`/`reduce` (§8.1d B) and `for…of` over an array. An indexed
element access already reads the header on both tiers.

### 117.1 The rule

1. The bound of an array iterator (`ArrayValues`, `ArrayKeys`, and
   their reverse forms) is the `len` field of the array header, read
   by the generated code on both tiers. No runtime call reads it.
2. A freed-handle diagnostic (§8.1a-1 to §8.1a-3) runs where both
   tiers run it. The array iterator bound is not such a site on the
   ship tier, so it is not one on the dev tier. The runtime entry
   `subscript_rt_array_len` keeps its check for its other callers.
3. The interpreter keeps its current bound read. It is not a timed
   tier, and its stdout and trap tuples do not change.

### 117.2 Goldens that move

None. The LIR does not change: the bound is an engine lowering. If a
golden moves, stop and report.

### 117.3 Exit criteria

1. `cross-language --only callbacks`, `subscript-jit` median, two runs:
   within 5% of 281 ms. The ship median does not change by more than
   the run-to-run spread.
2. `tools/gate.sh full` is green.
3. A test pins the dev-JIT bound read: an array iterator lowers with no
   call to `subscript_rt_array_len`, with a firing control (the string
   iterator, which still calls `subscript_rt_str_len`).
4. `tools/hygiene.sh` is clean.
