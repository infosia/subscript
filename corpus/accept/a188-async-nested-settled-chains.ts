// corpus: accept/a188-async-nested-settled-chains
// purpose: Two nested chains of settled awaits interleave through the host checkpoint in FIFO order.
// exercises: async-call, direct-await, settled-await, continuation-queue, checkpoint-order
// questions: §94, Q34, C8
// tsc: accepts; js-comparable: yes
async function leaf(tag: string): Promise<i32> {
  print(`${tag}:leaf`);
  return 1;
}

async function mid(tag: string): Promise<i32> {
  print(`${tag}:mid-start`);
  const value: i32 = await leaf(tag);
  print(`${tag}:mid-end`);
  return value + 1;
}

async function top(tag: string): Promise<i32> {
  print(`${tag}:top-start`);
  const value: i32 = await mid(tag);
  print(`${tag}:top-end=${value}`);
  return value + 1;
}

export async function main(): Promise<void> {
  const a: Promise<i32> = top("A");
  const b: Promise<i32> = top("B");
  print("main:mid");
  print(`a=${await a}`);
  print(`b=${await b}`);
}
