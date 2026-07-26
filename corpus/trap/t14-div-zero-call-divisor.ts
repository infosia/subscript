// corpus: trap/t14-div-zero-call-divisor
// purpose: Evaluates a call-valued divisor once before division-by-zero traps.
// exercises: integer-division, call-valued-divisor, single-evaluation
// questions: none
// expected-trap: division-by-zero after one divisor call

function divisor(): i32 {
  print("divisor called");
  return 0;
}

export function main(): void {
  print("before division");
  const quotient: i32 = 84 / divisor();
  print(`${quotient}`);
}
