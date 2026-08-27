// corpus: reject/r94-descriptor-method
// purpose: Rejects methods in a data-only descriptor class.
// exercises: descriptor-class, method
// questions: Q33
// tsc: rejects TS2322
// expected-error: S100 at the method name
@Descriptor
class InvalidDescriptor {
  value?: i32 = 1;
  read(): i32 {
    return this.value;
  }
}

export function main(): void {}
