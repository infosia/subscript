// corpus: reject/r482-element-nested-array-inner-move
// purpose: On a nested dynamic array the receiver has the inner element type, so a move of an array of that type is rejected.
// exercises: value-struct, dynamic-array, method-receiver, nested-array
// questions: compiler.md §221.1 rule 1
// tsc: accepts
// expected-error: S100, can move a dynamic array of `V`
@ValueType
class V {
  a: i32 = 1;

  grow(row: V[]): void {
    row.push(new V());
  }
}

export function main(): void {
  const rows: V[][] = [[new V()]];
  rows[0][0].grow(rows[0]);
  print(`${rows[0].length}`);
}
