// corpus: reject/r144-accessor-increment
// purpose: Rejects an increment through an accessor.
// exercises: accessor-increment
// questions: R37
// tsc: accepts
// expected-error: S100 at the increment

class Value {
  value: i32 = 1;

  get current(): i32 {
    return this.value;
  }

  set current(value: i32) {
    this.value = value;
  }
}

export function main(): void {
  const value: Value = new Value();
  value.current++;
}
