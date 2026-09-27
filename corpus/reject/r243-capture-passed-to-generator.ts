// corpus: reject/r243-capture-passed-to-generator
// purpose: Reject a coroutine capture escape (compiler.md §118).
// exercises: coroutine, escape
// questions: Q10
// tsc: accepts
// expected-error: S009 at escape boundary
function* gen(cb: () => i32): Generator<i32> {
  yield cb();
  yield cb();
}
function make(): Generator<i32> {
  const value: i32 = 5;
  return gen((): i32 => value);
}
export function main(): void {
  for (const value of make()) { print(`${value}`); }
}
