// corpus: reject/r207-array-from-generator
// purpose: Rejects a generator as an Array.from source.
// exercises: array-from, generator, single-use
// questions: Q22, compiler section 105
// tsc: accepts
// expected-error: S014 naming the single-use generator rule
function* one(): Generator<i32> {
  yield 1;
}
export function main(): void {
  const values = Array.from(one());
  print(`${values.length}`);
}
