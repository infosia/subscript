// corpus: reject/r294-loose-equality
// purpose: Rejects loose equality on concrete numeric operands.
// exercises: generic-body, type-parameter
// questions: collisions.md C20
// tsc: accepts
// expected-error: S100 at line 11, Rejects loose equality on concrete numeric operands.


export function main(): void {
  const x: i32 = 1, y: i32 = 2;
  const equal = x == y;
}
