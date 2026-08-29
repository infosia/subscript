// corpus: reject/r165-this-in-static-method
// purpose: Rejects this in a static method.
// exercises: static-method, static-this
// questions: §71
// tsc: accepts
// expected-error: S100 at this
class C {
  static value: i32 = 1;

  static read(): i32 {
    return this.value;
  }
}

export function main(): void {}
