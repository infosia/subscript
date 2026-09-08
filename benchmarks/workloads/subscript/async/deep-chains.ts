// §94.4 workload 3: 400 chains of depth 200.
async function down(n: i32): Promise<i32> {
  if (n <= 0) {
    return 0;
  }
  const value: i32 = await down(n - 1);
  return value + 1;
}

export async function main(): Promise<void> {
  let total: i32 = 0;
  for (let i: i32 = 0; i < 400; i += 1) {
    total += await down(200);
  }
  print(`total=${total}`);
}
