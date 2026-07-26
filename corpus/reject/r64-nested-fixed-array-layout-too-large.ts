// corpus: reject/r64-nested-fixed-array-layout-too-large
// purpose: Rejects a nested FixedArray whose combined byte size exceeds the limit.
// exercises: nested FixedArray layout multiplication
// expected-error: S100 at the outer FixedArray type

export function main(): void {
  const matrix: FixedArray<FixedArray<u8, 65536>, 65536> = [];
  print(`${matrix.length}`);
}
