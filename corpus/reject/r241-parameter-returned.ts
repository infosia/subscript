// corpus: reject/r241-parameter-returned
// purpose: Reject a function parameter escape (compiler.md §118).
// exercises: function-parameter, escape
// questions: Q10
// tsc: accepts
// expected-error: S009 at caller argument
function forward(cb: () => i32): () => i32 {
  return cb;
}
export function main(): void {
  const value: i32 = 5;
  const callback = forward((): i32 => value);
  print(`${callback()}`);
}
