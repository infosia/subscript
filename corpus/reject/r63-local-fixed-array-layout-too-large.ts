// corpus: reject/r63-local-fixed-array-layout-too-large
// purpose: Rejects a local FixedArray whose byte size exceeds the aggregate limit.
// exercises: local aggregate storage, FixedArray byte size
// questions: Q3
// tsc: accepts
// expected-error: S100 at the FixedArray type
export function main(): void {
  const data: FixedArray<u8, 2147483648> = [];
  print(`${data.length}`);
}
