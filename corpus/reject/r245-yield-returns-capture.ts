// corpus: reject/r245-yield-returns-capture
// purpose: Reject a coroutine capture escape (compiler.md §118).
// exercises: coroutine, escape
// questions: Q10
// tsc: accepts
// expected-error: S009 at escape boundary
function* gen(): Generator<() => i32> {
  const value: i32 = 5;
  yield (): i32 => value;
}
export function main(): void {
  for (const callback of gen()) { print(`${callback()}`); }
}
