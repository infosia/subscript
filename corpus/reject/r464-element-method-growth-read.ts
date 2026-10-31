// corpus: reject/r464-element-method-growth-read
// purpose: A method that grows the array of its element reads old storage through this.
// exercises: value-struct, dynamic-array, method-receiver, array-push
// questions: compiler.md §221.1 rule 3
// tsc: accepts
// expected-error: S100, can move a dynamic array of `V`
@ValueType
class V {
  a: i32 = 1;
  b: i32 = 2;

  grow(): i32 {
    for (let i: i32 = 0; i < 5000; i = i + 1) {
      all.push(new V());
    }
    return this.b;
  }
}

const all: V[] = [new V()];

export function main(): void {
  print(`${all[0].grow()}`);
}
