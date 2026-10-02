// corpus: reject/r304-conditional-numeric-widths
// purpose: A conditional cannot join different numeric widths.
// exercises: conditional-expression, branch-join
// questions: compiler.md §146, collisions.md C3
// tsc: accepts
// expected-error: S100 at the conditional with incompatible branch types
export function main(): void {
  const integer: i32 = 1;
  const float: f64 = 2.5;
  const joined = true ? integer : float;
}
