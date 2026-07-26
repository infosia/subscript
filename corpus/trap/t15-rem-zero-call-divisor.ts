// corpus: trap/t15-rem-zero-call-divisor
// purpose: Evaluates a call-valued divisor once before remainder-by-zero traps.
// exercises: integer-remainder, call-valued-divisor, single-evaluation
// questions: none
// expected-trap: division-by-zero after one divisor call

function divisor(): i32 {
  print("divisor called");
  return 0;
}

export function main(): void {
  print("before remainder");
  const remainder: i32 = 84 % divisor();
  print(`${remainder}`);
}
