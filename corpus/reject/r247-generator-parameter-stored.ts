// corpus: reject/r247-generator-parameter-stored
// purpose: Reject a capture at an escape boundary (compiler.md §118).
// exercises: capture, escape
// questions: Q10
// tsc: accepts
// expected-error: S009 at escape boundary
function* gen(cb: () => i32): Generator<i32> { yield cb(); }
function store(values: Generator<i32>[], value: Generator<i32>): void {
  values.push(value);
}
export function main(): void {
  const value: i32 = 5;
  const values: Generator<i32>[] = [];
  store(values, gen((): i32 => value));
}
