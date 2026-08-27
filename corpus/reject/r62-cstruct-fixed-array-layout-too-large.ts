// corpus: reject/r62-cstruct-fixed-array-layout-too-large
// purpose: Rejects a CStruct field whose FixedArray exceeds the aggregate byte limit.
// exercises: CStruct layout, FixedArray byte size
// questions: Q3
// tsc: rejects TS2564
// expected-error: S100 at the FixedArray type
@CStruct
class Big {
  data: FixedArray<u8, 4294967295>;
}

export function main(): void {
  const b: Big = new Big();
  print(`${b.data.length}`);
}
