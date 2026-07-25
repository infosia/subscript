// corpus: accept/a12-generics-mono
// purpose: Instantiates one generic function and one generic value struct at two types each.
// exercises: generic-function, generic-value-struct, monomorphization
// questions: Q1, Q2, Q12, Q14

function identity<T>(value: T): T {
  return value;
}

@CStruct
class Box<T> {
  value: T;

  constructor(value: T) {
    this.value = value;
  }
}

export function main(): void {
  const integerBox: Box<i32> = new Box<i32>(identity<i32>(42));
  const floatBox: Box<f64> = new Box<f64>(identity<f64>(2.5));
  print(`${integerBox.value}:${floatBox.value}`);
}
