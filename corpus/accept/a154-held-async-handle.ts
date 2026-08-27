// corpus: accept/a154-held-async-handle
// purpose: Holds two async handles, does work between their creation, and awaits each handle later.
// exercises: async-handle, held-handle, delayed-await, poll-order
// questions: §70, C8
// tsc: accepts; js-comparable: no C8: The coroutine API has no JavaScript shim.
let progress: i32 = 0;

async function work(id: i32): Promise<i32> {
  print(`work${id}:start=${progress}`);
  progress += id;
  await Context.suspend();
  print(`work${id}:resume=${progress}`);
  return progress + id * 10;
}

export async function main(): Promise<void> {
  print("main:start");
  const first: Promise<i32> = work(1);
  print("main:between");
  const second: Promise<i32> = work(2);
  print("main:held");

  const firstValue: i32 = await first;
  print(`main:first=${firstValue}`);
  const secondValue: i32 = await second;
  print(`main:second=${secondValue}`);
}
