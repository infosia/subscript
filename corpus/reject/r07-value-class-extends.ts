// corpus: reject/r07-value-class-extends
// purpose: Rejects inheritance between value classes.
// exercises: rejected-value-inheritance, value-class
// questions: none
// expected-error: value classes do not inherit

@CStruct
class Base {
  value: i32 = 4;
}

@CStruct
class Derived extends Base {
  extra: i32 = 5;
}

export function main(): void {
  const value: Derived = new Derived();
  print(`${value.value + value.extra}`);
}
