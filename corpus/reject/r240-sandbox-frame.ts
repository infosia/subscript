// corpus: reject/r240-sandbox-frame
// profile: sandbox
// purpose: Rejects a function whose stack frame is over the profile's frame limit of 65,536 bytes.
// exercises: sandbox-profile, frame-limit
// questions: Q6
// tsc: accepts
// expected-error: S027 at the function whose frame is over the limit
function probe(input: FixedArray<u8, 65537>): u8 {
  const block: FixedArray<u8, 65537> = input;
  return block[0];
}

export function main(): void {
  print("frame");
}
