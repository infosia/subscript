// corpus: reject/r248-capture-through-indirect-call
// purpose: Reject a capture at an escape boundary (compiler.md §118).
// exercises: capture, escape
// questions: Q10
// tsc: accepts
// expected-error: S009 at escape boundary
function call(cb: () => i32): i32 { return cb(); }
export function main(): void {
  const value: i32 = 5;
  const indirect: (cb: () => i32) => i32 = call;
  print(`${indirect((): i32 => value)}`);
}
