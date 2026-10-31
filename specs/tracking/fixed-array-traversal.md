# A `FixedArray` traversal reads its place — `compiler.md` §220

Status: COMPLETE.

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

The cause of the ship-tier figure is the cursor, not the copy. With both
copies gone and the cursor kept, `for…of` took 0.39 0.35 0.34 s.

## Red

At the pin behaviour, with `subscript run`, pin output then golden:

- `a405`: each form of the table `1,2` / `1,9`. `map` `10,21` / `10,81`.
  `reduce` `13` / `15`.
- `a406`: whole local `1,2` / `1,8`. Shifted element `1,2` / `1,4`.
- `a407`: `b` / `yield`, `q` / `element`.
- `a408`: `top 2` / `top 3`.
- `a409`: `stores counted operand 0 (value 0) without a fresh single-use
  owner or a preceding retain`, at the tree before the review fix.
- `a410`: `call target identity/signature is invalid`, at the tree before
  the review fix.
- `t113`: `step 1`, `step 2`, `trap [empty-pop]` / `step 1`,
  `index-out-of-bounds`.

## Cost after the change

User seconds, three runs each:

| Measurement | `for…of` | Index loop |
|---|---|---|
| ship tier, 300,000 traversals | 0.10 0.06 0.06 | 0.09 0.06 0.06 |
| `subscript run`, 3,000,000 traversals, exit after one element | 0.02 0.02 0.02 | 0.02 0.02 0.02 |

## Moved files

No `.expected` file moves. `codegen/tests/lir-goldens/corpus.txt` adds the
sections of `a406`, `a407`, `a409`, and `a410`, with 3,825 lines added and
0 removed: each entry has a generator or an async function.

## Verification

`gate full b6d87c4b dirty:51 debug 3134/0/5 release 3131/0/5 skips 2/0 clippy 5/10/13 goldens-moved 1 exit 0`. `tools/hygiene.sh` exit 0.
