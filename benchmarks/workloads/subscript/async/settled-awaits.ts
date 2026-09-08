// §94.4 workload 1: 200,000 awaits of an already-completed handle.
async function settled(value: i32): Promise<i32> {
  return value + 1;
}

export async function main(): Promise<void> {
  let total: i32 = 0;
  for (let i: i32 = 0; i < 200000; i += 1) {
    total = await settled(total);
  }
  print(`total=${total}`);
}
