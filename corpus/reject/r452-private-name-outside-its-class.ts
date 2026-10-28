// corpus: reject/r452-private-name-outside-its-class
// purpose: A #name member is private, so code outside its class cannot read it.
// exercises: private-names
// questions: compiler.md §206.1 rule 2
// tsc: rejects TS18013
// expected-error: S100, member `#count` is private in class `Counter`
class Counter {
  #count: i32 = 0;
}

export function main(): void {
  const c = new Counter();
  print(`${c.#count}`);
}
