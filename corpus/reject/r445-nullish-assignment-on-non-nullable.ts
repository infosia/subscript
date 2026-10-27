// corpus: reject/r445-nullish-assignment-on-non-nullable
// purpose: `??=` needs a nullable place.
// exercises: nullish-assignment
// questions: compiler.md §199.1 rule 6
// tsc: accepts
// expected-error: S100, the left operand of `??=` has type `Box`, which is not nullable
class Box {
  v: i32 = 1;
}
export function main(): void {
  let b: Box = new Box();
  b ??= new Box();
  print(`${b.v}`);
}
