# §191 — a padding does not end inside a UTF-8 sequence

Date: 2026-10-11. Contract: `specs/blocks/compiler/s191-a-padding-does-not-end-inside-a-utf-8-sequence.md`.
Contract pin: `88a4124b`. Red pin: `3112f7f7`. Apple arm64, debug
build of `subscript-cli` and `subscript-runtime`.

## 1. Padding implementations

One implementation exists. Every tier calls it.

| Site | Role |
|---|---|
| `runtime/src/strops.rs` `pad_fill_len`, `pad_cut_inside_sequence` | The fill length and the boundary rule (§191 rule 1). |
| `runtime/src/strops.rs` `pad` | The `Vec` reference. Only tests call it. |
| `runtime/src/strops.rs` `pad_into` | The writer into the exact-size result. |
| `runtime/src/ffi/strings.rs` `str_pad` | The body of `subscript_rt_str_pad_start` and `subscript_rt_str_pad_end`. It runs the check before `alloc_str_with`. |
| `codegen/src/jit/symbols.rs` | The dev tier binds the two runtime symbols. |
| `codegen/src/lir.rs` (`PadStart`, `PadEnd`) | The ship tier calls the two runtime symbols. The C runtime path links `libsubscript_runtime.a`; it has no padding of its own. |
| `codegen/src/interpreter/intrinsics.rs` (`PadStart`, `PadEnd`) | The interpreter calls the two runtime symbols. |

The compiler does not fold `padStart` or `padEnd`.

## 2. Corpus entries

- `corpus/trap/t110-pad-start-cut-inside-sequence`
- `corpus/trap/t111-pad-end-cut-inside-sequence`
- `corpus/accept/a368-pad-cut-on-boundary`

Trap text, measured at the working tree on the dev tier
(`subscript run`):

```text
t110-pad-start-cut-inside-sequence.ts:13:23: trap [string-slice]: padStart(2): the cut is at pad byte 1, inside a UTF-8 sequence
t111-pad-end-cut-inside-sequence.ts:13:23: trap [string-slice]: padEnd(2): the cut is at pad byte 1, inside a UTF-8 sequence
```

The ship tier (`subscript build --run`) prints `trap 3 11` and the same
message. Kind 3 is `TrapKind::StringSlice`. The pre-trap stdout is the
`.expected` file on both tiers.

## 3. Red at `3112f7f7`

Built from a `git archive` copy outside the repository. The
interpreter ran through a temporary example binary in that copy:
`check_program`, `lower_module`, `interpret`.

| Entry | dev JIT | ship C AOT | interpreter |
|---|---|---|---|
| t110 | no trap; stdout `あA 4`, `before`, `2`; exit 0 | same | same |
| t111 | no trap; stdout `Aあ 4`, `before`, `2`; exit 0 | same | same |
| a368 | the golden; exit 0 | the golden | the golden |

The two trap entries are Red: the cut result exists, and its length is
2. The accept entry is green at the pin, because rule 2 keeps every
boundary cut unchanged.

## 4. `node` 24.18.0

Run with type stripping and a `print` shim that calls `console.log`.

| Entry | `node` output | Same as golden |
|---|---|---|
| t110 | `あああA 4`, `before`, `2` | no: the target counts UTF-16 units (Q5) |
| t111 | `Aあああ 4`, `before`, `2` | no (Q5) |
| a368 | `あxあxあxあA 8`, `Aあxあxあxあ 8`, `あxあxA`, `𠮷𠮷A`, `A𠮷𠮷𠮷𠮷`, `xyxab`, `abxyx`, `  7`, `あ`, `あい`, `あい` | 6 ASCII and unchanged lines match; 5 multibyte lines differ (Q5) |

Each entry carries `js-comparable: no` with Q5.

## 5. Cost

The check is one remainder and one read of a pad byte for each call.
It runs before the allocation, so a trapped call allocates nothing.
The unit test `ffi_pad_cut_inside_a_sequence_traps_before_the_result_exists`
compares `Context::live_count` before and after each trapped call.

## 6. Tutorial measurement

`docs/tutorial-typescript.md`, section "Strings are UTF-8". The
program output in the tutorial is the output of `subscript run` at the
working tree. Measured with `node`: `"日本語"` gives length 3,
`indexOf("本")` 1, `slice(1, 4)` `本語`, `charCodeAt(1)` 26412.
Measured with `subscript run`: `"日本語".at(1)` traps with
`trap [string-range]: at(1) normalizes to 1, which is not on a UTF-8 boundary`,
and
`"日本語".slice(1)` traps with `trap [string-slice]: slice(1, 2147483647)
normalizes to (1, 9), which is not on a UTF-8 boundary`.

## 7. Phase Review MINOR fixes

- The tutorial states 3 bytes for each character of `"日本語"` only. An
  ASCII character is 1 byte, and `𠮷` is 4 bytes.
- `subscript_rt_str_at` named `charAt` and `codePointAt` in its trap
  messages. It now names `at`, with the argument and the normalized
  index: `at(-4) normalizes to 2, which is not on a UTF-8 boundary` and
  `at(3) normalizes to 3, out of range for string length 3`. The only
  table that moved is `codegen/tests/stdlib_array_search.rs`. No golden
  holds these messages.
- The purpose header of a368 states the result, not a comparison with
  earlier behaviour.

## 8. Open

- `codegen/tests/cemit.rs` has 1,986 lines. The cap is 2,000
  (`specs/blocks/compiler.md` §5.y). The next trap entry with an exact
  expectation can move it over the cap.
