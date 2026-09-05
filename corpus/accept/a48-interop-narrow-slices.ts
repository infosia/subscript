// corpus: accept/a48-interop-narrow-slices
// interpreter: no — calls the synthetic native interop library
// purpose: Passes every narrow numeric array zero-copy to a typed C slice facade.
// exercises: narrow-numerics, zero-copy-slice, contiguous-array, foreign-call
// questions: Q4, Q23, C3, C4
// tsc: accepts; js-comparable: no Q13: The host C boundary has no JavaScript shim.
export function main(): void {
  const unsignedBytes: u8[] = [1, 2, 255];
  const signedBytes: i8[] = [1, -2, 3];
  const unsignedShorts: u16[] = [1, 1000, 65535];
  const signedShorts: i16[] = [-1, 2, -3];
  const halves: f16[] = [1.0, -0.0, 65504.0];

  print(`${subSliceChecksumU8(unsignedBytes)}`);
  print(`${subSliceChecksumI8(signedBytes)}`);
  print(`${subSliceChecksumU16(unsignedShorts)}`);
  print(`${subSliceChecksumI16(signedShorts)}`);
  print(`${subSliceChecksumF16(halves)}`);
}
