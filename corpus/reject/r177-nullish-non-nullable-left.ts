// corpus: reject/r177-nullish-non-nullable-left
// purpose: Rejects nullish coalescing on a non-nullable left operand.
// exercises: nullish-coalescing, non-nullable-left
// questions: §82.3, C7
// tsc: accepts
// expected-error: S100 at the left operand
class Box {}
export function main(): void {
  const a: Box = new Box();
  const c: Box = new Box();
  const b: Box = a ?? c;
}
