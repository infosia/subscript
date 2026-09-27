// corpus: reject/r246-capture-through-loop-back-edge
// purpose: Reject a capture at an escape boundary (compiler.md §118).
// exercises: capture, escape
// questions: Q10
// tsc: accepts
// expected-error: S009 at escape boundary
function named(): i32 { return 0; }
function make(): () => i32 {
  const value: i32 = 5;
  let callback: () => i32 = named;
  for (let i: i32 = 0; i < 2; i++) {
    if (i === 1) { return callback; }
    callback = (): i32 => value;
  }
  return named;
}
export function main(): void { print(`${make()()}`); }
