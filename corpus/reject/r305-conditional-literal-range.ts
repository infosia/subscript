// corpus: reject/r305-conditional-literal-range
// purpose: A conditional literal must fit the other branch type.
// exercises: conditional-expression, contextual-literal, range-check
// questions: compiler.md §146, collisions.md C4
// tsc: accepts
// expected-error: integer literal 300 out of range for u8
function test(flag: boolean, b: u8): void {
  const value = flag ? b : 300;
}
