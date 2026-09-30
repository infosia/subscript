// corpus: reject/r285-generic-class-method-type-mismatch
// purpose: Rejects a type mismatch in a method of a generic class that no construction instantiates.
// exercises: generic-class, type-mismatch
// questions: compiler.md §135
// tsc: rejects TS2322
// expected-error: S100 at line 11, the string assigned to the i32 field count
class Tally<T> {
  count: i32 = 0;

  reset(): void {
    this.count = "zero";
  }
}

export function main(): void {
  print("ok");
}
