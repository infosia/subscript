// corpus: accept/a203-shift-count-widths
// purpose: Checks narrow and 64-bit masks with signedness controls.
// exercises: integer-shift, narrow-numerics, signedness
// questions: Q18
// tsc: accepts; js-comparable: no Q18: A JavaScript number shifted by 8 gives 256 where u8 gives 1.
export function main(): void {
  const x_i8: i8 = 1;
  const high_i8: i8 = -8;
  print(`${x_i8 << 7},${high_i8 >> 7},${high_i8 >>> 7}`);
  print(`${x_i8 << 8},${high_i8 >> 8},${high_i8 >>> 8}`);
  print(`${x_i8 << 9},${high_i8 >> 9},${high_i8 >>> 9}`);
  print(`${x_i8 << 16},${high_i8 >> 16},${high_i8 >>> 16}`);
  print(`${x_i8 << -1}`);
  const x_u8: u8 = 1;
  const high_u8: u8 = 248;
  print(`${x_u8 << 7},${high_u8 >> 7},${high_u8 >>> 7}`);
  print(`${x_u8 << 8},${high_u8 >> 8},${high_u8 >>> 8}`);
  print(`${x_u8 << 9},${high_u8 >> 9},${high_u8 >>> 9}`);
  print(`${x_u8 << 16},${high_u8 >> 16},${high_u8 >>> 16}`);
  const x_i16: i16 = 1;
  const high_i16: i16 = -8;
  print(`${x_i16 << 15},${high_i16 >> 15},${high_i16 >>> 15}`);
  print(`${x_i16 << 16},${high_i16 >> 16},${high_i16 >>> 16}`);
  print(`${x_i16 << 17},${high_i16 >> 17},${high_i16 >>> 17}`);
  print(`${x_i16 << 32},${high_i16 >> 32},${high_i16 >>> 32}`);
  print(`${x_i16 << -1}`);
  const x_u16: u16 = 1;
  const high_u16: u16 = 65528;
  print(`${x_u16 << 15},${high_u16 >> 15},${high_u16 >>> 15}`);
  print(`${x_u16 << 16},${high_u16 >> 16},${high_u16 >>> 16}`);
  print(`${x_u16 << 17},${high_u16 >> 17},${high_u16 >>> 17}`);
  print(`${x_u16 << 32},${high_u16 >> 32},${high_u16 >>> 32}`);
  const x_i64: i64 = 1;
  const high_i64: i64 = -8;
  print(`${x_i64 << 63},${high_i64 >> 63},${high_i64 >>> 63}`);
  print(`${x_i64 << 64},${high_i64 >> 64},${high_i64 >>> 64}`);
  print(`${x_i64 << 65},${high_i64 >> 65},${high_i64 >>> 65}`);
  print(`${x_i64 << 128},${high_i64 >> 128},${high_i64 >>> 128}`);
  print(`${x_i64 << -1}`);
  const x_u64: u64 = 1;
  const high_u64: u64 = 18446744073709551608;
  print(`${x_u64 << 63},${high_u64 >> 63},${high_u64 >>> 63}`);
  print(`${x_u64 << 64},${high_u64 >> 64},${high_u64 >>> 64}`);
  print(`${x_u64 << 65},${high_u64 >> 65},${high_u64 >>> 65}`);
  print(`${x_u64 << 128},${high_u64 >> 128},${high_u64 >>> 128}`);
  print(`${x_u64 << 0xFFFFFFFFFFFFFFFF},${high_u64 >> 0xFFFFFFFFFFFFFFFF},${high_u64 >>> 0xFFFFFFFFFFFFFFFF}`);
}
