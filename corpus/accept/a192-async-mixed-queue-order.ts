// corpus: accept/a192-async-mixed-queue-order
// purpose: A checkpoint runs ready jobs before the parked frames it promotes, in both creation orders.
// exercises: async-call, held-handle, Context.suspend, ready-queue, parked-promotion, two-async-roots
// questions: §94, Q34, C8
// tsc: accepts; js-comparable: no C8: The corpus runner starts async exports differently.
async function settled(): Promise<void> {
}

async function readyWork(tag: string): Promise<i32> {
  print(`${tag}:ready-start`);
  await settled();
  print(`${tag}:ready-resume`);
  return 1;
}

async function parkedWork(tag: string): Promise<i32> {
  print(`${tag}:parked-start`);
  await Context.suspend();
  print(`${tag}:parked-resume`);
  return 2;
}

export async function main(): Promise<void> {
  const parked: Promise<i32> = parkedWork("A");
  const ready: Promise<i32> = readyWork("A");
  print("A:held");
  const parkedValue: i32 = await parked;
  const readyValue: i32 = await ready;
  print(`A=${parkedValue + readyValue}`);
}

export async function second(): Promise<void> {
  const ready: Promise<i32> = readyWork("B");
  const parked: Promise<i32> = parkedWork("B");
  print("B:held");
  const readyValue: i32 = await ready;
  const parkedValue: i32 = await parked;
  print(`B=${readyValue + parkedValue}`);
}
