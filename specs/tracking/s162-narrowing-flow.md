# §162 — Narrowing flow

Contract: `specs/blocks/compiler/s162-the-narrowing-flow-follows-negation-loops-and-lambdas.md`.
Contract pin: `e5a29459`.
Code pin: `888b68d0`.
Red evidence pin: `e5a29459`.

## Red evidence

TypeScript used the repository `tsconfig.json`, the prelude, and one entry per check.
TypeScript 5.9.2 accepted `a333` and rejected r366 to r369 with one TS18047.
Node v24.18.0 ran the TypeScript-transpiled CommonJS module with `print` bound to an output collector.
Its `main` output supplies the new golden.
No existing golden changed.

| Entry | Pin diagnostic positions | TypeScript result |
| --- | --- | --- |
| a333-narrowing-flow | S011 at 10:79, 11:96, 12:107, 13:120, 14:121, 15:95, 16:119, 17:143, 18:93 | Accepts |
| r366-nullable-loop-store | S011 at 12:89 | TS18047 at 12:89 |
| r367-break-before-assignment | S011 at 12:80 | TS18047 at 12:80 |
| r368-unrelated-loop-condition | S011 at 12:46 | TS18047 at 12:46 |
| r369-lambda-without-guard | S011 at 12:40 | TS18047 at 12:40 |

Each S011 message is: `` `A | null` may be null here; narrow with a null check first ``.
Each diagnostic has the nullable-union rule note and no C17 divergence block.
The pin CLI exits with status 1 for each entry.
The accept entry has nine diagnostics. Each reject entry has one diagnostic.

The golden is:

```text
1
1
1
2
1
1
1
3
1
```

## Implementation

Negation exchanges the true and false facts at each depth.
Each loop exit joins the false condition facts and each reachable break edge.
Nested loops and switches consume their own breaks.
Each exit carries its live facts and C17 end notes.
The for update joins the body end and each reachable continue edge.
An unreachable break supplies no exit fact.

Rule 3a uses a closed list of source forms.
New expressions and non-null literals count as non-null.
A local or parameter read requires a written non-null annotation that names no type parameter.
A contextual lambda parameter and a parameter typed from its default supply no written annotation.
`ParamSig::annotated` and `Local::annotated` retain this source fact.
`ExprKind::Local` carries it beside the local name and storage type.
Destructured parameters retain their signature's provenance; for-of and synthetic bindings supply none.
`Checker::written_type` reads the annotation syntax and the enclosing type-parameter names, before substitution can supply proof.

A field read requires a non-null declaration in the class source that names no type parameter.
`Field::written_non_null` carries this fact from the source declaration into each checked instance.
Rule 3a reads this source fact instead of the substituted instance field type.
A global read uses the global-symbol declaration table; a missing declaration counts as nullable.
A conditional requires both value parts; a simple assignment requires its value part.
Every other form counts as nullable, including casts, generic calls, array elements, and inferred local reads.
Boolean-tail and catch-body summaries use the same classification.

Rule 3b saves the facts that rule 3 retains at each loop head.
After the final body check, one additional HIR walk derives nullable stores from the checked value types.
Each generic instance runs this check, including an instance that the final pass creates.
The condition, its synthetic prefix, and the for update receive the same check.
Nested body summaries exclude shadowed bindings; a for-of binding excludes its outer namesake from the retained facts.
The check compares independently derived checked stores and retained head facts.
Its S011 site uses `NullableMember`, classified `TscRejects`.
The diagnostic uses the display spelling of the path:

```text
`<path>` may be null at the loop head: this store can set it to null
note: the loop keeps the null check of `<path>` made before the loop
```

The HIR carries no provenance that identifies reads narrowed by retained facts.
No read-provenance record is synthesized for the check.
No measured program reaches rule 3b; the unit test is its only witness.
The unit test builds a nullable-store body against a retained fact; its control builds a non-null-store body.

A lambda keeps whole-value facts for captured const locals, with no field-path or mutable-local fact.
The lambda restores the enclosing flow state after its body check.

`record_loop_edge` shares the live fact and note snapshots, then visits only the scopes that the edge exits.
It restores each exited scope's hidden facts and applies disposal effects.
It calls `retain` only when an exited binding or disposal removes a live fact.
It clones no vector of all scopes' hidden sets.
The exit join intersects fact sets directly when no exit has a C17 note.
Equal exit fact sets keep their shared snapshot.

## Store order form

The summary lacks store order; §162.4 item 1 records rejection of a nullable store followed by a non-null store.

## Field receiver form

The summary lacks exact field-store receivers; §162.4 item 2 limits non-null preservation to whole locals.

## Inferred value form

Rule 3a excludes inferred non-null values; §162.4 item 3 records accepted TypeScript forms that this checker conservatively rejects.

## Probe evidence

TypeScript 5.9.2 checks each probe separately with the repository configuration and prelude.
The four probe sets contain 153 programs.
The checker accepts 39; TypeScript accepts all 39, and every accepted program exits 0 under `subscript run`.
The checker rejects 114; TypeScript accepts 25 of these and rejects 89.
These 25 rejections are missing facts under §162.2.
The checker accepts no program that TypeScript rejects.

| Set | Probes | Checker accepts | Accepted by TypeScript | Dev JIT exits 0 |
| --- | ---: | ---: | ---: | ---: |
| q162 | 42 | 7 | 7 | 7 |
| q162b | 43 | 17 | 17 | 17 |
| p162 | 46 | 14 | 14 | 14 |
| r162 | 22 | 1 | 1 | 1 |

The following table accounts for every probe. Names in a row have the same measured result.

| Set | Probes | Checker codes | TypeScript codes | Dev JIT |
| --- | --- | --- | --- | --- |
| q162 | `e01.ts`, `e05.ts`, `e06.ts`, `e07.ts`, `e10.ts`, `e11.ts`, `e12.ts`, `e13.ts`, `e14.ts`, `e16.ts`, `e20.ts`, `e21.ts`, `e22.ts`, `l01.ts`, `l02.ts`, `l03.ts`, `l04.ts`, `l04b.ts`, `l05.ts`, `l09.ts`, `q01.ts`, `q02.ts`, `q04.ts`, `q05.ts`, `q06.ts`, `q07.ts` | [S011] | TS18047 | Not run |
| q162 | `e02.ts`, `e03.ts`, `e04.ts`, `e15.ts`, `e17.ts`, `e19.ts`, `l03c.ts` | Accepts | Accepts | Exit 0 |
| q162 | `e08.ts`, `e09.ts`, `e09b.ts` | [S100] | Accepts | Not run |
| q162 | `e18.ts` | [S010] | Accepts | Not run |
| q162 | `l06.ts`, `l06b.ts` | [S009] | Accepts | Not run |
| q162 | `l07.ts`, `l08.ts` | [S011] | Accepts | Not run |
| q162 | `q03.ts` | [S011] | TS2531 | Not run |
| q162b | `c01.ts`, `c03.ts`, `c04.ts`, `c06.ts`, `c07.ts`, `c08.ts`, `c09.ts`, `f03.ts`, `f04.ts`, `f06.ts`, `f07.ts`, `f16.ts`, `f18.ts`, `n01.ts`, `n02.ts`, `n04.ts`, `n06.ts`, `n09.ts`, `n13.ts`, `n14.ts`, `n15.ts` | [S011] | TS18047 | Not run |
| q162b | `c02.ts`, `c05.ts` | [S011] | Accepts | Not run |
| q162b | `f01.ts`, `f05.ts`, `f08.ts`, `f09.ts`, `f10.ts`, `f11.ts`, `f14.ts`, `f15.ts`, `f17.ts`, `f19.ts`, `n03.ts`, `n05.ts`, `n07.ts`, `n08.ts`, `n10.ts`, `n11.ts`, `n12.ts` | Accepts | Accepts | Exit 0 |
| q162b | `f02.ts`, `f12.ts` | [S009], [S011] | Accepts | Not run |
| q162b | `f13.ts` | [S009], [S011] | TS18047 | Not run |
| p162 | `c1.ts`, `c2.ts`, `c3.ts`, `c7.ts`, `c8.ts` | [S011] | TS2531 | Not run |
| p162 | `c10.ts`, `c5.ts`, `c6.ts`, `p01.ts`, `p02.ts`, `p03.ts`, `p04.ts`, `p05.ts`, `p08.ts`, `p4.ts`, `r2n.ts`, `r4n.ts` | [S011] | TS18047 | Not run |
| p162 | `c4.ts`, `r6.ts`, `r6b.ts`, `s3.ts`, `s4.ts`, `s5.ts` | [S011] | Accepts | Not run |
| p162 | `c9.ts`, `ct.ts`, `lab.ts`, `p3.ts`, `p5.ts`, `r2.ts`, `r3.ts`, `r4.ts`, `r5.ts`, `r5b.ts`, `s1.ts`, `s6.ts`, `s7.ts`, `s8.ts` | Accepts | Accepts | Exit 0 |
| p162 | `ct2.ts` | [S100] | TS18046 | Not run |
| p162 | `p06.ts`, `p07.ts` | [S011], [S100] | TS18047 | Not run |
| p162 | `p09.ts`, `p10.ts` | [S011], [S100] | Accepts | Not run |
| p162 | `p1.ts`, `s2.ts` | [S009], [S011] | Accepts | Not run |
| p162 | `p2.ts` | [S009] | Accepts | Not run |
| p162 | `r1.ts` | [S100] | Accepts | Not run |
| r162 | `g01.ts`, `g02.ts`, `g03.ts`, `g04.ts`, `g06.ts`, `g08.ts`, `g09.ts`, `g10.ts`, `g11.ts`, `g13.ts`, `g14.ts`, `h05.ts`, `h06.ts` | [S011] | TS18047 | Not run |
| r162 | `g05.ts`, `g12.ts`, `g15.ts`, `h02.ts`, `h03.ts`, `h04.ts` | [S011], [S100] | TS18047 | Not run |
| r162 | `g07.ts` | Accepts | Accepts | Exit 0 |
| r162 | `g16.ts` | [S011] | Accepts | Not run |
| r162 | `h01.ts` | [S100] | TS18047 | Not run |

`r372` reports one S011 at 16:18; TypeScript reports TS18047 at 16:18.
The site is `NullableMember`, classified `TscRejects`, with no C17 block.
The code-pin binary also rejects this entry; it supplies a regression test and no new Red evidence at that pin.
`r370` and `r371` each report S011 at 12:47; TypeScript reports TS18047 at 12:47.

The unit tests pair M1, M2, M3, and explicit generic-call shapes with accepted controls.
They include contextual and default-typed lambda parameters, generic source fields, and parameters whose source type is `T`.

| Additional runtime control | TypeScript | Dev JIT output | Exit |
| --- | --- | --- | ---: |
| generic-explicit-written | Accepts | 2 lines of `1` | 0 |
| generic-field-written | Accepts | 2 lines of `1` | 0 |
| generic-parameter-written | Accepts | 2 lines of `1` | 0 |
| lambda-default-written | Accepts | 4 lines of `1` | 0 |
| lambda-written | Accepts | 4 lines of `1` | 0 |

The full golden differential suite includes `a333` and passes without an existing golden change.
`subscript run corpus/accept/a333-narrowing-flow.ts` matches its golden byte for byte.

## Paired release timings

The pin binary uses `888b68d0`; the after binary uses the final working tree.
Python `time.perf_counter()` measures each full CLI process; each table cell gives the minimum of three measured checks.

| Form | n | Pin seconds | After seconds | Ratio |
| --- | ---: | ---: | ---: | ---: |
| distinct | 1,000 | 0.032175 | 0.031667 | 0.984219 |
| distinct | 2,000 | 0.059895 | 0.060061 | 1.002770 |
| distinct | 4,000 | 0.115208 | 0.115778 | 1.004949 |
| narrowed | 1,000 | 0.052269 | 0.052133 | 0.997404 |
| narrowed | 2,000 | 0.099641 | 0.100521 | 1.008832 |
| narrowed | 4,000 | 0.197503 | 0.197961 | 1.002322 |
| blocks | 1,000 | 0.382050 | 0.383693 | 1.004301 |
| blocks | 2,000 | 0.794243 | 0.788006 | 0.992147 |
| blocks | 3,000 | 1.210936 | 1.202015 | 0.992633 |
| blocks | 4,000 | 1.661175 | 1.648652 | 0.992462 |
| same | 1,000 | 0.026441 | 0.026652 | 1.007963 |
| same | 2,000 | 0.049135 | 0.049348 | 1.004326 |
| same | 4,000 | 0.093890 | 0.093915 | 1.000262 |
| generic | 1,000 | 0.012958 | 0.013027 | 1.005360 |
| generic | 2,000 | 0.021417 | 0.021699 | 1.013169 |
| generic | 4,000 | 0.038611 | 0.038721 | 1.002852 |
| generic_i32 | 1,000 | 0.011049 | 0.010876 | 0.984380 |
| generic_i32 | 2,000 | 0.018766 | 0.018728 | 0.997979 |
| generic_i32 | 4,000 | 0.033080 | 0.032931 | 0.995487 |
| loop_edges | 1,000 | 1.952339 | 1.957161 | 1.002470 |
| loop_edges | 2,000 | 7.799556 | 7.759253 | 0.994833 |
| loop_edges | 4,000 | 31.057960 | 30.475034 | 0.981231 |

Acceptance 5 passes: the largest ratio is 1.013169 (`generic:2000`).

The `loop_edges` times are about 1.95 / 7.8 / 31 seconds at n = 1,000 / 2,000 / 4,000.
The cost is about 2 µs per live fact per loop, equal at the code pin.

With one fact, the review measures nesting costs of 0.25 / 0.75 / 1.70 seconds at depth 200 / 300 / 400.
Rule 3b adds one body walk per enclosing loop: O(size × depth).

## Gate

`tools/gate.sh full`: `debug 2348/0/3 release 2345/0/3 skips 2/0 clippy 5/18/13 goldens-moved 0 exit 0`.
