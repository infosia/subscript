// corpus: accept/a260-awaited-handle-outlives-its-holder
// purpose: A waiting await holds its handle after the last script holder is freed.
// exercises: async-function, await, held-handle, try-catch, explicit-free
// questions: Q9, Q34
// tsc: accepts; js-comparable: no Q6: The Context memory API has no JavaScript shim.
class Holder {
  job: Promise<i32>;
  constructor(job: Promise<i32>) { this.job = job; }
}
async function later(fail: boolean): Promise<i32> {
  await Context.suspend();
  print("later:resumed");
  if (fail) { throw new Error("late"); }
  return 5;
}
async function waitOn(holder: Holder): Promise<void> {
  try {
    print(`waited ${await holder.job}`);
  } catch (e) {
    if (e instanceof Error) { print(`caught ${e.message}`); }
  }
}
async function run(fail: boolean): Promise<void> {
  const holder: Holder = new Holder(later(fail));
  const waiter: Promise<void> = waitOn(holder);
  Context.free(holder);
  print("freed holder");
  await waiter;
  print("end");
}
export async function main(): Promise<void> {
  await run(true);
  await run(false);
}
