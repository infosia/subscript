<!-- §50 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 50. R23 — wire-mapped literal-union aliases (`CEnum`)

Owner decision 2026-08-08 (downstream request R23). Q32 (§24) barred
literal-union aliases from boundary signatures (v1), and the
downstream lowered every boundary enum to a bare integer. R23 retires
that bar for an alias that declares a wire mapping. The downstream
evidence: 45% of its generated API layer is converter functions
between union strings and mirror integers.

### 50.1 Declaration form

The prelude declares one generic alias:

    type CEnum<M extends Record<string, number>> = Extract<keyof M, string>;

`type A = CEnum<{ "m0": w0, "m1": w1 }>;` at module level declares a
Q32 alias. The members are the keys, in declaration order. Each
member carries its **wire value**: the declared integer literal.
Stock `tsc` resolves the alias to the string-literal union (measured
2026-08-08: the probe type-checks; a non-member literal fails TS2322;
a duplicate key fails TS2300).

The alias is a Q32 alias in full: §24, §41, §42, and §43 apply
unchanged. The in-language representation does not change: the
declaration-order `i32` discriminant and the per-alias string table.
The wire value is not script-visible. *(Revised 2026-08-09 by §52:
a wire-mapped alias's discriminant is the wire value itself. The
observable behavior of this section is unchanged.)*

The checker enforces three constraints beyond stock `tsc`:

- Each wire value is an integer literal in `i32` range. A fractional
  or out-of-range value is a compile error. Hex and negative
  literals are legal.
- Wire values are unique within one alias. A duplicate wire value is
  a compile error.
- The member set is non-empty.

### 50.2 Boundary positions

A wire-mapped alias is legal in a bound (mirror) signature at
parameter position and at return position. Its C type is `int32_t`.
A plain Q32 alias stays rejected in boundary signatures: the §24
rule narrows; it does not retire.

Out of scope in this slice, each parked until a downstream request:
wire-mapped aliases as boundary-struct members; emission of the form
by `subscript bind`.

### 50.3 Conversion at the crossing (both tiers)

- Parameter (script to C): the discriminant indexes a per-alias
  static table to the wire value. The wire value is passed.
- Return (C to script): the wire value maps to the discriminant. An
  unknown wire value **traps at the crossing** with a diagnostic
  that names the alias and the wire value. The trap uses the
  standing trap machinery (§18–§20) and is byte-exact across tiers.
- No string operation occurs at the crossing in either direction.

*(Revised 2026-08-09 by §52: with the wire-value discriminant, the
parameter direction is an identity pass and the table is gone; the
return direction keeps membership validation and the trap.)*

### 50.4 Corpus

The fixture is a hand-authored neutral mirror plus a committed C
callee beside the interop fixtures. `subscript bind` cannot emit the
form this slice, so this mirror is authored, not generated. Fixture
names stay synthetic (repo hygiene: no real-world API names).

- `a129-interop-wire-enum` (accept): a synthetic wire-mapped alias
  with non-dense wire values (hex, a gap, a negative). The script
  receives a value from a C return, switches on it, and passes each
  member to a C parameter that echoes the wire value. The golden
  holds the member strings and the echoed wire values, byte-exact
  under both tiers. The script contains no cast and no converter.
- `t48-wire-enum-unknown-value` (trap): a C function returns a wire
  value outside the mapping. Both tiers trap with one identical
  diagnostic.
- Reject, each `tsc`-clean (the strictly-narrower proof): `r121` a
  fractional wire value; `r122` a duplicate wire value across two
  members; `r123` a wire value outside `i32` range.

### 50.5 Exit criteria (pre-registered)

1. `a129` runs byte-identical under both tiers; the golden holds the
   member strings and the echoed wire values; the script contains no
   cast and no converter function.
2. `t48` traps with one identical diagnostic under both tiers, and
   the diagnostic names the alias and the unknown wire value.
3. `r121`–`r123` pin (code, line) and type-check under stock `tsc`.
4. A `cemit` unit test pins the crossing: parameter position lowers
   to a table access, return position lowers to a wire-to-
   discriminant mapping plus a trap path, and no string operation
   appears at the crossing.
5. Checker unit tests: a wire-mapped alias is accepted at parameter
   and return boundary positions; a plain Q32 alias stays rejected
   there; a fractional, duplicate, and out-of-range wire value are
   each rejected.
6. No existing golden moves; the bindgen regeneration gate stays
   byte-identical (bindgen untouched); full gate, `tsc` gate, and
   the zero-warning sweep are green.
