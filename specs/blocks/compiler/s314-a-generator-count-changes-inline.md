<!-- §314 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 314. A generator count changes inline

*(Added 2026-11-16.)* Origin: §255.4 items 1 and 2, §256.4 item 1. The
owner selected one section for the counted-value path on 2026-11-16.

Problem, measured at `bdfb6ec1` on the ship tier (median of 5 runs, a
loop of 10^7 calls). A retain or a release of a value whose static type
is `Generator<…>` is a call of `subscript_rt_counted_value`. The
runtime reads the description, then `async_retain` or `async_release`
looks up the frame in the async frame table and in the object
description table before it changes the holder count at
`generator_layout::HOLDERS_OFFSET`. One retain and release pair costs
19 to 21 ns:

| Form | With the pair | Without | Each call |
|---|---|---|---|
| `for (const h of hs) s += use2(h, pick(g))` | 1106.7 ms | 917.5 ms | 18.9 ns |
| `const f = (): i32 => use2(h, pick(g))` | 927.9 ms | 726.7 ms | 20.1 ns |
| `take(a: Generator<i32>, k = 1)` called as `take(pick(g))` | 907.2 ms | 697.2 ms | 21.0 ns |

With an async handle, the same pair is the inline count change of
§116.2 rule 6, and its cost is below noise (0.2 ns for each call). Each
call of `pick(x: Generator<i32>): Generator<i32> { return x; }` also
does one pair. So each counted operation on a generator costs the
lookups, not only the extra pairs of §255.4 and §256.4.

### 314.1 Rules

1. Where the static type is `Generator<…>`, the dev JIT and the ship C
   retain inline: a null test, then a saturating increment of the `u32`
   at `HOLDERS_OFFSET`. A release decrements inline when the count is
   above one, and calls the runtime otherwise. The runtime keeps the
   free, the holder release, and each check at one and at zero, as in
   §116.2 rule 6.
2. Rule 1 holds only if each non-null value of static type
   `Generator<…>` is a frame that the runtime registered, while the
   value is live. This includes a value after a hot reload. If a
   program can hold a non-null generator value that the runtime did
   not register, rule 1 does not apply to it. The round reports the
   case and stops.
2a. The C ABI has no generator. A mirror signature that names
   `Generator<…>`, in a parameter, a result, or a type that a mirror
   declaration holds, is rejected with a diagnostic that names the
   cause. The public header gives a host no function that makes, resumes,
   or counts a generator, so a host can only give a generator if it
   copies the internal frame layout. *(Added 2026-11-16: round 1 of
   §314 stopped under rule 2. A mirror `declare function
   hostGenerator(): Generator<i32>;`, with a host that builds a frame
   from the internal layout, gives a live generator that the runtime
   did not register. The checker accepted that mirror. No mirror in the
   repository names a generator.)* With rule 2a, each generator value
   comes from a script generator function, which registers its frame
   before it returns, in both tiers.
2b. Rule 2 covers each value that a host can give through the public
   C header (invariant 4). A host that writes the internal layout of a
   script object or of a frame is outside the C ABI, and rule 2 does
   not cover what it gives. §116.2 rule 6 has the same scope for an
   async handle. *(Added 2026-11-16: round 2 stopped under rule 2. A
   mirror `hostBox(): object`, cast to a script class with a generator
   field, gave an unregistered generator, because the host built both
   the object and the frame from the internal layouts. An object that
   a script made and gave to the host, and that the host gives back,
   holds a registered generator.)*
3. A counted value that holds a generator inside another value (an
   array element, a fixed array element, an iterator result) keeps the
   description path. Only a value whose own static type is a generator
   changes.
4. The output of each program does not change. The dev JIT, the ship
   C, the interpreter, and the golden agree on each corpus entry.
5. The interpreter keeps its own path. It is the reference for rules 1
   and 4.

### 314.2 Acceptance

1. Red first: a test of the emitted C for a function that retains and
   releases a `Generator<i32>` value finds no call of
   `subscript_rt_counted_value` (as `codegen/tests/async_cost_emission.rs`
   does for an async handle). It fails at the pin. A test of the JIT
   lowering states the same fact for the dev tier.
2. The three forms of the table, and `pick`, are measured again on the
   ship tier. `specs/tracking/s314-generator-count.md` records the time at the pin and after
   the change, with the method of the measurement at `bdfb6ec1`
   (median of 5 runs, one at a time).
3. The runtime release at one and at zero is tested: a generator that a
   program drops at its last holder is freed, and a generator that two
   holders hold is freed after the second release only.
4. The full gate passes; no golden moves.
5. Rule 2a: a reject corpus entry with a mirror that returns
   `Generator<i32>`, and a test for a parameter and for a declaration
   that holds one. Each is Red at the pin. The entry header states what
   `tsc` does, measured; the divergence and its example follow §307
   and §311.

### 314.3 Phase Review

*(Filled in after the review.)*

### 314.4 Open

1. A container of async handles retains through
   `subscript_rt_async_retain_array`: §255.4 item 3 measured 4.3 ns for
   each assignment. A container of generators keeps the description
   path (rule 3).
