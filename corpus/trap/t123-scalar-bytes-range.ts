// corpus: trap/t123-scalar-bytes-range
// purpose: A scalar byte read traps when the bytes of the scalar do not fit.
// exercises: Context.fromBytes, scalar-bytes, byte-range, trap-stop
// questions: R34, compiler.md §360
// tier-policy: both tiers trap with kind 1
// expected-trap: byte range at offset 6 with size 4 exceeds array length 8

export function main(): void {
  const bytes: u8[] = [1, 2, 3, 4, 5, 6, 7, 8];
  const value: u32 = Context.fromBytes<u32>(bytes, 6);
  print(`${value}`);
}
