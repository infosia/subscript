// corpus: accept/a15-manual-lifetime
// purpose: Allocates, uses, and manually frees a reference-class instance.
// exercises: reference-class, allocation, manual-free
// questions: Q1, Q2, Q6, Q12

class Counter {
  value: i32;

  constructor(value: i32) {
    this.value = value;
  }

  increment(): void {
    this.value += 1;
  }
}

export function main(): void {
  const counter: Counter = new Counter(10);
  counter.increment();
  print(`${counter.value}`);
  Context.free(counter);
}
