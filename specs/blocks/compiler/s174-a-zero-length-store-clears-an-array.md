<!-- §174 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 174. A zero-length store clears an array

*(Added 2026-10-07.)* Origin: `compiler.md` §154.3, the realistic
`TscRejects` instance 3. On 2026-10-06 the owner selected the §154.3
realistic instances; the §173 measurement round at `ae165007`
(`specs/tracking/s173-tsc-rejects-instances.md` item 3) found that this
one needs a lowering.

Problem: `xs.length = 0` is the common JavaScript form that empties an
array in place. Stock `tsc` 5.9.2 accepts it, and node prints `0` for
the length after it. This checker rejects it with S100 at
`ArrayUnknownMember`, class `TscRejects`, with no block, and the
message lists `length` as an accepted member. Any other length store
(`xs.length = n`) can grow the array, and JavaScript fills the new
slots with holes; this language has no hole value.

### 174.1 Rules

1. **The zero store.** An assignment statement `xs.length = 0` (the
   integer literal `0` as the right-hand side, on a dynamic array) is
   accepted. It evaluates `xs` once, removes every element, and sets
   the length to zero. The storage capacity is not specified.
2. **Counts.** On a counted array (§171 rule 1), each removed element is
   released, as a discarded removal releases it (§171 rule 5), in index
   order. A release that frees a frame with an unobserved exception
   traps (§171 rule 10), and the LIR instruction carries its count
   action and its trap site (§171 rules 9 and 10). The removal keeps
   the holder count of the array.
3. **The expression value.** The assignment as an expression
   (`const n = (xs.length = 0)`) and a compound form (`xs.length -= 1`)
   stay rejected at rule 4's site.
4. **Every other length store** is rejected at a new `Diverges` site,
   `ArrayLengthStore`, split from `ArrayUnknownMember` on the facts
   "the member is `length`" and "the access is a store". A new
   `stdlib.md` §9.12 row states the reason: a store of a nonzero length
   can grow the array, and the language has no hole value; `splice`
   and `pop` remove elements. The message states that only
   `xs.length = 0` is accepted. A `FixedArray` length store stays at
   its own site.
5. The three tiers (dev JIT, C AOT, interpreter) give the same output,
   the same counts, and the same trap.

### 174.2 Acceptance

1. Red first, at the contract pin: accept entry `a344` (a zero store on
   an `i32[]`, a `string[]`, a reference array, and a handle array that
   holds completed tasks, each followed by a `push` and a read of the
   new element and the length; a zero store through a function
   parameter, so that the caller sees an empty array), `js-comparable:
   yes` if node agrees; trap entry `t102` (a zero store on a handle
   array that holds a failed unobserved task traps at the store);
   reject entry `r395` (`xs.length = 2`, `xs.length -= 1`, and the
   zero store as an expression).
2. Unit tests: a three-tier test of the counts after a zero store on
   each counted shape (handle, handle array, nested array) with a
   same-shape control that pops each element; the §154 total test with
   the new site and its witnesses.
3. The LIR text golden moves for the new entries. No other `.expected`
   golden moves.

### 174.3 Open

The Phase Review found these. None is CRITICAL or MAJOR.

1. `compiler/tests/operation_signatures.rs` carries an inline copy of
   `compiler/tests/support/lifetime_sites.rs` with the `ArrayClear`
   arm; the support file is now unused and has no such arm.
2. `codegen/tests/zero_length_store.rs` runs `a344` in the three tiers
   again (the corpus differential test already does), and its other two
   tests build C with no stated cost (core principle 15).
3. The interpreter trap test checks only `is_err()`, with no trap kind
   or position, for the `pop` control too.
4. `f.length = 0` on a `FixedArray` and `s.length = 0` on a `string`
   give a message that lists `length` as accepted (`tsc` also rejects
   both stores).
5. `for (xs.length = 0; ...)` is rejected as not a statement; `tsc`
   accepts it (rule 1 names the statement form).
6. The first `unsafe` call in `codegen/src/interpreter/counted.rs`
   (near 24) has no `// SAFETY:` comment.
7. A rejected length store does not check its right-hand side, so an
   error there is reported in a later run (contrived).
