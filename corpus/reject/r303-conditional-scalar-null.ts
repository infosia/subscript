// corpus: reject/r303-conditional-scalar-null
// purpose: A conditional cannot join a scalar and null.
// exercises: conditional-expression, branch-join
// questions: compiler.md §146, collisions.md C7
// tsc: accepts
// expected-error: S100 at the conditional with incompatible branch types
export function main(): void {
  const value: i32 = 1;
  const joined = true ? value : null;
}
