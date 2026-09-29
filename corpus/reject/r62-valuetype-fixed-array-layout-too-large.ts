// corpus: reject/r62-valuetype-fixed-array-layout-too-large
// purpose: Rejects a ValueType field whose FixedArray exceeds the aggregate byte limit.
// exercises: ValueType layout, FixedArray byte size
// questions: Q3
// tsc: rejects TS2564
// expected-error: S100 at the FixedArray type
@ValueType
class Big {
  data: FixedArray<u8, 4294967295>;
}

export function main(): void {
  const b: Big = new Big();
  print(`${b.data.length}`);
}
