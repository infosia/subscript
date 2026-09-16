// corpus: accept/a235-sandbox-free-default
// purpose: Runs the r233 source under the default profile; Context.free stays callable there.
// exercises: sandbox-profile-twin, manual-free
// questions: Q6, Q7
// tsc: accepts; js-comparable: no Q6: The Context memory API has no JavaScript shim.
class Counter {
  value: i32;

  constructor(value: i32) {
    this.value = value;
  }
}

export function main(): void {
  const counter: Counter = new Counter(10);
  print(`${counter.value}`);
  Context.free(counter);
}
