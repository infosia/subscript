// corpus: reject/r143-accessor-compound-assign
// purpose: Rejects a compound assignment through an accessor.
// exercises: accessor-compound-assignment
// questions: R37
// tsc-clean-standalone: verified with node_modules/.bin/tsc against prelude/lang.d.ts; stock TypeScript accepts accessor compound assignment.
// expected-error: S100 at the compound assignment

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
  value.current += 2;
}
