// corpus: accept/a50-narrow-callbacks-shifts
// purpose: Pins callback argument extension for every narrow integer array
//          kind and modulo-width shifts for every integer width on both tiers.
// exercises: array-callback-signedness, masked-shifts, f16-literal-rounding
// questions: Q18, Q22, Q23, C4
// tsc: accepts; js-comparable: no C3 Q23: Narrow numeric operations produce different output.
export function main(): void {
  const signedBytes: i8[] = [-128, 3, -1, 127];
  print(`i8 map ${signedBytes.map((v: i8): i32 => v as i32).join(",")}`);
  print(`i8 reduce ${signedBytes.reduce((a: i32, v: i8): i32 => a + (v as i32), 0)}`);
  signedBytes.sort((a: i8, b: i8): i32 => (a as i32) - (b as i32));
  print(`i8 sort ${signedBytes.join(",")}`);
  const negativeBytes: i8[] = [-1];
  negativeBytes.forEach((v: i8): void => {
    print(`i8 forEach ${v as i32}`);
  });
  print(`i8 findIndex ${negativeBytes.findIndex((v: i8): boolean => v === -1)}`);

  const unsignedBytes: u8[] = [255, 3, 1, 127];
  print(`u8 map ${unsignedBytes.map((v: u8): i32 => v as i32).join(",")}`);
  print(`u8 reduce ${unsignedBytes.reduce((a: i32, v: u8): i32 => a + (v as i32), 0)}`);
  unsignedBytes.sort((a: u8, b: u8): i32 => (a as i32) - (b as i32));
  print(`u8 sort ${unsignedBytes.join(",")}`);
  const highBytes: u8[] = [255];
  highBytes.forEach((v: u8): void => {
    print(`u8 forEach ${v as i32}`);
  });
  print(`u8 findIndex ${highBytes.findIndex((v: u8): boolean => v === 255)}`);

  const signedShorts: i16[] = [-32768, 3, -2, 32767];
  print(`i16 map ${signedShorts.map((v: i16): i32 => v as i32).join(",")}`);
  print(`i16 reduce ${signedShorts.reduce((a: i32, v: i16): i32 => a + (v as i32), 0)}`);
  signedShorts.sort((a: i16, b: i16): i32 => (a as i32) - (b as i32));
  print(`i16 sort ${signedShorts.join(",")}`);
  const negativeShorts: i16[] = [-2];
  negativeShorts.forEach((v: i16): void => {
    print(`i16 forEach ${v as i32}`);
  });
  print(`i16 findIndex ${negativeShorts.findIndex((v: i16): boolean => v === -2)}`);

  const unsignedShorts: u16[] = [65535, 3, 1, 32767];
  print(`u16 map ${unsignedShorts.map((v: u16): i32 => v as i32).join(",")}`);
  print(`u16 reduce ${unsignedShorts.reduce((a: i32, v: u16): i32 => a + (v as i32), 0)}`);
  unsignedShorts.sort((a: u16, b: u16): i32 => (a as i32) - (b as i32));
  print(`u16 sort ${unsignedShorts.join(",")}`);
  const highShorts: u16[] = [65535];
  highShorts.forEach((v: u16): void => {
    print(`u16 forEach ${v as i32}`);
  });
  print(`u16 findIndex ${highShorts.findIndex((v: u16): boolean => v === 65535)}`);

  const i8Value: i8 = -2;
  const i8Amount: i8 = 9;
  print(`shift i8 ${i8Value << i8Amount} ${i8Value >> i8Amount} ${i8Value >>> i8Amount}`);
  const u8Value: u8 = 255;
  const u8Amount: u8 = 9;
  print(`shift u8 ${u8Value << u8Amount} ${u8Value >> u8Amount} ${u8Value >>> u8Amount}`);

  const i16Value: i16 = -2;
  const i16Amount: i16 = 17;
  print(`shift i16 ${i16Value << i16Amount} ${i16Value >> i16Amount} ${i16Value >>> i16Amount}`);
  const u16Value: u16 = 65535;
  const u16Amount: u16 = 17;
  print(`shift u16 ${u16Value << u16Amount} ${u16Value >> u16Amount} ${u16Value >>> u16Amount}`);

  const i32Value: i32 = -2;
  const i32Amount: i32 = 33;
  print(`shift i32 ${i32Value << i32Amount} ${i32Value >> i32Amount} ${i32Value >>> i32Amount}`);
  const u32Value: u32 = 4294967295;
  const u32Amount: u32 = 33;
  print(`shift u32 ${u32Value << u32Amount} ${u32Value >> u32Amount} ${u32Value >>> u32Amount}`);

  const i64Value: i64 = -2;
  const i64Amount: i64 = 65;
  print(`shift i64 ${i64Value << i64Amount} ${i64Value >> i64Amount} ${i64Value >>> i64Amount}`);
  const u64Zero: u64 = 0;
  const u64One: u64 = 1;
  const u64Value: u64 = u64Zero - u64One;
  const u64Amount: u64 = 65;
  print(`shift u64 ${u64Value << u64Amount} ${u64Value >> u64Amount} ${u64Value >>> u64Amount}`);

  const roundedFinite: f16 = 65505.0;
  print(`f16 edge ${roundedFinite}`);
}
