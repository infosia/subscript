// corpus: accept/a185-async-settled-await-order
// purpose: Pins the settled-await order without a microtask queue.
// exercises: async-call, direct-await, held-handle, settled-await
// questions: Q34
// tsc: accepts; js-comparable: no C16: Settled awaits continue in the same step.
// node-order: start1 leaf start2 leaf end1 end2
// node-order: outer:start inner main:mid outer:end
async function leaf(): Promise<void> {
  print("leaf");
}

async function work(id: i32): Promise<void> {
  print(`start${id}`);
  await leaf();
  print(`end${id}`);
}

async function inner(): Promise<void> {
  print("inner");
}

async function outer(): Promise<void> {
  print("outer:start");
  await inner();
  print("outer:end");
}

export async function main(): Promise<void> {
  const first: Promise<void> = work(1);
  const second: Promise<void> = work(2);
  await first;
  await second;
  const held: Promise<void> = outer();
  print("main:mid");
  await held;
}
