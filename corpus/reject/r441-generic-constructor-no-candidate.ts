// corpus: reject/r441-generic-constructor-no-candidate
// purpose: A constructor type parameter with no argument and no context has no candidate.
// exercises: generics, type-inference, constructor
// questions: compiler.md §197
// tsc: accepts
// expected-error: S100, cannot infer type parameter `T` of `Bag`: no candidate
class Bag<T> {
  items: T[] = [];
}

export function main(): void {
  const bag = new Bag();
  print(`${bag.items.length}`);
}
