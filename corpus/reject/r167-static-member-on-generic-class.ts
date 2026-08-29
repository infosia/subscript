// corpus: reject/r167-static-member-on-generic-class
// purpose: Rejects a static member on a generic class.
// exercises: static-field, generic-class
// questions: §71
// tsc: accepts
// expected-error: S100 at static
class Box<T> {
  static count: i32 = 0;
  value: T;

  constructor(value: T) {
    this.value = value;
  }
}

export function main(): void {
  const box: Box<i32> = new Box<i32>(1);
  print(`${box.value}`);
}
