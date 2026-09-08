// corpus: accept/a184-async-start-order
// purpose: Checks call-time completion and one async chain driven by the export runner.
// exercises: async-call, direct-await, held-handle, start-order, cached-result
// questions: Q34
// tsc: accepts; js-comparable: yes
async function leaf(): Promise<void> {
  print("leaf");
}

async function chain(): Promise<void> {
  print("chain:start");
  await leaf();
  print("chain:end");
}

async function quick(): Promise<i32> {
  print("quick");
  return 42;
}

export async function main(): Promise<void> {
  await chain();
  const handle: Promise<i32> = quick();
  print("after-create");
  const result: i32 = await handle;
  print(`result=${result}`);
  const cached: i32 = await handle;
  print(`cached=${cached}`);
}
