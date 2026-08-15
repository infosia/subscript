// corpus: accept/a135-f32-bits
// purpose: Reads and writes binary32 bit patterns through Math.
// exercises: math-f32-to-bits, math-f32-from-bits, canonical-nan
// questions: §17, R28

export function main(): void {
  print(`one ${Math.f32ToBits(1)}`);

  const onePointOneBits: u32 = Math.f32ToBits(1.1);
  const roundedBits: u32 = Math.f32ToBits(Math.fround(1.1));
  print(`fround agreement ${onePointOneBits} ${roundedBits} ${onePointOneBits === roundedBits}`);

  print(`negative zero ${Math.f32ToBits(-0)}`);
  print(`positive infinity ${Math.f32ToBits(Number.POSITIVE_INFINITY)}`);
  print(`overflow ${Math.f32ToBits(1e300)}`);
  print(`nan ${Math.f32ToBits(Number.NaN)}`);

  const subnormal: f64 = Math.f32FromBits(1);
  print(`subnormal ${subnormal}`);
  print(`round trip ${Math.f32ToBits(subnormal)}`);
  print(`nan pattern ${Math.f32ToBits(Math.f32FromBits(2139095041))}`);
}
