// corpus: reject/r465-element-method-growth-write
// purpose: A method that grows the array of its element writes to old storage through this.
// exercises: value-struct, dynamic-array, method-receiver, array-push
// questions: compiler.md §221.1 rule 3
// tsc: accepts
// expected-error: S100, can move a dynamic array of `V`
@ValueType
class V {
  a: i32 = 1;

  grow(): void {
    for (let i: i32 = 0; i < 5000; i = i + 1) {
      all.push(new V());
    }
    this.a = 7;
  }
}

const all: V[] = [new V()];

export function main(): void {
  all[0].grow();
  print(`${all[0].a}`);
}
