// corpus: reject/r141-value-class-write-accessor
// purpose: Rejects a write accessor on a value class.
// exercises: value-class-write-accessor
// questions: R37
// tsc-clean-standalone: verified with node_modules/.bin/tsc against prelude/lang.d.ts; stock TypeScript accepts a value-class write accessor.
// expected-error: S100 at the write accessor

@CStruct
class Value {
  value: i32 = 0;

  get current(): i32 {
    return this.value;
  }

  set current(value: i32) {
    this.value = value;
  }
}

export function main(): void {}
