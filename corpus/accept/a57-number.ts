// corpus: accept/a57-number
// purpose: Exercises Q25 Number constants and non-coercing predicates.
// exercises: number-constants, number-predicates
// questions: Q14, Q25

export function main(): void {
  // MAX_SAFE_INTEGER is the f64 precision bound, not an i64/u64 bound:
  // the language's 64-bit integer types remain exact (C3).
  print(`safe constants ${Number.MAX_SAFE_INTEGER} ${Number.MIN_SAFE_INTEGER}`);
  print(`float constants ${Number.EPSILON} ${Number.MAX_VALUE} ${Number.MIN_VALUE}`);
  print(`infinities ${Number.POSITIVE_INFINITY} ${Number.NEGATIVE_INFINITY} ${Number.NaN}`);

  print(`isNaN ${Number.isNaN(Number.NaN)} ${Number.isNaN(0.0)}`);
  print(`isFinite ${Number.isFinite(Number.MAX_VALUE)} ${Number.isFinite(Number.POSITIVE_INFINITY)}`);
  print(`isInteger ${Number.isInteger(-0.0)} ${Number.isInteger(7.0)} ${Number.isInteger(7.5)} ${Number.isInteger(Number.NaN)}`);
  print(`isSafeInteger ${Number.isSafeInteger(Number.MAX_SAFE_INTEGER)} ${Number.isSafeInteger(9007199254740992.0)} ${Number.isSafeInteger(1.5)}`);
}
