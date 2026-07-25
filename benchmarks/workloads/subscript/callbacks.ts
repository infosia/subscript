// benchmark: callbacks
// Indexed map/filter/reduce over 1000000 signed i32 values seeded by the fixed
// LCG state = state*1664525 + 1013904223, repeated 20 times. The filter
// removes exactly 250000 values per round. Every arithmetic expression is i32
// and therefore wraps under C3. Checksum: i32 sum of the reduce results.

const COUNT: i32 = 1000000;
const ROUNDS: i32 = 20;

function mapValue(value: i32, index: i32): i32 {
  return value + index;
}

function keepValue(value: i32, index: i32): boolean {
  return ((value ^ index) & 3) !== 0;
}

function reduceValue(acc: i32, value: i32, index: i32): i32 {
  acc = acc + value;
  return acc + index;
}

export function main(): void {
  let state: i32 = 0x12345678;
  const input: i32[] = [];
  for (let i: i32 = 0; i < COUNT; i += 1) {
    state = state * 1664525 + 1013904223;
    input.push(state);
  }

  let checksum: i32 = 0;
  for (let round: i32 = 0; round < ROUNDS; round += 1) {
    const mapped: i32[] = input.map(mapValue);
    const filtered: i32[] = mapped.filter(keepValue);
    const reduced: i32 = filtered.reduce(reduceValue, 0);
    checksum = checksum + reduced;
  }
  print(`${checksum}`);
}
