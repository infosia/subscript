// corpus: reject/r242-expression-body-returns-parameter
// purpose: Reject a function parameter escape (compiler.md §118).
// exercises: function-parameter, escape
// questions: Q10
// tsc: accepts
// expected-error: S009 at caller argument
export function main(): void {
  const forward = (cb: () => i32): (() => i32) => cb;
  const value: i32 = 5;
  const callback = forward((): i32 => value);
  print(`${callback()}`);
}
