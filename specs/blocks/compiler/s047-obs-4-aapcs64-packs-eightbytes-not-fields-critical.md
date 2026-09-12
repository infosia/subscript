<!-- §47 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 47. OBS-4 — AAPCS64 packs eightbytes, not fields (CRITICAL)

Owner decision 2026-08-03 (downstream observation OBS-4, accepted
as a miscompile). On `aarch64-apple-darwin` the dev tier delivers
**wrong values, silently**, for a by-value boundary struct with two
or more sub-64-bit integer fields; the ship tier is correct. The
callee's own print proves it is argument delivery, not computation:

| aggregate | dev-JIT receives | ship-C-AOT receives |
|---|---|---|
| `{ i32 }` | `a=3` | `a=3` |
| `{ i32, i32 }` | `x=3 y=0` | `x=3 y=7` |
| `{ i32, i32, i32 }` | `a=3 b=0 c=7` | `a=3 b=7 c=11` |
| `{ i64, i64 }` | `a=3 b=7` | `a=3 b=7` |
| `{ f32, f32 }` | `a=3.25 b=7.5` | `a=3.25 b=7.5` |

### 47.1 Cause, from this contract's own text

§12.3a records the AAPCS64 rule as "a ≤16-byte struct is packed
into registers (**its components as arguments**)". That is not
AAPCS64. B.4 passes a small non-HFA composite as **eightbyte
images** — the struct's bytes in consecutive general registers —
not one register per field. Passing components puts `x` in `x0`
and `y` in `x1`; the callee reads `x0` as the whole first eightbyte
and gets `y = 0`, and at three fields the original second field
arrives in the third position, which is exactly the matrix above.
`{ i64, i64 }` is unaffected because each field already is an
eightbyte, and `{ f32, f32 }` because an HFA is passed
component-wise in float registers, where the existing behavior is
correct.

Rule: **the dev tier's by-value marshaling follows the target
ABI's register-image rules, not its field list.** On AAPCS64: an
HFA/HVA goes component-wise in float registers; any other
composite of at most 16 bytes goes in consecutive general
registers as eightbyte images, with sub-eightbyte fields packed at
their C offsets; a larger one goes by reference to a caller copy.
§12.3a's "components as arguments" wording is corrected to this.
Win64's stated rule already packs the whole struct as one integer
and is unaffected; SysV remains a loud error.

### 47.2 Why nothing here found it

No fixture function takes a by-value struct with two or more
sub-64-bit integer fields — verified at the pin. Every by-value
aggregate the corpus passes is a `(pointer, count)` descriptor, a
string view, or a two-`i64`/HFA shape, all of which need no
packing. The downstream could not find it either: its facade
passes every struct by pointer. This is the third instance of one
pattern in this exchange — the corpus pinned the constructs it
knew, and the defect lived in a shape neither side's own code
happened to produce.

### 47.3 Corpus

`a126-interop-by-value-packing` (accept), Red-first: the fixture
gains by-value parameters covering `{i32}`, `{i32,i32}`,
`{i32,i32,i32}`, `{i16,i16,i32}`, four `u8`s, `{i64,i64}`, an HFA
of two and of four `f32`, a mixed `{i32,f32}`, a `{i32,i64}` with
its padding hole, and a >16-byte case that must go by reference —
each callee reporting every field it received, so a wrong delivery
is a wrong golden rather than a wrong sum. The pre-fix
observations are recorded before the fix lands.

### 47.4 Exit criteria (pre-registered)

1. The pre-fix matrix is recorded verbatim.
2. `a126` byte-identical under both tiers, every listed shape.
3. A lowering unit test asserts the register-image plan for each
   shape against the ABI rule, so the class is pinned and not only
   the instances.
4. §12.3a's corrected wording is the contract; no existing golden
   moves; full gates green.
