// corpus: reject/r467-element-method-growth-reduce
// purpose: A method that grows the array of its element traverses old storage in a reduce over a field of this.
// exercises: value-struct, dynamic-array, method-receiver, fixed-array, reduce
// questions: compiler.md §221.1 rule 3
// tsc: accepts
// expected-error: S100, can move a dynamic array of `V`
@ValueType
class V {
  a: FixedArray<i32, 2> = [1, 2];

  sum(): i32 {
    for (let i: i32 = 0; i < 5000; i = i + 1) {
      all.push(new V());
    }
    return this.a.reduce((acc: i32, x: i32): i32 => acc + x, 9);
  }
}

const all: V[] = [new V()];

export function main(): void {
  print(`${all[0].sum()}`);
}
