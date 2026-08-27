// corpus: accept/a93-async-chain
// purpose: Pins nested async suspension propagation and fulfilled values.
// exercises: async-function, direct-await, Context.suspend, async-chain
// questions: Q34, C8
// tsc: accepts; js-comparable: no C8: The coroutine API has no JavaScript shim.
let polls: i32 = 0;

async function leaf(): Promise<i32> {
  print("leaf:start");
  while (polls < 2) {
    print(`leaf:poll=${polls}`);
    polls += 1;
    await Context.suspend();
    print(`leaf:resume=${polls}`);
  }
  print(`leaf:done=${polls}`);
  return polls + 10;
}

async function middle(): Promise<i32> {
  print("middle:start");
  const value: i32 = await leaf();
  print(`middle:value=${value}`);
  return value + 20;
}

export async function main(): Promise<void> {
  print("main:start");
  const value: i32 = await middle();
  print(`main:value=${value}`);
}
