// corpus: accept/a185-async-settled-await-order
// purpose: Pins the settled-await order that the host checkpoint produces.
// exercises: async-call, direct-await, held-handle, settled-await
// questions: §94, Q34
// tsc: accepts; js-comparable: yes
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
