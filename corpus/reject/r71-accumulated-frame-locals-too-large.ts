// corpus: reject/r71-accumulated-frame-locals-too-large
// purpose: Rejects two individually valid local slots whose accumulated stack frame exceeds the frame limit.
// exercises: accumulated stack-frame layout, multiple aggregate locals
// questions: none
// tsc: accepts
// expected-error: S100 at the second local declaration
function probe(
  leftInput: FixedArray<u8, 1100000000>,
  rightInput: FixedArray<u8, 1100000000>,
): void {
  const left: FixedArray<u8, 1100000000> = leftInput;
  const right: FixedArray<u8, 1100000000> = rightInput;
}

export function main(): void {}
