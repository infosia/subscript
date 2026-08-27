// corpus: reject/r142-readonly-accessor-write
// purpose: Rejects a write through a read-only accessor.
// exercises: read-only-accessor-write
// questions: R37
// tsc: rejects TS2540
// expected-error: S100 at the assignment
class Value {
  get current(): i32 {
    return 1;
  }
}

export function main(): void {
  const value: Value = new Value();
  value.current = 2;
}
