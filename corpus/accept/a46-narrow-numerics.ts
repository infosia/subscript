// corpus: accept/a46-narrow-numerics
// purpose: Declares, converts, formats, and computes at each narrow numeric width.
// exercises: narrow-numerics, explicit-conversion, wrapping-arithmetic, bitwise
// questions: Q1, Q14, Q18, Q23, C3, C4

export function main(): void {
  const signedByte: i8 = -12;
  const unsignedByte: u8 = 250;
  const signedShort: i16 = -30000;
  const unsignedShort: u16 = 60000;
  const half: f16 = 1.5;
  print(`${signedByte} ${unsignedByte} ${signedShort} ${unsignedShort} ${half}`);

  const wrappedByte: u8 = (300 as i32) as u8;
  const signedBits: i8 = (255 as u16) as i8;
  const wrappedShort: i16 = (65530 as u32) as i16;
  const widened: i32 = signedByte as i32;
  const wrappedUnsignedShort: u16 = (-1 as i32) as u16;
  const truncatedFromFloat: i8 = (12.75 as f32) as i8;
  const narrowToFloat: f32 = unsignedShort as f32;
  const halfFromF32: f16 = (1.25 as f32) as f16;
  const halfBack: f64 = halfFromF32 as f64;
  print(
    `${wrappedByte} ${signedBits} ${wrappedShort} ${widened} ${wrappedUnsignedShort} ${truncatedFromFloat} ${narrowToFloat} ${halfBack}`,
  );

  const wrappedSum: i8 = 120 + 10;
  const high: u8 = 240;
  const low: u8 = 15;
  const bits: u8 = high | low;
  const one: u8 = 1;
  const shifted: u8 = bits << one;
  print(`${wrappedSum} ${bits} ${shifted}`);
}
