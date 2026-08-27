// corpus: accept/a73-p19-divisor-single-eval
// purpose: Evaluates a call-valued integer divisor exactly once.
// exercises: integer-division, call-valued-divisor, single-evaluation, Math.random
// questions: none
export function main(): void {
  const q: i32 = 100 / ((Math.random() * 3.0 + 1.0) as i32);
  print(`q ${q}`);
  print(`next ${Math.random()}`);
}
// tsc: accepts
