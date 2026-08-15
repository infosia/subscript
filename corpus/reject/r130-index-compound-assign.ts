// corpus: reject/r130-index-compound-assign
// purpose: Rejects compound assignment through a class index signature.
// exercises: class-index-signature, compound-index-write
// questions: §58, R29
// expected-error: an index signature rejects `a[i] op= v`

class MutableValues {
  [index: u32]: i32;
  data: i32[] = [1];

  get(index: u32): i32 {
    return this.data[index as i32];
  }

  set(index: u32, value: i32): void {
    this.data[index as i32] = value;
  }
}

export function main(): void {
  const values: MutableValues = new MutableValues();
  const index: u32 = 0;
  values[index] += 2;
}
