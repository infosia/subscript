// corpus: reject/r202-array-spread-generator
// purpose: Rejects a generator operand in an array-literal spread.
// exercises: array-spread, generator, single-use
// questions: Q30, compiler section 103
// tsc: accepts
// expected-error: S014 naming the single-use generator rule
function* one(): Generator<i32> {
  yield 1;
}
export function main(): void {
  const values: i32[] = [...one()];
  print(`${values.length}`);
}
