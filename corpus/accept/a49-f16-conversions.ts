// corpus: accept/a49-f16-conversions
// purpose: Pins IEEE binary16 narrowing and widening at representable, rounded, overflow, subnormal, NaN, and signed-zero cases.
// exercises: f16-storage, explicit-conversion, ieee-binary16, q14-formatting
// questions: Q14, Q23, C3, C4
// tsc: accepts; js-comparable: no C3 Q14 Q23: Binary16 conversion and negative-zero formatting produce different output.
export function main(): void {
  const representable: f16 = (1.5 as f64) as f16;
  const rounded: f16 = (1.0006 as f64) as f16;
  const overflow: f16 = (70000.0 as f64) as f16;
  const subnormal: f16 = (0.000000059604644775390625 as f64) as f16;
  const nan: f16 = Math.sqrt(-1) as f16;
  const negativeZero: f16 = (-0.0 as f64) as f16;

  print(`representable ${representable as f64}`);
  print(`rounded ${rounded as f64}`);
  print(`overflow ${overflow as f64}`);
  print(`subnormal ${subnormal as f64}`);
  print(`nan ${nan as f64}`);
  print(`negative-zero ${negativeZero as f64}`);
}
