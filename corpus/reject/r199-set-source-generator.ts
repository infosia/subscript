// corpus: reject/r199-set-source-generator
// purpose: Rejects a Generator as the source of a Set construction.
// exercises: set-source-construction, generator, single-use
// questions: Q30, compiler section 103
// tsc: accepts
// expected-error: S014 naming the single-use generator rule
function* one(): Generator<i32> {
  yield 1;
}
export function main(): void {
  const values: Set<i32> = new Set<i32>(one());
  print(`${values.size}`);
}
