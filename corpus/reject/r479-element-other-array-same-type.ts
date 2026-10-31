// corpus: reject/r479-element-other-array-same-type
// purpose: The type decides, so a method that pushes to another array of the element type is rejected on an element.
// exercises: value-struct, dynamic-array, method-receiver, type-rule
// questions: compiler.md §221.1 rule 5
// tsc: accepts
// expected-error: S100, can move a dynamic array of `V`
@ValueType
class V {
  a: i32 = 1;

  note(other: V[]): void {
    other.push(new V());
  }
}

export function main(): void {
  const all: V[] = [new V()];
  const other: V[] = [];
  all[0].note(other);
  print(`${other.length}`);
}
