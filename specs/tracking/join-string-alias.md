# `join` formats a string-literal alias — `compiler.md` §216

Status: COMPLETE.

## Measurement at the pin `673b1113`

`const ms: Mode[] = ["on", "off"]; print(ms.join(","));` gives S014
"`join` formats elements by the Q14 interpolation rules; `Mode` elements
are not interpolatable (Q22)". `` print(`${m}`) `` with `m: Mode` is
accepted.

The §204 text names a new format kind in the runtime formatter. The
checker already rewrites `` `${xs}` `` to `xs.join(",")`, and each tier
has the string form of one alias value, so a rewrite needs no change in
the three tiers.

The first text of §216 assumed `ms.map` without a measurement. At the
pin `ms.map`, `filter`, `forEach`, `includes`, and `indexOf` give S014:
`ArrElemKind::of` has no arm for `Type::StringAlias`. The coding agent
measured it, and §216.1 rule 1a exists for it.

## Implementation

Red, on a binary built from `1e10c5ac`: `a399` gives S014 "`join`
formats elements by the Q14 interpolation rules; `Mode` elements are not
interpolatable (Q22)".

Each row was run on a plain alias `"z" | "a" | "m"` and a wire alias
`CEnum<{ z: 9; a: 1; m: 5 }>`, on the three tiers, against `node`.

| Form | Pin | Now |
|---|---|---|
| `map`, `filter`, `forEach`, `some`, `every`, `findIndex`, `findLastIndex`, `flatMap`, `reduce`, `reduceRight` | S014 | equal to `node` |
| `includes`, `indexOf`, `lastIndexOf`, `sort(cmp)` | S014 | equal to `node` |
| `reverse`, `slice`, `concat`, `fill`, `push`, `unshift`, `splice` with no insert, `copyWithin` | accepted | equal to `node` |
| `join`, `toString()`, `` `${ms}` `` | S014 | equal to `node` |
| `find`, `findLast` | S014 | S014, the text of an enum array |
| `sort()` with no comparator, `sort` with `localeCompare`, `splice` with an insert | S014 | S014 |

`sort()` with no comparator is rejected for each element type, so no
method orders by the discriminant.

`codegen/src/lir_types.rs` `array_element_kind` is a second copy of
`ArrElemKind::of`. With only the first copy changed, JIT and AOT gave an
internal error on `includes`.

`every_hir_execution_fact_is_carried_by_lir` required one capacity
allocation at a position. A written `ns.filter(f).map(g)` gives two calls
at one position, and it failed that check at the pin too. The HIR count
at the position is now the requirement.

`gate full e6f03a14 dirty:10 debug 3079/0/5 release 3076/0/5 skips 2/0 clippy 5/18/11 goldens-moved 1 exit 0`. `tools/hygiene.sh` exit 0. The moved golden is `a399`, which this section adds.
