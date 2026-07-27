// corpus: reject/r72-for-of-user-class
// purpose: Rejects user-defined iteration under invariant 5.
// exercises: for-of-closed-list, symbol-iterator-nongoal
// questions: Q30
// expected-error: a user class needs Symbol.iterator and stock tsc rejects it

class Bag {
  value: i32 = 1;
}

export function main(): void {
  const bag: Bag = new Bag();
  for (const value of bag) {
    print(`${value}`);
  }
}
