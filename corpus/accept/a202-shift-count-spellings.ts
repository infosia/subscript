// corpus: accept/a202-shift-count-spellings
// purpose: Checks one count mask for all spellings and compound targets.
// exercises: integer-shift, compound-assignment, evaluation-order
// questions: Q18
// tsc: accepts; js-comparable: yes
class Counter { calls: i32 = 0; }
function index(counter: Counter): i32 { counter.calls += 1; return 0; }
function parameter_i32(x: i32, k: i32): void { print(`${x << k},${x >> k},${x >>> k}`); }
function parameter_u32(x: u32, k: u32): void { print(`${x << k},${x >> k},${x >>> k}`); }
export function main(): void {
  const zero_i32: i32 = 0;
  const zero_u32: u32 = 0;
  const x_i32: i32 = 1;
  print(`${zero_i32 << 31},${x_i32 >> 31},${x_i32 >>> 31}`);
  print(`${x_i32 << 32},${x_i32 >> 32},${x_i32 >>> 32}`);
  print(`${x_i32 << 33},${x_i32 >> 33},${x_i32 >>> 33}`);
  print(`${x_i32 << 64},${x_i32 >> 64},${x_i32 >>> 64}`);
  let k_i32: i32 = 32;
  print(`${x_i32 << k_i32},${x_i32 >> k_i32},${x_i32 >>> k_i32}`);
  parameter_i32(x_i32, k_i32);
  print(`${x_i32 << (16 + 16)},${x_i32 >> (16 + 16)},${x_i32 >>> (16 + 16)}`);
  const x_u32: u32 = 1;
  print(`${zero_u32 << 31},${x_u32 >> 31},${x_u32 >>> 31}`);
  print(`${x_u32 << 32},${x_u32 >> 32},${x_u32 >>> 32}`);
  print(`${x_u32 << 33},${x_u32 >> 33},${x_u32 >>> 33}`);
  print(`${x_u32 << 64},${x_u32 >> 64},${x_u32 >>> 64}`);
  let k_u32: u32 = 32;
  print(`${x_u32 << k_u32},${x_u32 >> k_u32},${x_u32 >>> k_u32}`);
  parameter_u32(x_u32, k_u32);
  print(`${x_u32 << ((16 as u32) + (16 as u32))},${x_u32 >> ((16 as u32) + (16 as u32))},${x_u32 >>> ((16 as u32) + (16 as u32))}`);
  const negative: i32 = -8;
  print(`${negative >> 1},${negative >>> 1},${x_i32 << -1}`);
  const counter = new Counter();
  const values: i32[] = [1];
  values[index(counter)] <<= 32;
  print(`${values[0]},${counter.calls}`);
  values[index(counter)] >>= 33;
  print(`${values[0]},${counter.calls}`);
  values[0] = -8;
  values[index(counter)] >>>= 33;
  print(`${values[0]},${counter.calls}`);
}
