<!-- §136 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 136. A mirror constant carries its value

*(Added 2026-09-30.)* Origin: found during §125 (the second Phase
Review; `specs/tracking/s125-global-names.md`, "Open").

Problem: the checker accepts a mirror variable declaration with a type
annotation and no value, and lowering has no storage for it. Measured
at `8a1959f1`:

- `declare const K: i32;` in a mirror, and
  ``export function main(): void { print(`${K}`); }``:
  `subscript check main.ts --mirror m.d.ts` gives "no errors";
  `subscript emit` exits 1 with "internal error: LIR construction
  failed: main.ts:1:41: unknown global `K`".
- `declare let L: i32;` in a mirror, and `L = 3;` in `main`: S100
  "cannot rebind `const` binding `L`". The checker reads every mirror
  variable as a constant.
- The same `declare const K: i32;` in program source: S100
  "module-level variables require an initializer".

A mirror constant exists to carry a C value into the program: `bind`
folds each C `static const` member into `declare const X = <value>;`
(Q13, Q18). A typed declaration with no value names host data. The
language binds host functions through the C ABI; it binds no host data
symbol, and no downstream program or corpus entry needs one.

*(Added 2026-09-30 after the Phase Review.)* `bind` does not always
emit that form. It reads each constant as a signed 64-bit value and
prints it in decimal, and it emits the members of every scalar alias,
signed or unsigned. Measured with
`typedef uint64_t F; static const F F_ALL = ~(uint64_t)0;` and
`typedef int32_t G; static const G G_NEG = -1;`: `bind` emits
`declare const F_ALL = -1;` and `declare const G_NEG = -1;`, and the
checker rejects both (at `8a1959f1` with "ambient constants require a
type annotation or an integer-literal initializer"). The checker types
every mirror constant `u64` (Q18: a flag set is an unsigned alias), so
a signed value has no correct reading.

### 136.1 Rules

1. A mirror variable declaration is in the surface only in the form
   `declare const X = <non-negative integer literal>;`, the form `bind`
   emits.
1a. `bind` emits `static const` members only for an alias whose scalar
   is an unsigned integer (`u8`, `u16`, `u32`, `u64`), and prints each
   value as the unsigned value of that width: `~(uint64_t)0` is
   `18446744073709551615`. A `static const` member of any other scalar
   alias fails `bind` with a message that names the constant and its
   alias, and no mirror is written (as `cli.md` §10.1 fails for any other
   construct outside the mirror surface).
2. Every other mirror variable declaration is S100 at the declared
   name, with a message that names the one accepted form: a `const`
   with a type annotation and no value, a `let`, and a `var`. It is
   reported once per declared name, and a use of the name reports
   nothing more. A second declaration of one name also gives the
   existing S017; the two are separate faults.
3. The checker never passes a mirror global with no value to lowering.
   Lowering needs no "unknown global" path for a mirror name.
4. Program source does not change (`declare const` there stays the
   existing S100).

### 136.2 Acceptance

1. Red first: a unit test (the mirror reject form of
   `compiler/tests/corpus_reject.rs` r29, or `interop_mirror.rs`) with
   `declare const K: i32;` in a mirror and a read of `K` in `main`,
   which checks clean at `8a1959f1`; the round records the Red output.
2. Unit tests: rule 2 for each form (`const` with a type, `let`,
   `var`), each exactly one diagnostic at the declared name, also with
   a use; control: `declare const X = 1;` in the same mirror is
   accepted and runs.
3. Tests that use a typed mirror constant as a fixture move to the
   rule 1 form, with the same assertion; the codegen test that pins
   the lowering error becomes the rule 2 test or is removed.
4. Every committed mirror stays byte-identical. No `.expected` golden
   moves.
5. Tests for rule 1a: an unsigned 64-bit all-bits constant is emitted
   as its unsigned value, checks, and runs (the value prints
   `18446744073709551615` on every engine); a signed alias constant
   fails `bind` with the named message; the byte-identical
   regeneration test still passes.
