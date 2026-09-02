// corpus: reject/r173-compound-write-as-value
// purpose: Rejects an accessor compound write used as a value.
// exercises: accessor-compound-assignment, value-position-write
// questions: R39.3, §82.1
// tsc: accepts
// expected-error: S100 at the compound assignment

class Counter {
  value: i32 = 1;

  get v(): i32 {
    return this.value;
  }

  set v(value: i32) {
    this.value = value;
  }
}

export function main(): void {
  const counter: Counter = new Counter();
  const changed: i32 = (counter.v += 1);
  print(`${changed}`);
}
