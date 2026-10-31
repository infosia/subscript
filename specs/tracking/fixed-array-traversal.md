# A `FixedArray` traversal reads its place — `compiler.md` §220

Status: IN PROGRESS.

## Step 0, at `bb79fcd0`

`node` v24.22.1 against `subscript run`, with the probe forms of §220:

| Form | subscript | `node` |
|---|---|---|
| local | `1,2` | `1,9` |
| global | `1,2` | `1,9` |
| field | `1,2` | `1,9` |
| nested field | `1,2` | `1,9` |
| element of a `FixedArray` | `1,2` | `1,9` |
| element of a dynamic array | `1,2` | `1,9` |
| `forEach` over a global | `1,2` | `1,9` |
| `r = s` in the body of `r.a` | `1,2` | `1,2` |
| `b = c` in the body of `b` | `1,2` | `1,2` |
| rows of a nested `FixedArray`, with a later row changed | `1,9,3,9` | `1,9,5,9` |

Cost, user seconds on the ship tier, three runs each, 300,000 traversals
of a `FixedArray<f64, 256>` class field:

| Loop | Runs |
|---|---|
| `for…of` | 0.38 0.35 0.35 |
| index loop | 0.09 0.06 0.06 |

`subscript run`, 3,000,000 traversals with an exit after one element:
`for…of` 0.15 s, index loop 0.01 s.
