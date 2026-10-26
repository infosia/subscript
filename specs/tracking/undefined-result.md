# A reference result can be `undefined` — `compiler.md` §195

Status: landed.

## Measurement at the pin `ee65e587`

Tools: `tsc` 5.9.2 with this project's options, `node` v24.22.1, and
`target/release/subscript` built from the pin.

`tsc`, for `u: Box | undefined` and `n: Box | null`:

| Form | `tsc` |
|---|---|
| `return null` in a `Box \| undefined` function | TS2322 |
| `const x: Box \| null = u` | TS2322 |
| `if (u !== null) { u.v }` | TS18048 |
| `u === null`, `n === undefined`, `u === n`, `n == undefined` | no error |
| `return;` and a reachable body end in a `Box \| undefined` function | no error |
| `class Store<V> { get(i: number): V \| undefined }` | no error |

`node`, both values absent: `u === undefined` true, `u == null` true,
`u == undefined` true, `n == undefined` true, `u === null` false,
`n === undefined` false, `u === n` false, `u == n` true.

subscript at the pin:

| Program | Output |
|---|---|
| `const x = new Map<string, Box>().get("a"); print(\`${x === null}\`)` | `true` (`node`: `false`) |
| the same with `x === undefined` | S012 (`node`: `true`) |
| `Map<string, Box \| null>`, `set("k", null)`, `get("k") === null`, `get("z") === null` | `true true` (`node`: `true false`) |
| `class Store<V> { first(): V \| null }` with `Store<Box>` | runs |
| the same with `Store<i32>` | S011 |
| `${b}`, `!b` for `b: Box \| null` | S100 |
| `const f = o.get` for a method `get` | S100 |

## Red

Each new entry gives this first diagnostic at the pin. None is its
expected error.

| Entry | Diagnostic at the pin |
|---|---|
| `a373`, `r422`–`r424`, `r426`–`r434` | S012 `` `undefined` is banned; the single null story is `null` `` |
| `r425`, `r435` | S012 `` `undefined` is legal only in a presence test on an absence-capable descriptor member `` |

The 15 entries pass `tsc`. The `a373` output under `node` equals its
golden. Each line of the golden was checked by hand.


## Landing

The checker type is `Type::Nullable(Box<Type>, Absence)`. The HIR
builder erases the `undefined` spelling once, in
`compiler/src/hir/spelling.rs`. The §195.1 rule 6 check reads the same
type-slot walk before the erasure.

`codegen/tests/undefined_result_lir.rs` lowers one program with the
`undefined` spelling and the same program with `null`: the two LIR
texts are identical, so the run-time code is the code of `T | null`.

Three accept entries change their source from `=== null` to
`=== undefined` on a `Map.get` result (`a51`, `a66`, `a272`). Their
outputs do not change.

Gate at the contract `b86970a3` with the implementation in the working
tree: `gate full … debug 2881/0/5 release 2878/0/5 skips 2/0 clippy
5/18/11 goldens-moved 0 exit 0`.
