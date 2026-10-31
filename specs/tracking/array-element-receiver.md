# A method that moves its array is rejected on an element — `compiler.md` §221

Status: COMPLETE.

## Step 0, at `020215de`

`node` v24.22.1 against the three tiers, on `all: V[]`, with a method body
that first pushes 5,000 elements:

| Form | JIT and ship | `node` |
|---|---|---|
| `return this.b` | `0` | `2` |
| `this.a = 7`, then `all[0].a` | `1` | `7` |
| `all[1].a = f()`, with a growth in `f` | `9` | `9` |
| `all[2].a += f()`, with a growth in `f` | `10` | `10` |

The last two forms have an address in one function, and §68.2 item 9
recomputes it.

The corpus has 2 element method sites, both in `a150`, by a text search of
the entries with a value method.

## Cost

User seconds, 100,000,000 calls, element class with three writes:

| Tier | Size | `all[i].step(0.5)` | Copy, call, assign |
|---|---|---|---|
| ship | 12 bytes | 0.09 0.06 0.06 | 0.10 0.06 0.06 |
| ship | 64 bytes | 0.09 0.06 0.06 | 0.10 0.06 0.06 |
| JIT | 12 bytes | 0.21 0.21 | 0.45 0.45 |
| JIT | 64 bytes | 0.21 0.21 | 0.94 0.94 |

The JIT cost is at an element receiver only. The dev tier is not the
measured tier of §3.

## Copy form, rejected

The first form of §221 copied the element to a hidden local and wrote it
back. It passed the quick gate, and a Phase Review measured these
programs. Each one is equal to `node` at the pin.

| Program | copy form | `node` |
|---|---|---|
| depth-first visit of a cycle of 3 array elements | no end: JIT signal 4, ship exit 139 | `count 3 1 2 3` |
| `this.balance -= amount; accs[j].balance += amount`, `j` equal to self | 90 | 100 |
| `all[0].recur(d - 1); this.a += 1`, 3 levels | 2 | 5 |
| `all[0].a = 40; this.b = 41` | a=1 | a=40 |
| `list.splice(i, 1); this.hp = 0` | the removed element returns | `2 2 10 3 10` |

Cost, seconds, 3,000,000 iterations of `rows[n & 63].put(n, 1.5); total +=
rows[n & 63].at(n);`:

| Element | ship, copy | ship, address | JIT, copy | JIT, address |
|---|---|---|---|---|
| 256 bytes | 0.035 | 0.012 | 0.11 | 0.02 |
| 4,096 bytes | 0.645 | 0.012 | 1.07 | 0.02 |

The cost table above measured 12 and 64 bytes with a method that the C
compiler inlines, so it did not show this cost. The form stays out of the
tree.

## Red

At the pin behaviour, each reject entry is accepted: `r464` prints `0`,
`r465` `1`, `r466` and `r467` `9`, `r468` `2 0`, `r469` `1`, `r470` `2`,
`r471` `1`, `r472` `2`, `r473` `2`, `r474` `1`, `r479` `1`, and `r482` `2`.
`r475`, `r476`, `r477`, `r478`, `r480`, `r481`, and `r483` give no error.
`a415` at the pin on the JIT: `write 9|grow 0|const 7 7`. On the ship tier:
`write 2|grow 2|const 2 2`.

`a411` to `a414` pass at the pin: they guard the forms that the copy form
broke.

## Rejected calls

The rule rejects 0 calls in `corpus/accept`, `corpus/warn`, `corpus/trap`,
`corpus/interop`, and `examples/`.

## Interpreter poison

The registry was keyed by `ValueId` and shared by all frames. `a413`, a
correct recursion, stopped with a false poison. The registry is now in
`Frame`.

## Pass time

Release build, before the foreign unit: 3.5 to 9 µs on `a204`, the largest
file. 2.45 ms over 339 accept entries, maximum 99 µs. 14.8 ms of a 4.8 s
check on 20,000 functions. 0.23 s on 1,500 element types and 4,000 chain
functions.

## Verification

`gate full ce88d7af dirty:61 debug 3160/0/5 release 3157/0/5 skips 2/0 clippy 5/10/13 goldens-moved 1 exit 0`. `tools/hygiene.sh` exit 0. The moved golden is `codegen/tests/lir-goldens/corpus.txt`, which adds the section of `a415` with 0 lines removed.
