// corpus: accept/a63-q27-math-number
// purpose: Exercises Q27 Stage 1: wrapping Math.imul, Math.fround's
//          binary32 rounding, and Number parser aliases agreeing with
//          the accepted globals on identical inputs.
// exercises: math-imul, math-fround, number-parse-int, number-parse-float
// questions: Q27
// tsc: accepts; js-comparable: yes
export function main(): void {
  const high: i32 = 2147483647;
  const low: i32 = -2147483648;
  print(`imul ${Math.imul(high, 2)} ${Math.imul(low, -1)}`);
  print(`fround ${Math.fround(1.1)}`);

  const globalInt: f64 = parseInt("7ftail", 16);
  const staticInt: f64 = Number.parseInt("7ftail", 16);
  print(`parseInt ${globalInt} ${staticInt} ${globalInt === staticInt}`);

  const globalFloat: f64 = parseFloat("  -1.25e2tail");
  const staticFloat: f64 = Number.parseFloat("  -1.25e2tail");
  print(`parseFloat ${globalFloat} ${staticFloat} ${globalFloat === staticFloat}`);
}
