// corpus: accept/a96-interop-byte-pairs
// purpose: Passes scalar arrays through adjacent count/pointer C parameters in const-input and mutable-fill directions.
// exercises: scalar-parameter-pair, zero-copy-slice, out-array, u8, u16, foreign-call
// questions: Q13, C4

// compiler.md §27. The mirror collapses each adjacent
// `size_t dataCount, [const] S* data` pair to one `S[]` parameter. The
// consumer receives the input array's pointer and length. Each filler gets
// the destination array's existing length and writes its backing storage in
// place, so the script observes every value after the call with no copy-back.

export function main(): void {
  const input: u8[] = [1, 2, 3, 250];
  print(`${subDeviceSumBytes(input)}`);

  const bytes: u8[] = [0, 0, 0, 0, 0];
  subDeviceFillBytes(bytes);
  print(`${bytes[0]}`);
  print(`${bytes[1]}`);
  print(`${bytes[2]}`);
  print(`${bytes[3]}`);
  print(`${bytes[4]}`);

  const shorts: u16[] = [0, 0, 0, 0];
  subDeviceFillShorts(shorts);
  print(`${shorts[0]}`);
  print(`${shorts[1]}`);
  print(`${shorts[2]}`);
  print(`${shorts[3]}`);
}
