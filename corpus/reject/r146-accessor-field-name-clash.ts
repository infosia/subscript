// corpus: reject/r146-accessor-field-name-clash
// purpose: Rejects a field and an accessor with the same member name.
// exercises: accessor-field-name-clash
// questions: R37
// tsc: rejects TS2300
// expected-error: S017 at the second declaration
class Value {
  current: i32 = 1;

  get current(): i32 {
    return 2;
  }
}

export function main(): void {}
