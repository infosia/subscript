# §93 — an async method declares type parameters

Contract: `specs/blocks/compiler.md` §93. Pin for every measurement
below: `61c64c36ec1b0bd678a9e6a579140be77c2f120d`. Host: aarch64
macOS; rustc 1.95.0; Apple clang 21.0.0 (clang-2100.1.1.101); tsc
5.9.2; node v24.18.0.

## Why the section exists

§82.4 accepts `recv.m<A>(...)`. §64 accepts `await f<A>(...)` and
`await recv.m(...)`. Only the combination of the two accepted
features is rejected, and `tsc` accepts it. §82.4 rule 5 gave the
reason as "the await grammar gains no form", which states a fact
about the checker, not about the language. The fix is mechanical: the
sync call path already instantiates a generic method at the call, and
the await path lacks that one step.

## Measured at the pin

Each program below is in the §93 measurement list. The command is
`subscript run <file>`; every one exits 1.

1. An async method with type parameters on a non-generic reference
   class: S100 "async generic methods are not in the decided surface"
   at the declaration, with a divergence block.
2. The rejection is at collection. A call without type arguments, a
   floating call, and a read of the method as a value each report
   that same declaration diagnostic, at the declaration line.
3. On a generic class: S100 "generic classes cannot declare generic
   methods".
4. `static async m<T>()`: S100 "async static methods are not in the
   decided surface".
5. On a `@CStruct` value class: S100 "async generic methods are not
   in the decided surface". The value-class rejection sits after the
   generic-method branch in `collect_class`, so the branch masks it.
6. Bodiless, in a `declare class` written in a `.ts` source: the same
   masked result, where §82.4 rule 1a states "function bodies are
   required".

Items 5 and 6 are message defects. Each program is rejected today,
and each one names the wrong rule.

## Red, at the pin

The three corpus sources of §93.3 were run at the pin from a scratch
directory. Every one exits 1 with the item-1 diagnostic at the method
declaration:

| Entry | Line reported | Line the section requires |
|---|---|---|
| `a187-async-generic-method` | 20, the declaration | none; the entry must run |
| `r186-async-generic-method-without-type-args` | 8, the declaration | the call |
| `r187-async-generic-method-on-value-class` | 12, the declaration | the declaration, other rule |

`tsc -p` over the three sources with the repository prelude and the
repository `tsconfig.json` options exits 0. The `tsc: accepts` header
of each entry is measured, not assumed.

## Correction found while writing the section

The `compiler.md` §0 section index stopped at §89. §90, §91, and §92
were absent. The rows are added with §93's. No check reads the index,
so nothing reported the gap.

## Round 1 — what the implementation round measured

The round implemented §93.1 rules 1 to 10 and reported three contract
defects. Each is measured, and each one changed the contract.

### The bodiless declare case rejects under `tsc`

§93.3 item 5 asked for a `BodilessDeclareGenericMethod` divergence
block on the async form. `tsc` 5.9.2 rejects that form, so §79 rule 2
forbids the block:

| Source | tsc result |
|---|---|
| `declare class Box { async load<T>(value: T): Promise<T>; }` | TS1040 "'async' modifier cannot be used in an ambient context", exit 2 |
| the same without `async` | exit 0 |

Command: `node_modules/.bin/tsc --noEmit --strict --target ES2020
--skipLibCheck <file>`. Measured twice, by the round and again here.
§93.1 rule 11 records the corrected rule.

### The aggregate LIR snapshot gains a block by construction

`codegen/tests/lir.rs`
`coroutine_and_measurement_lir_text_matches_goldens` collects every
async corpus entry, so a187 adds a block to
`codegen/tests/lir-goldens/corpus.txt`. Both profiles reported:

```
LIR text golden differs at line 11071 (actual 1139176 bytes, expected 1117090 bytes)
```

The added block is 22,086 bytes. With that block removed, the
captured text is byte-identical to the committed snapshot, so no
pre-existing block moves. §93.3 item 8 records the corrected
requirement: capture the snapshot and record the move under §2.

### The LIR half of the two-instance assertion needs a codegen crate

`compiler/tests/` does not depend on `subscript-codegen`, and
`lower_module` belongs to that crate. The sync equivalent is
`codegen/tests/cemit.rs`
`generic_method_instances_hold_distinct_hir_names_and_lir_ids`.
§93.2 now names that file.

### Measured results the round reports as green

a187 runs and matches on three witnesses. The measured golden equals
the predicted one, and the round wrote the file from captured stdout:

```
load:i32:start
load:i32:resume
number=7
load:vec2:start
main:held
load:vec2:resume
vector=1.5,2.5
calls=2
```

`a187 HIR ["load<i32>", "load<Vec2>"]; LIR {FunctionId(1),
FunctionId(2)}`. The independent SHA-256 comparison of 252
pre-existing golden and `.expected` files reports no move.

Gate verdict, round 1:

```
gate full cbba2b8294af82971a776a34ad8c4d20f4fc693c dirty:13 debug 1308/3/2 release 1306/3/2 skips 2/0 clippy 7/18/13 goldens-moved 0 exit 1
```

The three failures per profile are the LIR snapshot above, and two
gate-wrapper tests that the hygiene failure below causes.

### A hygiene failure this session caused

`tools/hygiene.sh` reported `agent session trailer in a commit
message` against two commits of this session. The trailer came from a
tool instruction, not from the repository rules. `hygiene.sh` rejects
it, and the repository rules govern. The two commits are rewritten
without the trailer.

## Round 2 — green

Tasks D to G landed. Gate verdict, round 2:

```
gate full ab9f814d7113a2b888301e988d16f66004b50764 dirty:15 debug 1320/0/2 release 1318/0/2 skips 2/0 clippy 7/18/13 goldens-moved 1 exit 0
```

Record: `target/gate/20260908T101847Z-full.md`. Every step exits 0,
including `tools/hygiene.sh`.

### Verified here, not only reported

- The LIR snapshot grows by exactly a187's block. With the 22,086
  bytes of `===== a187-async-generic-method =====` removed, the new
  file equals `git show HEAD:codegen/tests/lir-goldens/corpus.txt`
  byte for byte. `goldens-moved 1` names that one file.
- a187 runs and matches its committed golden.
- r186 reports one diagnostic, S100 at 16:25, with the
  `GenericMethodTypeArguments` block.
- Counts: accept `.ts` 185, `.expected` 186, reject `.ts` 175.
- The file set equals the authorized set. Nothing under `specs/`,
  `codegen/src/`, `tools/`, or any Cargo manifest changed.

### The reach of rule 12, measured

Rule 12 sends every collection rule that rejects a method with type
parameters through one record. The round reports one consequence
beyond the call: a static method with type parameters on a generic
class now reports "generic classes cannot declare static members"
alone, where it reported that rule and "generic classes cannot
declare generic methods" together. §93.1 rule 12a records it. Sixteen
tests in `compiler/tests/async_generic_method.rs` pin the ten
collection rules, each beside an accepted control in the same shape.
