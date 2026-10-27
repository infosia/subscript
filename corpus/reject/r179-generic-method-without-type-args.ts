// corpus: reject/r179-generic-method-without-type-args
// purpose: Rejects a generic method call whose type parameter has no candidate.
// exercises: generic-method, type-arguments
// questions: §82.4, §64, compiler.md §197
// tsc: accepts
// expected-error: S100, cannot infer type parameter `T` of `Box.empty`: no candidate
class Box {
  empty<T>(): T[] {
    return [];
  }
}

export function main(): void {
  const box: Box = new Box();
  print(`${box.empty().length}`);
}
