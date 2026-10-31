# A nullable array and a nullable string — `compiler.md` §217

Status: contract.

## Step 0 at `14a143f1`

A prototype added `Array` and `Str` to `HandleKind::supports_nullable`.
It contradicted `collisions.md` C7. It ran 180 programs on the three
tiers, `tsc`, and `node`, and it is reverted.

`Map<K, V> | null` and `Set<T> | null` were accepted at the pin. No record
gives a reason for the omission of `Array` and `Str`.

Array, table change only:

| Form | Result |
|---|---|
| parameter, result, local, field, element, `Map` value, global, generic argument | equal to `node` on the three tiers |
| `=== null`, `== null`, narrowing, `??`, `??=`, `?.` member and method | equal to `node` |
| `.length`, `[0]`, a method, `for…of` on a nullable | rejected, as `tsc` TS18047 does |
| `?.[i]`, `x!` | rejected, as for a class |
| `Context.collect()` with a null in a field, an element, a `Map` value, across `await` and `yield` | equal to `node` |
| `const j: Promise<void>[] \| null = [ok()];` | internal error: "fresh counted owner 1 is not consumed" |
| an alias of `Promise<void>[]` in a nullable local | use-after-delete on two tiers, an internal trap on the third |
| a mirror with `data: i32[] \| null` and no provenance | `check` accepts, and `cc` fails with a wrong ABI type |

`counted_type()` gives `None` for a nullable, so a nullable has no holder
count. That is the reason for §217.1 rule 2.

String, in addition:

| Form | Result |
|---|---|
| `(string \| null)[]`, `includes(mk())`, `indexOf(mk())` | wrong value on the three tiers: `false`, `-1`. `ArrElemKind::of` maps `Nullable` to `Int`. |
| `s === t` if the checker opens it | pointer compare. `"abc"` built two ways gives `false`. |
| `string` left, null right | interpreter stops: "null used as a string". |
| `pop`, `find`, `Map.get` on `string` | a `map_get_value_ok` row changes §196 rule 2. |

A `string` has no null zero, so the null form changes no default.
`subscript_rt_str_eq` and `valeq.rs` are null-safe. The risk is that
dispatch keys on `Type::Str`.

Other finding: two `FixedArray<X | null, N>` types with different `X` give
`typedef redefinition ... SubFA_ptr_2` on the ship tier at the pin.

`tools/gate.sh quick` on the table change: six failures, each a pin or a
total-check row. No entry reaches the wrong values above, so the corpus
needs new entries.
