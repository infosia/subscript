<!-- §27 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 27. R5 — scalar array-pairs at parameter position

Owner decision 2026-08-01 (downstream request R5, blocking its
buffers+queue area). Diagnosis first, because the request's own
guess was wrong in a useful way: the bindgen scalar table
(`lang_scalar`) already maps the full `stdint.h` set — `uint8_t` was
never unmapped as a scalar. The failing site is **pointer-to-scalar
at a boundary position**, which routes to the named-type registry
and correctly fails loud. What is missing is a rule for the shape
the downstream facade needs: a count-first scalar pair as two
adjacent **function parameters**.

### 27.1 The rule

The struct-level `<name>Count` / `<name>` adjacency rule (§13.2)
extends to parameter lists. Two adjacent parameters
`size_t <name>Count, [const] S* <name>` — where `S` is any
`lang_scalar` scalar — mirror as one parameter `<name>: S[]`:

- **`const S*` (script → C).** The existing input-direction pair
  semantics: the language array's `(ptr, len)` cross zero-copy; the
  callee reads `<name>Count` elements.
- **`S*` (C fills, R5.2).** The count crossed is the script array's
  **length**; the callee writes up to that count in place and never
  grows the array; the writes are visible to the script after the
  call, byte-identically in both tiers. This is §14.3's out-array
  direction applied to scalar elements at parameter position.

Non-adjacent count/pointer parameters, non-scalar pointer
parameters, and pointer returns keep today's fail-loud rejection —
the rule is the adjacency spelling, not a general pointer surface.
Struct-level scalar pairs (registered aggregates) are unchanged.

### 27.2 Corpus and fixture

The interop fixture gains one function per direction — a
deterministic byte consumer (`const uint8_t*`: e.g. summing) and a
deterministic filler (`uint8_t*`: a fixed pattern), plus a `u16`
variant of one direction (the downstream index-data case) — and the
mirror regenerates through `subscript bind`. `a96` (accept): script
builds a `u8[]`, passes it to the consumer, prints the result;
passes an array to the filler and prints the elements after the
call (the visibility pin); exercises the `u16` variant. Bindgen unit
tests pin the parameter-pair mapping for both directions and that a
non-adjacent pair still fails loud.

### 27.3 Exit criteria (pre-registered)

1. `a96` byte-identical under both tiers, including the
   after-the-call element prints for the filled array.
2. Bindgen tests: `(size_t xCount, const uint8_t* x)` and
   `(size_t xCount, uint8_t* x)` parameters mirror as `x: u8[]`; a
   `u16` variant likewise; a lone scalar-pointer parameter and a
   non-adjacent pair keep the fail-loud error.
3. The stdint audit is answered by the existing `lang_scalar` table
   plus a test asserting every stdint spelling it names maps at a
   pair site (one table, as the request hoped).
4. No existing golden moves; full gate, `tsc` gate, zero-warning
   sweep green.
