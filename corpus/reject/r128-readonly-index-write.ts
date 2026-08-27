// corpus: reject/r128-readonly-index-write
// purpose: Rejects a write through a readonly class index signature.
// exercises: class-index-signature, readonly-index-write
// questions: §58, R29
// tsc: rejects TS2542
// expected-error: a readonly index signature rejects `a[i] = v`
class ReadOnlyValues {
  readonly [index: u32]: i32;
  data: i32[] = [1];

  get(index: u32): i32 {
    return this.data[index as i32];
  }
}

export function main(): void {
  const values: ReadOnlyValues = new ReadOnlyValues();
  const index: u32 = 0;
  values[index] = 2;
}
