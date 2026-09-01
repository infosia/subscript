// example: e01-sized-integers
// teaches: Use sized integers and floats, convert explicitly, and observe fixed-width wrapping.
// differs-from-typescript: C3 rejects bare number and makes as a numeric conversion; C4 types literals from context.
// see: corpus/accept/a02-integer-types.ts, corpus/accept/a03-integer-literals.ts, corpus/accept/a22-matrix-propagation.ts, corpus/reject/r08-bare-number.ts, corpus/reject/r09-int-literal-overflow.ts, collisions.md C3-C4, compiler.md §7

// The numeric vocabulary of the language. Each name states a width, and the
// compiler holds that width through every later expression.
// C3: the compiler preserves these declared widths even though tsc sees
// every alias as number. Rejected alternative: bare number is S007;
// diagnostic excerpt: "bare `number` is rejected; there is no default
// numeric type — use a sized type"; corpus/reject/r08-bare-number.ts pins it.
const signed: i32 = -12;
const unsigned: u32 = 20;
const wide: i64 = 9000000000;
const single: f32 = 1.5;
const double: f64 = 2.25;

// The largest u32 value. The program needs it to make the wrap below visible.
// C4: suffix-less literals adopt the annotated sized type. Rejected
// alternative: `const tooLarge: i32 = 3000000000` is S008, "integer
// literal 3000000000 out of range for `i32`";
// corpus/reject/r09-int-literal-overflow.ts pins it.
const maximum: u32 = 4294967295;

// Q12: this zero-argument void export is a host-callable script entry.
export function main(): void {
  // Mixed widths do not combine on their own. Each line below names its
  // target width, so the program states every conversion it wants.
  // C3: as performs an explicit numeric conversion rather than tsc's
  // type-only assertion.
  const integerSum: i32 = signed + (unsigned as i32);
  const realSum: f64 = (single as f64) + double;
  const truncated: i32 = realSum as i32;

  // C3 and compiler.md §7: u32 arithmetic wraps at 32 bits, so max + 2 is 1.
  const wrapped: u32 = maximum + 2;

  // The output is the proof. converted=8,3.75,3 shows the u32 to i32
  // conversion, the f32 to f64 widening, and the truncation of 3.75 to 3.
  print(`integers=${signed},${unsigned},${wide}`);
  print(`floats=${single},${double}`);
  print(`converted=${integerSum},${realSum},${truncated}`);
  print(`wrapped=${wrapped}`);
}
