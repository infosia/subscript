// corpus: reject/r67-frame-local-boundary-too-large
// purpose: Rejects the first FixedArray size whose ABI-rounded stack frame reaches 2^31.
// exercises: accumulated stack-frame layout, final 16-byte ABI alignment
// questions: Q3
// tsc: accepts
// expected-error: S100 at the local declaration
function probe(input: FixedArray<u8, 2147483633>): void {
  const data: FixedArray<u8, 2147483633> = input;
}

export function main(): void {}
