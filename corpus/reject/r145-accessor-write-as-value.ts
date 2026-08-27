// corpus: reject/r145-accessor-write-as-value
// purpose: Rejects an accessor write that appears in a value position.
// exercises: accessor-write-as-value
// questions: R37
// tsc: accepts
// expected-error: S100 at the assignment

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
  const result: i32 = value.current = 2;
  print(`${result}`);
}
