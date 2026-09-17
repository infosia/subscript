// corpus: accept/a242-sandbox-frame-default
// purpose: Runs the r240 source under the default profile; the 65,536-byte frame limit is the profile's.
// exercises: sandbox-profile-twin, frame-limit
// questions: Q6
// tsc: accepts; js-comparable: no C2: FixedArray has no JavaScript shim.
function probe(input: FixedArray<u8, 65537>): u8 {
  const block: FixedArray<u8, 65537> = input;
  return block[0];
}

export function main(): void {
  print("frame");
}
