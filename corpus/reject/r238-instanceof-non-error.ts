// corpus: reject/r238-instanceof-non-error
// purpose: Rejects `instanceof` with a class outside the Error family, because only an Error carries a runtime class tag.
// exercises: instanceof, class, exception
// questions: Q9
// tsc: accepts
// expected-error: S100 at the `instanceof` test
class Box {
  value: i32 = 1;
}

export function main(): void {
  const box: Box = new Box();
  if (box instanceof Box) {
    print(`${box.value}`);
  }
}
