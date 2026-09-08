// §94.4 workload 2: 20 rounds of 2,000 independently held handles.
async function work(id: i32): Promise<i32> {
  await Context.suspend();
  return id;
}

export async function main(): Promise<void> {
  let total: i32 = 0;
  for (let round: i32 = 0; round < 20; round += 1) {
    const handles: Promise<i32>[] = [];
    for (let i: i32 = 0; i < 2000; i += 1) {
      handles.push(work(i));
    }
    for (let i: i32 = 0; i < handles.length; i += 1) {
      total += await handles[i];
    }
  }
  print(`total=${total}`);
}
