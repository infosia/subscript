// corpus: reject/r466-element-method-growth-for-of
// purpose: A method that grows the array of its element traverses old storage in a for-of over a field of this.
// exercises: value-struct, dynamic-array, method-receiver, fixed-array, for-of
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
    let total: i32 = 9;
    for (const x of this.a) {
      total = total + x;
    }
    return total;
  }
}

const all: V[] = [new V()];

export function main(): void {
  print(`${all[0].sum()}`);
}
