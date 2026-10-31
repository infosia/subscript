// corpus: reject/r481-element-length-store
// purpose: An assignment to length moves an element.
// exercises: value-struct, dynamic-array, method-receiver, array-length
// questions: compiler.md §221.1 rule 2
// tsc: accepts
// expected-error: S100, can move a dynamic array of `V`
@ValueType
class V {
  a: i32 = 1;

  clear(list: V[]): void {
    list.length = 0;
  }
}

export function main(): void {
  const all: V[] = [new V()];
  all[0].clear(all);
}
