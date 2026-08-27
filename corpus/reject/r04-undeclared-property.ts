// corpus: reject/r04-undeclared-property
// purpose: Rejects a write to a property absent from a nominal type.
// exercises: rejected-undeclared-property, closed-nominal-type
// questions: none
// tsc: rejects TS2339
// expected-error: nominal types are closed
class Box {
  value: string = "inside";
}

export function main(): void {
  const box: Box = new Box();
  box.extra = "outside";
  print(box.value);
}
