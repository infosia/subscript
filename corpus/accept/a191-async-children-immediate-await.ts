// corpus: accept/a191-async-children-immediate-await
// purpose: Two held children progress together when the holder awaits both without suspending first.
// exercises: async-call, held-handle, Context.suspend, checkpoint-order, blocked-continuation
// questions: §94, Q34, C8
// tsc: accepts; js-comparable: no C8: The Context coroutine API has no JavaScript shim.
async function work(id: i32): Promise<i32> {
  print(`start${id}`);
  await Context.suspend();
  print(`mid${id}`);
  await Context.suspend();
  print(`end${id}`);
  return id * 10;
}

export async function main(): Promise<void> {
  const a: Promise<i32> = work(1);
  const b: Promise<i32> = work(2);
  print("held");
  print("before-await");
  print(`a=${await a}`);
  print(`b=${await b}`);
}
