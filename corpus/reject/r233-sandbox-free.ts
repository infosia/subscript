// corpus: reject/r233-sandbox-free
// profile: sandbox
// purpose: Rejects Context.free under the sandbox profile; memory is allocate-only.
// exercises: sandbox-profile, manual-free
// questions: Q6, Q7
// tsc: accepts
// expected-error: S023 at `Context.free`
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
